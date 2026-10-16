//! Typeform toolkit (REST API `https://api.typeform.com`).
//! Action set modelled on Activepieces `piece-typeform` (MIT). Every request in
//! this file is the plain REST call the TypeScript piece makes through
//! `typeformCommon.typeformRequest` (`lib/common/index.ts`) or `httpClient.sendRequest`.
//!
//! Auth: OAuth2 exactly as `src/lib/auth.ts` (`https://admin.typeform.com/oauth/authorize`
//! -> `https://api.typeform.com/oauth/token`, the same twelve scopes). `AuthScheme::Bearer`
//! is offered as an alternative because the piece's own transport sends the access token
//! as `Authorization: Bearer <token>` (`AuthenticationType.BEARER_TOKEN`).
//!
//! Pagination follows the piece: `page` / `page_size` for the paged collections
//! (`pageQuery`, max 200) and `page_size` for responses (max 1000). Both are only sent
//! when the caller passes them, because `compactQuery` drops undefined values.
//!
//! Actions that need a read-modify-write round trip against a second endpoint before
//! their write (`duplicate_form`, `update_choice_options`, `update_form_messages`,
//! `update_theme`, `delete_theme`, `delete_workspace`) are not ported: an adapter emits
//! exactly one provider request per tool call, so their payload cannot be derived from
//! the tool arguments alone. Their endpoints are documented in the module test.
use anyhow::{bail, Result};
use serde_json::{json, Map, Value};

use super::common::*;
use super::{path_segment, ToolkitAdapter};
use crate::http::{HttpRequest, HttpResponse};
use crate::model::*;

pub const SLUG: &str = "typeform";

const FORMS_R: &str = "forms:read";
const FORMS_W: &str = "forms:write";
const THEMES_R: &str = "themes:read";
const THEMES_W: &str = "themes:write";
const WORKSPACES_R: &str = "workspaces:read";
const WORKSPACES_W: &str = "workspaces:write";
const RESPONSES_R: &str = "responses:read";
const RESPONSES_W: &str = "responses:write";
const IMAGES_R: &str = "images:read";
const IMAGES_W: &str = "images:write";
const ACCOUNTS_R: &str = "accounts:read";

/// `maxPageSize` of the piece.
const MAX_PAGE_SIZE: u64 = 200;
/// `list-responses.ts` clamps the response page size to 1000.
const MAX_RESPONSE_PAGE_SIZE: u64 = 1000;
/// `delete-responses.ts` rejects more than 1000 ids.
const MAX_DELETE_RESPONSE_IDS: usize = 1000;

/// `READ_ONLY_FORM_KEYS` — dropped by `toFormPayload` before a `PUT /forms/{id}`.
const READ_ONLY_FORM_KEYS: &[&str] = &[
    "id",
    "_links",
    "self",
    "created_at",
    "last_updated_at",
    "published_at",
];

// ---------------------------------------------------------------- schema fragments

fn choice(desc: &str, options: &[&str]) -> Value {
    json!({"type": "string", "description": desc, "enum": options})
}

/// Free-form JSON object property (`Property.Json` in the piece).
fn json_object(desc: &str) -> Value {
    json!({"type": "object", "description": desc})
}

/// JSON array of objects (`Property.Json` holding the Typeform `fields` array).
fn json_object_array(desc: &str) -> Value {
    json!({"type": "array", "items": {"type": "object"}, "description": desc})
}

/// `page` / `page_size` of `pageQuery`, only sent when the caller sets them.
fn page_props() -> Vec<(&'static str, Value)> {
    vec![
        (
            "page",
            int("Seitenzummer, beginnend bei 1.", 1, 100_000, 1),
        ),
        (
            "page_size",
            int(
                "Einträge pro Seite, 1 bis 200.",
                1,
                MAX_PAGE_SIZE,
                MAX_PAGE_SIZE,
            ),
        ),
    ]
}

// ---------------------------------------------------------------- helpers

/// Numeric argument clamped like the piece's `Math.min/max` guards; `None` when absent
/// so the parameter is left out of the query (`compactQuery` drops undefined values).
fn opt_num(args: &Value, key: &str, min: u64, max: u64) -> Option<u64> {
    args.get(key)
        .and_then(Value::as_u64)
        .map(|n| n.clamp(min, max))
}

/// Trimmed string argument (`propsValue.x?.trim()` in the piece).
fn trimmed<'a>(args: &'a Value, key: &str) -> Option<&'a str> {
    args.get(key)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|v| !v.is_empty())
}

/// `toStringList({ values })` + `join(',')` of the piece.
fn joined_list(args: &Value, key: &str) -> Option<String> {
    let list = str_vec(args, key);
    if list.is_empty() {
        None
    } else {
        Some(list.join(","))
    }
}

/// `{ href: "{api_base}/{resource}/{id}" }` (`toHref`).
fn href(api_base: &str, resource: &str, id: &str) -> Value {
    json!({"href": format!("{api_base}/{resource}/{id}")})
}

/// `omitKeys` of `toFormPayload`: drop the read-only keys, and strip `active` from
/// `settings.enrichment_in_renderer`.
fn to_form_payload(definition: &Value) -> Value {
    let Some(obj) = definition.as_object() else {
        return definition.clone();
    };
    let mut out = Map::new();
    for (k, v) in obj {
        if READ_ONLY_FORM_KEYS.contains(&k.as_str()) {
            continue;
        }
        if k == "settings" {
            let mut settings = v.clone();
            if let Some(enrichment) = settings
                .get("enrichment_in_renderer")
                .and_then(Value::as_object)
                .cloned()
            {
                let mut kept = enrichment;
                kept.remove("active");
                settings["enrichment_in_renderer"] = Value::Object(kept);
            }
            out.insert(k.clone(), settings);
            continue;
        }
        out.insert(k.clone(), v.clone());
    }
    Value::Object(out)
}

/// `themeBody({ values, current: {} })` of the piece, used by `create-theme`.
fn theme_body(args: &Value) -> Result<Value> {
    let name = trimmed(args, "name").ok_or_else(|| anyhow::anyhow!("missing argument `name`"))?;
    let mut body = Map::new();
    body.insert("name".into(), json!(name));
    if let Some(font) = trimmed(args, "font") {
        body.insert("font".into(), json!(font));
    }
    let mut colors = Map::new();
    for (key, field) in [
        ("question_color", "question"),
        ("answer_color", "answer"),
        ("button_color", "button"),
        ("background_color", "background"),
    ] {
        if let Some(v) = trimmed(args, key) {
            colors.insert(field.into(), json!(v));
        }
    }
    if !colors.is_empty() {
        body.insert("colors".into(), Value::Object(colors));
    }
    let mut fields = Map::new();
    if let Some(v) = trimmed(args, "field_alignment") {
        fields.insert("alignment".into(), json!(v));
    }
    if let Some(v) = trimmed(args, "field_font_size") {
        fields.insert("font_size".into(), json!(v));
    }
    if !fields.is_empty() {
        body.insert("fields".into(), Value::Object(fields));
    }
    let mut screens = Map::new();
    if let Some(v) = trimmed(args, "screen_alignment") {
        screens.insert("alignment".into(), json!(v));
    }
    if let Some(v) = trimmed(args, "screen_font_size") {
        screens.insert("font_size".into(), json!(v));
    }
    if !screens.is_empty() {
        body.insert("screens".into(), Value::Object(screens));
    }
    Ok(Value::Object(body))
}

// ---------------------------------------------------------------- adapter

pub struct Typeform {
    spec: ToolkitSpec,
    api_base: String,
}

impl Default for Typeform {
    fn default() -> Self {
        Self::with_api_base("https://api.typeform.com")
    }
}

#[allow(clippy::too_many_lines)]
impl Typeform {
    pub fn with_api_base(api_base: &str) -> Self {
        let t = |slug, title, desc, props: Props<'_>, req: &[&str], scopes: &[&str], m| {
            tool(SLUG, slug, title, desc, props, req, scopes, m)
        };
        let spec = ToolkitSpec {
            slug: SLUG.into(),
            name: "Typeform".into(),
            description: LocalizedText::new(
                "Formulare, Antworten, Workspaces und Themes in Typeform lesen und bearbeiten.",
                "Read and change forms, responses, workspaces and themes in Typeform.",
            ),
            auth_schemes: vec![
                AuthScheme::OAuth2(OAuth2Scheme {
                    authorize_url: "https://admin.typeform.com/oauth/authorize".into(),
                    token_url: "https://api.typeform.com/oauth/token".into(),
                    // Scopes of `src/lib/auth.ts`, unchanged.
                    default_scopes: [
                        "accounts:read",
                        "forms:read",
                        "forms:write",
                        "images:read",
                        "images:write",
                        "offline",
                        "responses:read",
                        "responses:write",
                        "themes:read",
                        "themes:write",
                        "webhooks:write",
                        "workspaces:read",
                        "workspaces:write",
                    ]
                    .iter()
                    .map(|s| (*s).to_string())
                    .collect(),
                    pkce: false,
                    scope_separator: " ".into(),
                    // The piece asks for the `offline` scope.
                    refreshable: true,
                    extra_authorize_params: Vec::new(),
                }),
                // `AuthenticationType.BEARER_TOKEN` in the piece's `typeformRequest`.
                AuthScheme::Bearer,
            ],
            tools: vec![
                t(
                    "TYPEFORM_GET_ME",
                    ("Verbundenen Account lesen", "Get account"),
                    (
                        "Liest Alias, E-Mail und Sprache des verbundenen Typeform-Accounts.",
                        "Reads the alias, email and language of the connected Typeform account.",
                    ),
                    &[],
                    &[],
                    &[ACCOUNTS_R],
                    false,
                ),
                t(
                    "TYPEFORM_LIST_FORMS",
                    ("Formulare auflisten", "List forms"),
                    (
                        "Listet Formulare, optional nach Titel, Workspace und Sortierung gefiltert.",
                        "Lists forms, optionally filtered by title, workspace and sorted by time.",
                    ),
                    &[
                        ("search", s("Nur Formulare, deren Titel diesen Text enthält")),
                        ("workspace", s("Workspace-ID (aus TYPEFORM_LIST_WORKSPACES)")),
                        ("sort_by", choice("Sortierfeld", &["created_at", "last_updated_at"])),
                        ("order_by", choice("Sortierreihenfolge", &["asc", "desc"])),
                        ("page", page_props()[0].1.clone()),
                        ("page_size", page_props()[1].1.clone()),
                    ],
                    &[],
                    &[FORMS_R],
                    false,
                ),
                t(
                    "TYPEFORM_GET_FORM",
                    ("Formular lesen", "Get form"),
                    (
                        "Liest ein Formular mit Fragen, Logik, Bildschirmen und Einstellungen.",
                        "Reads one form with its questions, logic, screens and settings.",
                    ),
                    &[("form_id", s("Formular-ID (aus TYPEFORM_LIST_FORMS)"))],
                    &["form_id"],
                    &[FORMS_R],
                    false,
                ),
                t(
                    "TYPEFORM_GET_FORM_MESSAGES",
                    ("Formular-Texte lesen", "Get form messages"),
                    (
                        "Liest die Oberflächentexte eines Formulars (Button-Labels, Fehlermeldungen).",
                        "Reads the interface texts of a form (button labels, error messages).",
                    ),
                    &[("form_id", s("Formular-ID (aus TYPEFORM_LIST_FORMS)"))],
                    &["form_id"],
                    &[FORMS_R],
                    false,
                ),
                t(
                    "TYPEFORM_LIST_RESPONSES",
                    ("Antworten suchen", "Find responses"),
                    (
                        "Listet Antworten zu einem Formular, filterbar nach Text, Zeitraum und Typ.",
                        "Lists responses to a form, filterable by text, time range and type.",
                    ),
                    &[
                        ("form_id", s("Formular-ID (aus TYPEFORM_LIST_FORMS)")),
                        ("query", s("Nur Antworten, die diesen Text enthalten")),
                        ("since", s("Nur Antworten, die nach diesem Zeitpunkt abgeschickt wurden (ISO 8601)")),
                        ("until", s("Nur Antworten, die vor diesem Zeitpunkt abgeschickt wurden (ISO 8601)")),
                        ("response_type", choice("Nur Antworten dieses Typs", &["completed", "partial", "started"])),
                        ("included_response_ids", str_list("Nur diese Antworten (IDs)")),
                        ("sort", choice("Sortierung", &["submitted_at,desc", "submitted_at,asc"])),
                        ("before", s("Token einer Antwort; liefert ältere Antworten (nächste Seite)")),
                        ("after", s("Token einer Antwort; liefert neuere Antworten")),
                        ("page_size", int("Antworten pro Seite, 1 bis 1000.", 1, MAX_RESPONSE_PAGE_SIZE, 25)),
                    ],
                    &["form_id"],
                    &[RESPONSES_R],
                    false,
                ),
                t(
                    "TYPEFORM_LIST_THEMES",
                    ("Themes auflisten", "List themes"),
                    (
                        "Listet Themes des Accounts mit Schrift, Farben und Sichtbarkeit.",
                        "Lists the account's themes with font, colors and visibility.",
                    ),
                    &page_props(),
                    &[],
                    &[THEMES_R],
                    false,
                ),
                t(
                    "TYPEFORM_GET_THEME",
                    ("Theme lesen", "Get theme"),
                    (
                        "Liest ein Theme mit Farben, Schrift und Layout.",
                        "Reads one theme with its colors, font and layout.",
                    ),
                    &[("theme_id", s("Theme-ID (aus TYPEFORM_LIST_THEMES)"))],
                    &["theme_id"],
                    &[THEMES_R],
                    false,
                ),
                t(
                    "TYPEFORM_LIST_WORKSPACES",
                    ("Workspaces auflisten", "List workspaces"),
                    (
                        "Listet die Workspaces des Accounts mit Name, Default-Flag und Formularzahl.",
                        "Lists the account's workspaces with name, default flag and form count.",
                    ),
                    &[
                        ("search", s("Nur Workspaces, deren Name diesen Text enthält")),
                        ("page", page_props()[0].1.clone()),
                        ("page_size", page_props()[1].1.clone()),
                    ],
                    &[],
                    &[WORKSPACES_R],
                    false,
                ),
                t(
                    "TYPEFORM_LIST_ACCOUNT_WORKSPACES",
                    ("Workspaces eines Accounts auflisten", "List account workspaces"),
                    (
                        "Listet die Workspaces eines bestimmten Accounts (account_id aus einem Workspace).",
                        "Lists the workspaces of one account (account_id taken from a workspace).",
                    ),
                    &[("account_id", s("Account-ID (aus TYPEFORM_LIST_WORKSPACES)"))],
                    &["account_id"],
                    &[ACCOUNTS_R, WORKSPACES_R],
                    false,
                ),
                t(
                    "TYPEFORM_GET_WORKSPACE",
                    ("Workspace lesen", "Get workspace"),
                    (
                        "Liest einen Workspace mit seinen Mitgliedern und Rollen.",
                        "Reads one workspace with its members and their roles.",
                    ),
                    &[("workspace_id", s("Workspace-ID (aus TYPEFORM_LIST_WORKSPACES)"))],
                    &["workspace_id"],
                    &[WORKSPACES_R],
                    false,
                ),
                t(
                    "TYPEFORM_LIST_IMAGES",
                    ("Bilder auflisten", "List images"),
                    (
                        "Listet die in den Account hochgeladenen Bilder mit ID, Dateiname und Link.",
                        "Lists images uploaded to the account with ID, file name and link. The REST \
                         endpoint returns them all at once, so there is no page argument.",
                    ),
                    &[],
                    &[],
                    &[IMAGES_R],
                    false,
                ),
                t(
                    "TYPEFORM_GET_IMAGE",
                    ("Bild lesen", "Get image"),
                    (
                        "Liest ein Bild mit Dateiname, Medientyp, Maßen und Link.",
                        "Reads one image with its file name, media type, dimensions and link.",
                    ),
                    &[("image_id", s("Bild-ID (aus TYPEFORM_LIST_IMAGES)"))],
                    &["image_id"],
                    &[IMAGES_R],
                    false,
                ),
                t(
                    "TYPEFORM_CREATE_FORM",
                    ("Formular anlegen", "Create form"),
                    (
                        "Legt ein leeres oder mit Fragen gefülltes Formular an; braucht --confirm.",
                        "Creates an empty form or one with questions; requires --confirm.",
                    ),
                    &[
                        ("title", s("Titel des Formulars")),
                        ("workspace", s("Workspace-ID, in den das Formular gelegt wird")),
                        ("theme", s("Theme-ID des Formulars")),
                        ("fields", json_object_array("JSON-Array der Typeform-Felder; leer lassen für ein leeres Formular")),
                    ],
                    &["title"],
                    &[FORMS_W],
                    true,
                ),
                t(
                    "TYPEFORM_UPDATE_FORM",
                    ("Formular ändern", "Update form"),
                    (
                        "Ändert Titel, Sichtbarkeit, Theme oder Workspace eines Formulars; braucht --confirm.",
                        "Changes a form's title, visibility, theme or workspace; requires --confirm.",
                    ),
                    &[
                        ("form_id", s("Formular-ID (aus TYPEFORM_LIST_FORMS)")),
                        ("title", s("Neuer Titel")),
                        ("is_public", choice("Formular offen oder geschlossen", &["true", "false"])),
                        ("theme", s("Theme-ID des Formulars")),
                        ("workspace", s("Workspace-ID des Formulars")),
                    ],
                    &["form_id"],
                    &[FORMS_W],
                    true,
                ),
                t(
                    "TYPEFORM_REPLACE_FORM",
                    ("Formular ersetzen", "Replace form"),
                    (
                        "Ersetzt das gesamte Formular durch ein JSON-Objekt aus Get Form; braucht --confirm. \
                         Fragen, die in der Definition fehlen, werden mit ihren Antworten gelöscht.",
                        "Replaces the whole form with a JSON object taken from Get Form; requires --confirm. \
                         Questions missing from the definition are deleted together with their responses.",
                    ),
                    &[
                        ("form_id", s("Formular-ID (aus TYPEFORM_LIST_FORMS)")),
                        ("definition", json_object("Vollständiges Formular-JSON aus TYPEFORM_GET_FORM, geändert; muss einen Titel enthalten")),
                    ],
                    &["form_id", "definition"],
                    &[FORMS_W],
                    true,
                ),
                t(
                    "TYPEFORM_DELETE_FORM",
                    ("Formular löschen", "Delete form"),
                    (
                        "Löscht ein Formular samt Antworten unwiderruflich; braucht --confirm.",
                        "Deletes a form and all its responses permanently; requires --confirm.",
                    ),
                    &[("form_id", s("Formular-ID (aus TYPEFORM_LIST_FORMS)"))],
                    &["form_id"],
                    &[FORMS_W],
                    true,
                ),
                t(
                    "TYPEFORM_CREATE_THEME",
                    ("Theme anlegen", "Create theme"),
                    (
                        "Legt ein eigenes Theme mit Farben, Schrift und Ausrichtung an; braucht --confirm.",
                        "Creates a custom theme with colors, font and alignment; requires --confirm.",
                    ),
                    &[
                        ("name", s("Name des Themes")),
                        ("question_color", s("Hex-Farbe der Fragen, z. B. #3D3D3D")),
                        ("answer_color", s("Hex-Farbe der Antworten")),
                        ("button_color", s("Hex-Farbe des Buttons")),
                        ("background_color", s("Hex-Farbe des Hintergrunds")),
                        ("font", s("Schrift, z. B. Montserrat")),
                        ("field_alignment", choice("Ausrichtung der Fragen", &["left", "center"])),
                        ("field_font_size", choice("Schriftgröße der Fragen", &["small", "medium", "large"])),
                        ("screen_alignment", choice("Ausrichtung der Bildschirme", &["left", "center"])),
                        ("screen_font_size", choice("Schriftgröße der Bildschirme", &["small", "medium", "large"])),
                    ],
                    &[
                        "name",
                        "question_color",
                        "answer_color",
                        "button_color",
                        "background_color",
                    ],
                    &[THEMES_W],
                    true,
                ),
                t(
                    "TYPEFORM_CREATE_WORKSPACE",
                    ("Workspace anlegen", "Create workspace"),
                    (
                        "Legt einen Workspace im verbundenen Account an; braucht --confirm.",
                        "Creates a workspace in the connected account; requires --confirm.",
                    ),
                    &[("name", s("Name des Workspaces"))],
                    &["name"],
                    &[WORKSPACES_W],
                    true,
                ),
                t(
                    "TYPEFORM_CREATE_ACCOUNT_WORKSPACE",
                    ("Workspace in einem Account anlegen", "Create account workspace"),
                    (
                        "Legt einen Workspace in einem bestimmten Account an; braucht --confirm.",
                        "Creates a workspace in one specific account; requires --confirm.",
                    ),
                    &[
                        ("account_id", s("Account-ID (aus TYPEFORM_LIST_WORKSPACES)")),
                        ("name", s("Name des Workspaces")),
                    ],
                    &["account_id", "name"],
                    &[ACCOUNTS_R, WORKSPACES_W],
                    true,
                ),
                t(
                    "TYPEFORM_UPDATE_WORKSPACE",
                    ("Workspace umbenennen", "Update workspace"),
                    (
                        "Benennt einen Workspace um; braucht --confirm.",
                        "Renames a workspace; requires --confirm.",
                    ),
                    &[
                        ("workspace_id", s("Workspace-ID (aus TYPEFORM_LIST_WORKSPACES)")),
                        ("name", s("Neuer Name des Workspaces")),
                    ],
                    &["workspace_id", "name"],
                    &[WORKSPACES_W],
                    true,
                ),
                t(
                    "TYPEFORM_DELETE_IMAGE",
                    ("Bild löschen", "Delete image"),
                    (
                        "Löscht ein hochgeladenes Bild; braucht --confirm.",
                        "Deletes an uploaded image; requires --confirm.",
                    ),
                    &[("image_id", s("Bild-ID (aus TYPEFORM_LIST_IMAGES)"))],
                    &["image_id"],
                    &[IMAGES_W],
                    true,
                ),
                t(
                    "TYPEFORM_DELETE_RESPONSES",
                    ("Antworten löschen", "Delete responses"),
                    (
                        "Löscht 1 bis 1000 Antworten eines Formulars; braucht --confirm.",
                        "Deletes 1 to 1000 responses of a form; requires --confirm.",
                    ),
                    &[
                        ("form_id", s("Formular-ID (aus TYPEFORM_LIST_FORMS)")),
                        ("response_ids", str_list("IDs der zu löschenden Antworten (1 bis 1000)")),
                    ],
                    &["form_id", "response_ids"],
                    &[RESPONSES_W],
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

impl ToolkitAdapter for Typeform {
    fn spec(&self) -> &ToolkitSpec {
        &self.spec
    }

    #[allow(clippy::too_many_lines)]
    fn build_request(&self, tool: &str, args: &Value) -> Result<HttpRequest> {
        let base = self.api_base.as_str();
        Ok(match tool {
            // AP: src/lib/actions/get-me.ts -> GET /me
            "TYPEFORM_GET_ME" => HttpRequest::new("GET", format!("{base}/me")),
            // AP: src/lib/actions/list-forms.ts -> GET /forms?search&workspace_id&sort_by&order_by&page&page_size
            "TYPEFORM_LIST_FORMS" => HttpRequest::new("GET", format!("{base}/forms")).with_query(&query(&[
                ("search", trimmed(args, "search").map(str::to_string)),
                ("workspace_id", trimmed(args, "workspace").map(str::to_string)),
                ("sort_by", trimmed(args, "sort_by").map(str::to_string)),
                ("order_by", trimmed(args, "order_by").map(str::to_string)),
                ("page", opt_num(args, "page", 1, u64::MAX).map(|n| n.to_string())),
                ("page_size", opt_num(args, "page_size", 1, MAX_PAGE_SIZE).map(|n| n.to_string())),
            ])),
            // AP: src/lib/common/index.ts getForm() + src/lib/actions/get-form.ts -> GET /forms/{form_id}
            "TYPEFORM_GET_FORM" => HttpRequest::new(
                "GET",
                format!("{base}/forms/{}", path_segment(args, "form_id")?),
            ),
            // AP: src/lib/actions/get-form-messages.ts -> GET /forms/{form_id}/messages
            "TYPEFORM_GET_FORM_MESSAGES" => HttpRequest::new(
                "GET",
                format!(
                    "{base}/forms/{}/messages",
                    path_segment(args, "form_id")?
                ),
            ),
            // AP: src/lib/actions/list-responses.ts -> GET /forms/{form_id}/responses?query&since&until&response_type&included_response_ids&sort&before&after&page_size
            "TYPEFORM_LIST_RESPONSES" => HttpRequest::new(
                "GET",
                format!(
                    "{base}/forms/{}/responses",
                    path_segment(args, "form_id")?
                ),
            )
            .with_query(&query(&[
                ("query", trimmed(args, "query").map(str::to_string)),
                ("since", trimmed(args, "since").map(str::to_string)),
                ("until", trimmed(args, "until").map(str::to_string)),
                (
                    "response_type",
                    trimmed(args, "response_type").map(str::to_string),
                ),
                (
                    "included_response_ids",
                    joined_list(args, "included_response_ids"),
                ),
                ("sort", trimmed(args, "sort").map(str::to_string)),
                ("before", trimmed(args, "before").map(str::to_string)),
                ("after", trimmed(args, "after").map(str::to_string)),
                (
                    "page_size",
                    opt_num(args, "page_size", 1, MAX_RESPONSE_PAGE_SIZE).map(|n| n.to_string()),
                ),
            ])),
            // AP: src/lib/actions/list-themes.ts -> GET /themes?page&page_size
            "TYPEFORM_LIST_THEMES" => {
                HttpRequest::new("GET", format!("{base}/themes")).with_query(&query(&[
                    ("page", opt_num(args, "page", 1, u64::MAX).map(|n| n.to_string())),
                    (
                        "page_size",
                        opt_num(args, "page_size", 1, MAX_PAGE_SIZE).map(|n| n.to_string()),
                    ),
                ]))
            }
            // AP: src/lib/actions/get-theme.ts -> GET /themes/{themeId}
            "TYPEFORM_GET_THEME" => HttpRequest::new(
                "GET",
                format!("{base}/themes/{}", path_segment(args, "theme_id")?),
            ),
            // AP: src/lib/actions/list-workspaces.ts (no accountId) -> GET /workspaces?search&page&page_size
            "TYPEFORM_LIST_WORKSPACES" => {
                HttpRequest::new("GET", format!("{base}/workspaces")).with_query(&query(&[
                    ("search", trimmed(args, "search").map(str::to_string)),
                    ("page", opt_num(args, "page", 1, u64::MAX).map(|n| n.to_string())),
                    (
                        "page_size",
                        opt_num(args, "page_size", 1, MAX_PAGE_SIZE).map(|n| n.to_string()),
                    ),
                ]))
            }
            // AP: src/lib/actions/list-workspaces.ts (accountId set) -> GET /accounts/{accountId}/workspaces
            "TYPEFORM_LIST_ACCOUNT_WORKSPACES" => HttpRequest::new(
                "GET",
                format!(
                    "{base}/accounts/{}/workspaces",
                    path_segment(args, "account_id")?
                ),
            ),
            // AP: src/lib/actions/get-workspace.ts -> GET /workspaces/{workspace}
            "TYPEFORM_GET_WORKSPACE" => HttpRequest::new(
                "GET",
                format!(
                    "{base}/workspaces/{}",
                    path_segment(args, "workspace_id")?
                ),
            ),
            // AP: src/lib/actions/list-images.ts -> GET /images (the endpoint takes no query
            // parameters; the piece's `limit` only slices the returned array).
            "TYPEFORM_LIST_IMAGES" => HttpRequest::new("GET", format!("{base}/images")),
            // AP: src/lib/actions/get-image.ts -> GET /images/{imageId}
            "TYPEFORM_GET_IMAGE" => HttpRequest::new(
                "GET",
                format!("{base}/images/{}", path_segment(args, "image_id")?),
            ),
            // AP: src/lib/actions/create-form.ts -> POST /forms
            "TYPEFORM_CREATE_FORM" => {
                let mut body = Map::new();
                body.insert(
                    "title".into(),
                    json!(req_str(args, "title")?.trim()),
                );
                if let Some(w) = trimmed(args, "workspace") {
                    body.insert("workspace".into(), href(base, "workspaces", w));
                }
                if let Some(t) = trimmed(args, "theme") {
                    body.insert("theme".into(), href(base, "themes", t));
                }
                match args.get("fields") {
                    Some(Value::Array(f)) if !f.is_empty() => {
                        body.insert("fields".into(), Value::Array(f.clone()));
                    }
                    Some(Value::Null) | None => {}
                    Some(_) => bail!("`fields` must be a JSON array of field objects"),
                }
                HttpRequest::new("POST", format!("{base}/forms")).json(&Value::Object(body))
            }
            // AP: src/lib/actions/update-form.ts -> PATCH /forms/{form_id} (JSON-Patch operations)
            "TYPEFORM_UPDATE_FORM" => {
                let mut ops: Vec<Value> = Vec::new();
                if let Some(title) = trimmed(args, "title") {
                    ops.push(json!({"op": "replace", "path": "/title", "value": title}));
                }
                match trimmed(args, "is_public") {
                    Some("true") => ops.push(
                        json!({"op": "replace", "path": "/settings/is_public", "value": true}),
                    ),
                    Some("false") => ops.push(
                        json!({"op": "replace", "path": "/settings/is_public", "value": false}),
                    ),
                    Some(_) => bail!("`is_public` must be \"true\" or \"false\""),
                    None => {}
                }
                if let Some(t) = trimmed(args, "theme") {
                    ops.push(json!({"op": "replace", "path": "/theme", "value": href(base, "themes", t)}));
                }
                if let Some(w) = trimmed(args, "workspace") {
                    ops.push(json!({"op": "replace", "path": "/workspace", "value": href(base, "workspaces", w)}));
                }
                if ops.is_empty() {
                    bail!("nothing to update: set at least one of title, is_public, theme or workspace");
                }
                HttpRequest::new(
                    "PATCH",
                    format!("{base}/forms/{}", path_segment(args, "form_id")?),
                )
                .json(&Value::Array(ops))
            }
            // AP: src/lib/actions/replace-form.ts -> PUT /forms/{form_id} (toFormPayload of `definition`).
            // The piece additionally re-reads the current form to keep its theme and to
            // report which questions a definition drops; both need the extra GET, so the
            // definition has to carry the theme and every question it wants to keep.
            "TYPEFORM_REPLACE_FORM" => {
                let definition = args
                    .get("definition")
                    .ok_or_else(|| anyhow::anyhow!("missing argument `definition`"))?;
                if !definition.is_object() {
                    bail!("`definition` must be a JSON object");
                }
                let title = definition
                    .get("title")
                    .and_then(Value::as_str)
                    .map(str::trim)
                    .filter(|v| !v.is_empty())
                    .ok_or_else(|| anyhow::anyhow!("`definition` must include a title"))?;
                let mut body = to_form_payload(definition);
                body["title"] = json!(title);
                HttpRequest::new(
                    "PUT",
                    format!("{base}/forms/{}", path_segment(args, "form_id")?),
                )
                .json(&body)
            }
            // AP: src/lib/actions/delete-form.ts -> DELETE /forms/{form_id}
            "TYPEFORM_DELETE_FORM" => HttpRequest::new(
                "DELETE",
                format!("{base}/forms/{}", path_segment(args, "form_id")?),
            ),
            // AP: src/lib/actions/create-theme.ts -> POST /themes (themeBody)
            "TYPEFORM_CREATE_THEME" => {
                let mut body = theme_body(args)?;
                // create-theme.ts defaults the alignments and font sizes, so both
                // `fields` and `screens` are always part of the create payload.
                body["fields"] = json!({
                    "alignment": trimmed(args, "field_alignment").unwrap_or("left"),
                    "font_size": trimmed(args, "field_font_size").unwrap_or("medium"),
                });
                body["screens"] = json!({
                    "alignment": trimmed(args, "screen_alignment").unwrap_or("left"),
                    "font_size": trimmed(args, "screen_font_size").unwrap_or("medium"),
                });
                HttpRequest::new("POST", format!("{base}/themes")).json(&body)
            }
            // AP: src/lib/actions/create-workspace.ts (no accountId) -> POST /workspaces
            "TYPEFORM_CREATE_WORKSPACE" => {
                let name = req_str(args, "name")?.trim();
                HttpRequest::new("POST", format!("{base}/workspaces"))
                    .json(&json!({"name": name}))
            }
            // AP: src/lib/actions/create-workspace.ts (accountId set) -> POST /accounts/{accountId}/workspaces
            "TYPEFORM_CREATE_ACCOUNT_WORKSPACE" => {
                let name = req_str(args, "name")?.trim();
                HttpRequest::new(
                    "POST",
                    format!(
                        "{base}/accounts/{}/workspaces",
                        path_segment(args, "account_id")?
                    ),
                )
                .json(&json!({"name": name}))
            }
            // AP: src/lib/actions/update-workspace.ts -> PATCH /workspaces/{workspace}
            // with [{op:replace, path:/name, value}]; the piece answers with a follow-up
            // GET, here the PATCH response itself is returned.
            "TYPEFORM_UPDATE_WORKSPACE" => HttpRequest::new(
                "PATCH",
                format!(
                    "{base}/workspaces/{}",
                    path_segment(args, "workspace_id")?
                ),
            )
            .json(&json!([{"op": "replace", "path": "/name", "value": req_str(args, "name")?.trim()}])),
            // AP: src/lib/actions/delete-image.ts -> DELETE /images/{imageId}
            "TYPEFORM_DELETE_IMAGE" => HttpRequest::new(
                "DELETE",
                format!("{base}/images/{}", path_segment(args, "image_id")?),
            ),
            // AP: src/lib/actions/delete-responses.ts -> DELETE /forms/{form_id}/responses?included_response_ids=
            "TYPEFORM_DELETE_RESPONSES" => {
                let ids = str_vec(args, "response_ids");
                if ids.is_empty() || ids.len() > MAX_DELETE_RESPONSE_IDS {
                    bail!("`response_ids` must contain between 1 and 1000 IDs");
                }
                HttpRequest::new(
                    "DELETE",
                    format!(
                        "{base}/forms/{}/responses",
                        path_segment(args, "form_id")?
                    ),
                )
                .with_query(&query(&[(
                    "included_response_ids",
                    Some(ids.join(",")),
                )]))
            }
            other => bail!("unknown Typeform tool `{other}`"),
        })
    }

    fn parse_response(&self, tool: &str, response: &HttpResponse) -> Result<Value> {
        let v = response.json()?;
        Ok(match tool {
            // list-images.ts returns the raw array; the piece shapes it into a page.
            "TYPEFORM_LIST_IMAGES" => {
                let items = v.as_array().cloned().unwrap_or_default();
                let count = items.len();
                json!({"items": items, "count": count, "total_items": count})
            }
            // delete-form.ts / delete-image.ts / delete-responses.ts answer with a
            // confirmation object after the provider returns an empty body.
            "TYPEFORM_DELETE_FORM" | "TYPEFORM_DELETE_IMAGE" | "TYPEFORM_DELETE_RESPONSES" => {
                json!({"deleted": true})
            }
            // update-form.ts and update-workspace.ts re-read the object after the PATCH;
            // the provider's own answer is returned when it carries one.
            "TYPEFORM_UPDATE_FORM" | "TYPEFORM_UPDATE_WORKSPACE" => {
                if v.is_object() {
                    v
                } else {
                    json!({"updated": true})
                }
            }
            _ => v,
        })
    }
}

/// `HttpRequest::with_query` keeps every request arm a single expression.
trait WithQuery {
    fn with_query(self, q: &str) -> Self;
}

impl WithQuery for HttpRequest {
    fn with_query(self, q: &str) -> Self {
        if q.is_empty() {
            self
        } else {
            HttpRequest {
                url: format!("{}{}", self.url, q),
                ..self
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn requests_and_validation() {
        let a = Typeform::with_api_base("https://tf.test");
        let r = a
            .build_request("TYPEFORM_LIST_FORMS", &json!({"search": "Q3", "page": 2}))
            .unwrap();
        assert_eq!(r.method, "GET");
        assert_eq!(r.url, "https://tf.test/forms?search=Q3&page=2");

        // Unset pagination is left out of the query, like `compactQuery`.
        let r = a.build_request("TYPEFORM_LIST_THEMES", &json!({})).unwrap();
        assert_eq!(r.url, "https://tf.test/themes");

        let r = a
            .build_request("TYPEFORM_LIST_RESPONSES", &json!({"form_id": "abc123", "sort": "submitted_at,desc"}))
            .unwrap();
        assert_eq!(
            r.url,
            "https://tf.test/forms/abc123/responses?sort=submitted_at%2Cdesc"
        );

        assert!(a
            .build_request("TYPEFORM_GET_FORM", &json!({"form_id": "../secrets"}))
            .is_err());
        assert!(a
            .build_request("TYPEFORM_UPDATE_FORM", &json!({"form_id": "abc"}))
            .is_err());
        assert!(a
            .build_request("TYPEFORM_REPLACE_FORM", &json!({"form_id": "abc", "definition": {"fields": []}}))
            .is_err());
        assert!(a
            .build_request("TYPEFORM_DELETE_RESPONSES", &json!({"form_id": "abc", "response_ids": []}))
            .is_err());
    }

    #[test]
    fn bodies_and_mappers() {
        let a = Typeform::with_api_base("https://tf.test");
        let r = a
            .build_request(
                "TYPEFORM_UPDATE_FORM",
                &json!({"form_id": "abc", "title": "Neu", "workspace": "w1"}),
            )
            .unwrap();
        assert_eq!(r.method, "PATCH");
        let body: Value = serde_json::from_slice(r.body.as_deref().unwrap()).unwrap();
        assert_eq!(body[0]["path"], "/title");
        assert_eq!(body[1]["value"]["href"], "https://tf.test/workspaces/w1");

        let r = a
            .build_request(
                "TYPEFORM_REPLACE_FORM",
                &json!({"form_id": "abc", "definition": {
                    "id": "abc", "title": "T", "created_at": "2026-01-01",
                    "settings": {"is_public": true, "enrichment_in_renderer": {"active": true, "version": "2"}}}}),
            )
            .unwrap();
        let body: Value = serde_json::from_slice(r.body.as_deref().unwrap()).unwrap();
        assert!(body.get("id").is_none());
        assert!(body.get("created_at").is_none());
        assert_eq!(body["settings"]["enrichment_in_renderer"]["active"], Value::Null);
        assert_eq!(body["settings"]["enrichment_in_renderer"]["version"], "2");

        let r = a
            .build_request(
                "TYPEFORM_CREATE_THEME",
                &json!({"name": " Acme ", "question_color": "#111", "answer_color": "#222",
                        "button_color": "#333", "background_color": "#444"}),
            )
            .unwrap();
        let body: Value = serde_json::from_slice(r.body.as_deref().unwrap()).unwrap();
        assert_eq!(body["name"], "Acme");
        assert_eq!(body["colors"]["question"], "#111");
        assert_eq!(body["fields"]["alignment"], "left");
        assert_eq!(body["screens"]["font_size"], "medium");

        let resp = crate::http::HttpResponse {
            status: 200,
            body: serde_json::to_vec(&json!([{"id": "img1"}])).unwrap(),
            session_id: None,
        };
        let out = a.parse_response("TYPEFORM_LIST_IMAGES", &resp).unwrap();
        assert_eq!(out["count"], 1);
        assert_eq!(out["items"][0]["id"], "img1");

        let resp = crate::http::HttpResponse {
            status: 204,
            body: Vec::new(),
            session_id: None,
        };
        let out = a.parse_response("TYPEFORM_DELETE_FORM", &resp).unwrap();
        assert_eq!(out["deleted"], true);
        let out = a.parse_response("TYPEFORM_UPDATE_FORM", &resp).unwrap();
        assert_eq!(out["updated"], true);
    }
}