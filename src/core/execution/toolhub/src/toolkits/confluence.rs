//! Confluence Cloud toolkit (REST API v1 + v2). Action set modelled on Activepieces
//! `confluence` (MIT): every arm below is one `confluenceApiCall` /
//! `confluencePaginatedApiCall` from `src/lib/common/index.ts` of that piece.
//!
//! Auth: the piece uses `PieceAuth.CustomAuth` with `username` (account e-mail) plus
//! `password` (Atlassian API token), sent as **basic auth**, and a third prop
//! `confluenceDomain` such as `https://your-domain.atlassian.net`. Basic auth is
//! therefore `AuthScheme::Basic` here; the site URL cannot live in the auth config, so
//! it is a constructor argument ([`Confluence::with_api_base`], the piece's
//! `confluenceDomain` prop). Confluence Cloud basic auth has no OAuth scopes, so the
//! `scopes` of every tool carry the Atlassian API-token permission level the call
//! needs (`read:confluence-content.all` / `write:confluence-content.all`).
//!
//! The two API roots come straight from the piece:
//! `version: 'v1'` -> `{domain}/wiki/rest/api`, `version: 'v2'` -> `{domain}/wiki/api/v2`.
//! Pagination mirrors `confluencePaginatedApiCall`: v2 follows `_links.next` (first page
//! carries `limit=200`), v1 walks `start`/`limit=100` pages. Since one tool call builds
//! exactly one provider request, the list tools send the first page parameters only.
//!
//! `DEFAULT_SITE` is the template value from the piece's own auth prop description
//! ("Example value - https://your-domain.atlassian.net"); it is a placeholder, never a
//! real account. Every caller that knows the site must build its own adapter with
//! `Confluence::with_api_base("https://<your site>.atlassian.net")`.
use anyhow::{bail, Result};
use serde_json::{json, Map, Value};

use super::common::*;
use super::{path_segment, ToolkitAdapter};
use crate::http::{HttpRequest, HttpResponse};
use crate::model::*;

pub const SLUG: &str = "confluence";

const READ: &str = "read:confluence-content.all";
const WRITE: &str = "write:confluence-content.all";

/// Placeholder site root (the piece's documented example value). Real accounts use
/// `https://<your site>.atlassian.net`; wire that in via [`Confluence::with_api_base`].
const DEFAULT_SITE: &str = "https://your-domain.atlassian.net";

// ---------------------------------------------------------------- schema fragments

fn choice(desc: &str, options: &[&str]) -> Value {
    json!({"type": "string", "description": desc, "enum": options})
}

fn flag(desc: &str, default: bool) -> Value {
    json!({"type": "boolean", "description": desc, "default": default})
}

/// `{representation, value}` — the Confluence body envelope of the piece's v2 calls.
fn body_field(representation: &str, value: &str) -> Value {
    json!({"representation": representation, "value": value})
}

fn req_page_id(args: &Value) -> Result<&str> {
    path_segment(args, "page_id")
}

// ---------------------------------------------------------------- flatteners

fn flatten_space(sp: &Value) -> Value {
    json!({
        "id": sp["id"], "key": sp["key"], "name": sp["name"], "type": sp["type"],
        "status": sp["status"], "description": sp["description"],
        "homepage_id": sp["homepageId"],
        "webui_link": sp["_links"]["webui"],
    })
}

fn flatten_page_ref(p: &Value) -> Value {
    json!({"id": p["id"], "title": p["title"], "status": p["status"], "space_id": p["spaceId"]})
}

fn flatten_page(p: &Value) -> Value {
    json!({
        "id": p["id"], "title": p["title"], "status": p["status"], "space_id": p["spaceId"],
        "author_id": p["authorId"], "created_date": p["createdDate"],
        "version_number": p["version"]["number"], "version_created_at": p["version"]["createdAt"],
        "body_representation": p["body"]["representation"], "body_value": p["body"]["value"],
        "webui_link": p["_links"]["webui"],
    })
}

fn flatten_comment(c: &Value) -> Value {
    json!({
        "id": c["id"], "title": c["title"], "type": c["type"], "status": c["status"],
        "page_id": c["pageId"], "space_id": c["spaceId"], "parent_id": c["parentId"],
        "footer_id": c["footerId"], "inline_id": c["inlineId"], "author_id": c["authorId"],
        "created_date": c["createdDate"],
        "body_representation": c["body"]["representation"], "body_value": c["body"]["value"],
    })
}

fn flatten_attachment(a: &Value) -> Value {
    json!({
        "id": a["id"], "title": a["title"], "media_type": a["mediaType"],
        "file_size": a["fileSize"], "comment": a["comment"], "created_date": a["createdDate"],
        "version": a["version"], "download_link": a["downloadLink"],
        "webui_link": a["_links"]["webui"],
    })
}

/// v1 `/content/...` result (CQL search).
fn flatten_content(c: &Value) -> Value {
    json!({
        "id": c["id"], "type": c["type"], "title": c["title"], "status": c["status"],
        "space_key": c["space"]["key"], "space_name": c["space"]["name"],
        "version_number": c["version"]["number"], "version_when": c["version"]["when"],
        "webui_link": c["_links"]["webui"],
    })
}

fn flatten_user(u: &Value) -> Value {
    json!({
        "type": u["type"], "account_id": u["accountId"], "account_status": u["accountStatus"],
        "user_key": u["userkey"], "username": u["username"], "display_name": u["displayName"],
        "email": u["email"], "profile_picture_url": u["profilePictureUrl"],
        "is_external_collaborator": u["isExternalCollaborator"],
    })
}

/// `{count, results}` of a v2 collection, mapped with `f`.
fn collection(v: &Value, f: fn(&Value) -> Value) -> Value {
    let items: Vec<Value> = v["results"].as_array().cloned().unwrap_or_default().iter().map(f).collect();
    json!({"count": items.len(), "results": items})
}

// ---------------------------------------------------------------- adapter

pub struct Confluence {
    spec: ToolkitSpec,
    site: String,
}

impl Default for Confluence {
    fn default() -> Self {
        Self::with_api_base(DEFAULT_SITE)
    }
}

impl Confluence {
    /// `with_api_base("https://acme.atlassian.net")` — the piece's `confluenceDomain`
    /// prop. Trailing slashes are stripped exactly like `normalizeDomain`.
    pub fn with_api_base(api_base: &str) -> Self {
        let site = api_base.trim_end_matches('/').to_string();
        let t = |slug, title, desc: (&str, &str), props: Props<'_>, req: &[&str], scopes: &[&str], m| {
            tool(SLUG, slug, title, desc, props, req, scopes, m)
        };
        let spec = ToolkitSpec {
            slug: SLUG.into(),
            name: "Confluence Cloud".into(),
            description: LocalizedText::new(
                "Confluence-Cloud-Seiten, Spaces, Kommentare und Labels lesen und bearbeiten.",
                "Read and change Confluence Cloud pages, spaces, comments and labels.",
            ),
            auth_schemes: vec![AuthScheme::Basic],
            tools: vec![
                // --- read / search ---
                t(
                    "CONFLUENCE_LIST_SPACES",
                    ("Spaces auflisten", "List spaces"),
                    (
                        "Listet die zugänglichen Spaces (erste Seite mit `limit=200`).",
                        "Lists the accessible spaces (first page with `limit=200`).",
                    ),
                    &[
                        ("type", choice("Optionaler Space-Typ", &["global", "personal", "collaboration", "knowledge_base"])),
                        ("status", choice("Optionaler Status", &["current", "archived"])),
                    ],
                    &[],
                    &[READ],
                    false,
                ),
                t(
                    "CONFLUENCE_GET_SPACE",
                    ("Space lesen", "Get space"),
                    (
                        "Liest einen Space inklusive Schlüssel und Startseite.",
                        "Reads one space including its key and home page.",
                    ),
                    &[("space_id", s("Space-ID (aus CONFLUENCE_LIST_SPACES)"))],
                    &["space_id"],
                    &[READ],
                    false,
                ),
                t(
                    "CONFLUENCE_LIST_SPACE_PAGES",
                    ("Seiten eines Spaces auflisten", "List pages in space"),
                    (
                        "Listet die Seiten eines Spaces (erste Seite mit `limit=200`).",
                        "Lists the pages of one space (first page with `limit=200`).",
                    ),
                    &[("space_id", s("Space-ID (aus CONFLUENCE_LIST_SPACES)"))],
                    &["space_id"],
                    &[READ],
                    false,
                ),
                t(
                    "CONFLUENCE_FIND_PAGE_BY_TITLE",
                    ("Seite nach Titel finden", "Find page by title"),
                    (
                        "Sucht Seiten nach exaktem Titel, optional auf einen Space begrenzt.",
                        "Looks pages up by exact title, optionally restricted to one space.",
                    ),
                    &[
                        ("title", s("Exakter Titel")),
                        ("space_id", s("Optional: nur in diesem Space suchen")),
                    ],
                    &["title"],
                    &[READ],
                    false,
                ),
                t(
                    "CONFLUENCE_GET_PAGE_CONTENT",
                    ("Seiteninhalt lesen", "Get page content"),
                    (
                        "Liest eine Seite mit Body im Storage-Format (`body-format=storage`).",
                        "Reads one page with its body in storage format (`body-format=storage`).",
                    ),
                    &[("page_id", s("Numerische Seiten-ID aus der Seiten-URL"))],
                    &["page_id"],
                    &[READ],
                    false,
                ),
                t(
                    "CONFLUENCE_GET_PAGE_BY_URL",
                    ("Seite über URL lesen", "Get page by URL"),
                    (
                        "Liest eine Seite; die Seiten-ID wird aus der Confluence-URL extrahiert.",
                        "Reads one page; the page ID is extracted from the Confluence URL.",
                    ),
                    &[
                        ("url", s("Confluence-Seiten-URL, z. B. https://acme.atlassian.net/wiki/spaces/DOCS/pages/12345/Title")),
                        ("body_format", choice("Body-Format; leer lassen, um keinen Body zu laden", &["storage", "view", "atlas_doc_format"])),
                    ],
                    &["url"],
                    &[READ],
                    false,
                ),
                t(
                    "CONFLUENCE_LIST_PAGE_CHILDREN",
                    ("Unterseiten auflisten", "List child pages"),
                    (
                        "Listet die direkten Unterseiten einer Seite.",
                        "Lists the direct child pages of one page.",
                    ),
                    &[("page_id", s("Numerische Seiten-ID"))],
                    &["page_id"],
                    &[READ],
                    false,
                ),
                t(
                    "CONFLUENCE_LIST_FOOTER_COMMENTS",
                    ("Fuß-Kommentare auflisten", "List footer comments"),
                    (
                        "Listet die Fuß-Kommentare einer Seite (`/footer-comments`).",
                        "Lists the footer comments of one page (`/footer-comments`).",
                    ),
                    &[
                        ("page_id", s("Numerische Seiten-ID")),
                        ("body_format", choice("Body-Format der Kommentare", &["storage", "view", "atlas_doc_format"])),
                    ],
                    &["page_id"],
                    &[READ],
                    false,
                ),
                t(
                    "CONFLUENCE_LIST_INLINE_COMMENTS",
                    ("Inline-Kommentare auflisten", "List inline comments"),
                    (
                        "Listet die Inline-Kommentare einer Seite (`/inline-comments`).",
                        "Lists the inline comments of one page (`/inline-comments`).",
                    ),
                    &[
                        ("page_id", s("Numerische Seiten-ID")),
                        ("body_format", choice("Body-Format der Kommentare", &["storage", "view", "atlas_doc_format"])),
                    ],
                    &["page_id"],
                    &[READ],
                    false,
                ),
                t(
                    "CONFLUENCE_LIST_ATTACHMENTS",
                    ("Anhänge auflisten", "List attachments"),
                    (
                        "Listet die Anhänge einer Seite (`limit=100`).",
                        "Lists the attachments of one page (`limit=100`).",
                    ),
                    &[("page_id", s("Numerische Seiten-ID"))],
                    &["page_id"],
                    &[READ],
                    false,
                ),
                t(
                    "CONFLUENCE_SEARCH_PAGES",
                    ("Inhalte per CQL durchsuchen", "Search content (CQL)"),
                    (
                        "CQL-Suche in Confluence, z. B. `type = \"page\" AND space = \"DOCS\"`.",
                        "CQL search in Confluence, e.g. `type = \"page\" AND space = \"DOCS\"`.",
                    ),
                    &[
                        ("cql", s("CQL-Ausdruck, z. B. type = \"page\" ORDER BY lastmodified DESC")),
                        ("max_results", int("Maximale Trefferzahl (Seitengröße 50)", 1, 1000, 25)),
                        ("expand", s("Kommagetrennte Liste, z. B. body.storage,version,space")),
                    ],
                    &["cql"],
                    &[READ],
                    false,
                ),
                t(
                    "CONFLUENCE_FIND_USER",
                    ("Nutzer suchen", "Find user"),
                    (
                        "Sucht Nutzer per CQL nach Name oder E-Mail.",
                        "Searches users via CQL by name or email.",
                    ),
                    &[
                        ("keyword", s("Benutzername, E-Mail oder Anzeigename")),
                        ("limit", int("Maximale Trefferzahl", 1, 1000, 25)),
                    ],
                    &["keyword"],
                    &[READ],
                    false,
                ),
                t(
                    "CONFLUENCE_LIST_PAGE_TEMPLATES",
                    ("Seiten-Templates auflisten", "List page templates"),
                    (
                        "Listet die Seiten-Templates eines Spaces (`/template/page?spaceKey=`).",
                        "Lists the page templates of one space (`/template/page?spaceKey=`).",
                    ),
                    &[("space_key", s("Space-Schlüssel, z. B. DOCS (aus CONFLUENCE_GET_SPACE)"))],
                    &["space_key"],
                    &[READ],
                    false,
                ),
                t(
                    "CONFLUENCE_GET_TEMPLATE",
                    ("Template lesen", "Get template"),
                    (
                        "Liest ein Seiten-Template inklusive Storage-XHTML.",
                        "Reads one page template including its storage XHTML.",
                    ),
                    &[("template_id", s("Template-ID"))],
                    &["template_id"],
                    &[READ],
                    false,
                ),
                t(
                    "CONFLUENCE_LIST_FOLDERS",
                    ("Ordner auflisten", "List folders"),
                    (
                        "Listet die Ordner unterhalb der Startseite eines Spaces.",
                        "Lists the folders below the home page of one space.",
                    ),
                    &[("homepage_id", s("Startseiten-ID des Spaces, aus CONFLUENCE_GET_SPACE/homepageId"))],
                    &["homepage_id"],
                    &[READ],
                    false,
                ),
                // --- write ---
                t(
                    "CONFLUENCE_CREATE_PAGE",
                    ("Seite anlegen", "Create page"),
                    (
                        "Legt eine Seite in einem Space an; braucht --confirm.",
                        "Creates a page in a space; requires --confirm.",
                    ),
                    &[
                        ("space_id", s("Space-ID")),
                        ("parent_id", s("Optional: Elternseite, unter der die Seite entsteht")),
                        ("title", s("Titel der Seite")),
                        ("body", s("Inhalt im gewählten Format")),
                        ("body_format", choice("Format des Inhalts", &["storage", "wiki", "atlas_doc_format"])),
                        ("status", choice("Veröffentlicht oder Entwurf", &["current", "draft"])),
                    ],
                    &["space_id", "title", "status"],
                    &[WRITE],
                    true,
                ),
                t(
                    "CONFLUENCE_UPDATE_PAGE",
                    ("Seite ändern", "Update page"),
                    (
                        "Schreibt Titel, Body und Status einer Seite; sendet `current_version + 1`; braucht --confirm.",
                        "Writes title, body and status of a page; sends `current_version + 1`; requires --confirm.",
                    ),
                    &[
                        ("page_id", s("Numerische Seiten-ID")),
                        ("current_version", int("Aktuelle Versionsnummer (z. B. aus CONFLUENCE_GET_PAGE_CONTENT)", 1, 1_000_000, 1)),
                        ("title", s("Neuer Titel")),
                        ("body", s("Neuer Inhalt im gewählten Format")),
                        ("body_format", choice("Format des Inhalts", &["storage", "wiki", "atlas_doc_format"])),
                        ("status", choice("Veröffentlicht oder Entwurf", &["current", "draft"])),
                    ],
                    &["page_id", "title"],
                    &[WRITE],
                    true,
                ),
                t(
                    "CONFLUENCE_PUBLISH_DRAFT",
                    ("Entwurf veröffentlichen", "Publish draft"),
                    (
                        "Setzt den Status einer Seite auf `current`; braucht --confirm.",
                        "Sets a page's status to `current`; requires --confirm.",
                    ),
                    &[
                        ("page_id", s("ID der Entwurfsseite")),
                        ("title", s("Titel des Entwurfs (wie bei GET /pages/{id} ausgelesen)")),
                        ("body", s("Storage-Inhalt des Entwurfs (wie bei GET /pages/{id} ausgelesen)")),
                    ],
                    &["page_id", "title"],
                    &[WRITE],
                    true,
                ),
                t(
                    "CONFLUENCE_MOVE_PAGE",
                    ("Seite verschieben", "Move page"),
                    (
                        "Verschiebt eine Seite relativ zu einer Zielseite; braucht --confirm.",
                        "Moves a page relative to a target page; requires --confirm.",
                    ),
                    &[
                        ("page_id", s("Numerische Seiten-ID der zu verschiebenden Seite")),
                        ("target_page_id", s("Numerische Seiten-ID der Zielseite")),
                        ("position", choice("Position relativ zur Zielseite", &["append", "before", "after"])),
                    ],
                    &["page_id", "target_page_id", "position"],
                    &[WRITE],
                    true,
                ),
                t(
                    "CONFLUENCE_DELETE_PAGE",
                    ("Seite löschen", "Delete page"),
                    (
                        "Verschiebt eine Seite in den Papierkorb oder löscht sie endgültig; braucht --confirm.",
                        "Trashes a page or purges it permanently; requires --confirm.",
                    ),
                    &[
                        ("page_id", s("Numerische Seiten-ID")),
                        ("purge", flag("Endgültig aus dem Papierkorb löschen", false)),
                    ],
                    &["page_id"],
                    &[WRITE],
                    true,
                ),
                t(
                    "CONFLUENCE_ADD_COMMENT",
                    ("Kommentar hinzufügen", "Add comment to page"),
                    (
                        "Schreibt einen Fuß-Kommentar auf eine Seite; braucht --confirm.",
                        "Posts a footer comment on a page; requires --confirm.",
                    ),
                    &[
                        ("page_id", s("Numerische Seiten-ID")),
                        ("body", s("Kommentartext im Storage-Format (XHTML); Klartext geht auch")),
                    ],
                    &["page_id", "body"],
                    &[WRITE],
                    true,
                ),
                t(
                    "CONFLUENCE_REPLY_TO_COMMENT",
                    ("Auf Kommentar antworten", "Reply to comment"),
                    (
                        "Antwortet auf einen bestehenden Fuß-Kommentar; braucht --confirm.",
                        "Replies to an existing footer comment; requires --confirm.",
                    ),
                    &[
                        ("parent_comment_id", s("ID des Kommentars, auf den geantwortet wird")),
                        ("body", s("Antworttext im Storage-Format (XHTML)")),
                    ],
                    &["parent_comment_id", "body"],
                    &[WRITE],
                    true,
                ),
                t(
                    "CONFLUENCE_ADD_LABEL",
                    ("Label hinzufügen", "Add label to page"),
                    (
                        "Hängt ein oder mehrere Labels an eine Seite; braucht --confirm.",
                        "Adds one or more labels to a page; requires --confirm.",
                    ),
                    &[
                        ("page_id", s("Numerische Seiten-ID")),
                        ("labels", str_list("Label-Namen, einer pro Zeile")),
                        ("prefix", choice("Label-Präfix", &["global", "my", "team"])),
                    ],
                    &["page_id", "labels"],
                    &[WRITE],
                    true,
                ),
                t(
                    "CONFLUENCE_REMOVE_LABEL",
                    ("Label entfernen", "Remove label from page"),
                    (
                        "Entfernt ein Label von einer Seite; braucht --confirm.",
                        "Removes one label from a page; requires --confirm.",
                    ),
                    &[
                        ("page_id", s("Numerische Seiten-ID")),
                        ("label", s("Name des zu entfernenden Labels")),
                    ],
                    &["page_id", "label"],
                    &[WRITE],
                    true,
                ),
            ],
        };
        Self { spec, site }
    }

    fn v1(&self, uri: &str) -> String {
        format!("{}/wiki/rest/api{uri}", self.site)
    }

    fn v2(&self, uri: &str) -> String {
        format!("{}/wiki/api/v2{uri}", self.site)
    }
}

/// `parsePageIdFromUrl`: `/pages/<id>` first, then `?pageId=<id>`.
fn parse_page_id(url: &str) -> Option<&str> {
    let start = url
        .find("/pages/")
        .map(|i| i + "/pages/".len())
        .or_else(|| url.find("pageId=").map(|i| i + "pageId=".len()))?;
    let rest = &url[start..];
    let end = rest
        .find(|c: char| !c.is_ascii_digit())
        .unwrap_or(rest.len());
    (end > 0).then_some(&rest[..end])
}

/// CQL of `find-user.ts` with `"` escaped as `\"`.
fn user_cql(keyword: &str) -> String {
    let k = keyword.replace('"', "\\\"");
    format!("user.fullname ~ \"{k}\" OR user = \"{k}\"")
}

impl ToolkitAdapter for Confluence {
    fn spec(&self) -> &ToolkitSpec {
        &self.spec
    }

    #[allow(clippy::too_many_lines)]
    fn build_request(&self, tool: &str, args: &Value) -> Result<HttpRequest> {
        let page_id = req_page_id(args);
        let body_format = || -> String {
            opt_str(args, "body_format").unwrap_or("storage").to_string()
        };
        Ok(match tool {
            // AP: src/lib/actions/list-spaces.ts
            "CONFLUENCE_LIST_SPACES" => HttpRequest::new(
                "GET",
                format!(
                    "{}{}",
                    self.v2("/spaces?limit=200"),
                    query(&[
                        ("type", opt_str(args, "type").map(String::from)),
                        ("status", opt_str(args, "status").map(String::from)),
                    ])
                ),
            ),
            // AP: src/lib/common/props.ts (space dropdown: GET /spaces/{spaceId})
            "CONFLUENCE_GET_SPACE" => HttpRequest::new(
                "GET",
                self.v2(&format!("/spaces/{}", path_segment(args, "space_id")?)),
            ),
            // AP: src/lib/common/props.ts (page dropdown: GET /spaces/{spaceId}/pages)
            "CONFLUENCE_LIST_SPACE_PAGES" => HttpRequest::new(
                "GET",
                self.v2(&format!(
                    "/spaces/{}/pages?limit=200",
                    path_segment(args, "space_id")?
                )),
            ),
            // AP: src/lib/actions/find-page-by-title.ts
            "CONFLUENCE_FIND_PAGE_BY_TITLE" => HttpRequest::new(
                "GET",
                format!(
                    "{}{}",
                    self.v2("/pages"),
                    query(&[
                        ("title", Some(req_str(args, "title")?.to_string())),
                        ("limit", Some("25".to_string())),
                        ("space-id", opt_str(args, "space_id").map(String::from)),
                    ])
                ),
            ),
            // AP: src/lib/actions/get-page-content.ts
            "CONFLUENCE_GET_PAGE_CONTENT" => HttpRequest::new(
                "GET",
                format!(
                    "{}{}",
                    self.v2(&format!("/pages/{}", page_id?)),
                    query(&[("body-format", Some("storage".to_string()))])
                ),
            ),
            // AP: src/lib/actions/get-page-by-url.ts
            "CONFLUENCE_GET_PAGE_BY_URL" => {
                let url = req_str(args, "url")?;
                let id = parse_page_id(url)
                    .filter(|d| !d.is_empty())
                    .ok_or_else(|| anyhow::anyhow!("could not extract a page ID from the URL: {url}"))?;
                HttpRequest::new(
                    "GET",
                    format!(
                        "{}{}",
                        self.v2(&format!("/pages/{id}")),
                        query(&[("body-format", opt_str(args, "body_format").map(String::from))])
                    ),
                )
            }
            // AP: src/lib/actions/get-page-content.ts (children of a page)
            "CONFLUENCE_LIST_PAGE_CHILDREN" => {
                HttpRequest::new("GET", self.v2(&format!("/pages/{}/children", page_id?)))
            }
            // AP: src/lib/actions/list-comments.ts (commentType = footer)
            "CONFLUENCE_LIST_FOOTER_COMMENTS" => HttpRequest::new(
                "GET",
                format!(
                    "{}{}",
                    self.v2(&format!("/pages/{}/footer-comments", page_id?)),
                    query(&[
                        ("body-format", Some(body_format())),
                        ("limit", Some("100".to_string())),
                    ])
                ),
            ),
            // AP: src/lib/actions/list-comments.ts (commentType = inline)
            "CONFLUENCE_LIST_INLINE_COMMENTS" => HttpRequest::new(
                "GET",
                format!(
                    "{}{}",
                    self.v2(&format!("/pages/{}/inline-comments", page_id?)),
                    query(&[
                        ("body-format", Some(body_format())),
                        ("limit", Some("100".to_string())),
                    ])
                ),
            ),
            // AP: src/lib/actions/list-attachments.ts
            "CONFLUENCE_LIST_ATTACHMENTS" => HttpRequest::new(
                "GET",
                format!(
                    "{}{}",
                    self.v2(&format!("/pages/{}/attachments", page_id?)),
                    query(&[("limit", Some("100".to_string()))])
                ),
            ),
            // AP: src/lib/actions/search-pages.ts
            "CONFLUENCE_SEARCH_PAGES" => {
                let max = opt_u64(args, "max_results", 25, 1, 1000);
                HttpRequest::new(
                    "GET",
                    format!(
                        "{}{}",
                        self.v1("/content/search"),
                        query(&[
                            ("cql", Some(req_str(args, "cql")?.to_string())),
                            ("start", Some("0".to_string())),
                            ("limit", Some(max.min(50).to_string())),
                            ("expand", opt_str(args, "expand").map(String::from)),
                        ])
                    ),
                )
            }
            // AP: src/lib/actions/find-user.ts
            "CONFLUENCE_FIND_USER" => {
                let limit = opt_u64(args, "limit", 25, 1, 1000);
                HttpRequest::new(
                    "GET",
                    format!(
                        "{}{}",
                        self.v1("/search/user"),
                        query(&[
                            ("cql", Some(user_cql(req_str(args, "keyword")?))),
                            ("limit", Some(limit.to_string())),
                        ])
                    ),
                )
            }
            // AP: src/lib/common/props.ts (template dropdown: GET /template/page?spaceKey=)
            "CONFLUENCE_LIST_PAGE_TEMPLATES" => {
                let key = req_str(args, "space_key")?.trim();
                HttpRequest::new(
                    "GET",
                    format!(
                        "{}{}",
                        self.v1("/template/page?start=0&limit=100"),
                        query(&[("spaceKey", Some(key.to_string()))])
                    ),
                )
            }
            // AP: src/lib/common/props.ts (template variables: GET /template/{templateId})
            "CONFLUENCE_GET_TEMPLATE" => HttpRequest::new(
                "GET",
                self.v1(&format!("/template/{}", path_segment(args, "template_id")?)),
            ),
            // AP: src/lib/common/props.ts (folder dropdown: GET /content/{homepageId}/descendant/folder)
            "CONFLUENCE_LIST_FOLDERS" => HttpRequest::new(
                "GET",
                self.v1(&format!(
                    "/content/{}/descendant/folder?start=0&limit=100",
                    path_segment(args, "homepage_id")?
                )),
            ),
            // AP: src/lib/actions/create-page.ts
            "CONFLUENCE_CREATE_PAGE" => {
                let status = status_of(args)?;
                let mut m = Map::new();
                m.insert("spaceId".into(), json!(path_segment(args, "space_id")?));
                if let Some(parent) = opt_str(args, "parent_id") {
                    m.insert("parentId".into(), json!(parent));
                }
                m.insert("title".into(), json!(req_str(args, "title")?.trim()));
                m.insert("status".into(), json!(status));
                m.insert(
                    "body".into(),
                    body_field(&body_format(), opt_str(args, "body").unwrap_or("")),
                );
                HttpRequest::new("POST", self.v2("/pages")).json(&Value::Object(m))
            }
            // AP: src/lib/actions/update-page.ts (PUT arm; version = current + 1)
            "CONFLUENCE_UPDATE_PAGE" => {
                let id = page_id?;
                let mut m = Map::new();
                m.insert("id".into(), json!(id));
                m.insert("status".into(), json!(status_of(args)?));
                m.insert("title".into(), json!(req_str(args, "title")?.trim()));
                m.insert("body".into(), body_field(&body_format(), opt_str(args, "body").unwrap_or("")));
                let current = opt_u64(args, "current_version", 1, 1, 1_000_000);
                m.insert("version".into(), json!({"number": current + 1}));
                HttpRequest::new("PUT", self.v2(&format!("/pages/{id}"))).json(&Value::Object(m))
            }
            // AP: src/lib/actions/publish-draft.ts (PUT arm, version.number = 1 as in the piece)
            "CONFLUENCE_PUBLISH_DRAFT" => {
                let id = page_id?;
                let m = json!({
                    "id": id,
                    "status": "current",
                    "title": req_str(args, "title")?.trim(),
                    "body": body_field("storage", opt_str(args, "body").unwrap_or("")),
                    "version": {"number": 1},
                });
                HttpRequest::new("PUT", self.v2(&format!("/pages/{id}"))).json(&m)
            }
            // AP: src/lib/actions/move-page.ts
            "CONFLUENCE_MOVE_PAGE" => {
                let position = match opt_str(args, "position") {
                    Some(p @ ("append" | "before" | "after")) => p,
                    _ => bail!("`position` must be append, before or after"),
                };
                let target = path_segment(args, "target_page_id")?;
                let page_id = page_id?;
                if target == page_id {
                    bail!("a page cannot be moved relative to itself");
                }
                HttpRequest::new(
                    "PUT",
                    self.v1(&format!("/content/{page_id}/move/{position}/{target}")),
                )
            }
            // AP: src/lib/actions/delete-page.ts
            "CONFLUENCE_DELETE_PAGE" => {
                let purge = args.get("purge").and_then(Value::as_bool).unwrap_or(false);
                HttpRequest::new(
                    "DELETE",
                    format!(
                        "{}{}",
                        self.v2(&format!("/pages/{}", page_id?)),
                        query(&[("purge", purge.then(|| "true".to_string()))])
                    ),
                )
            }
            // AP: src/lib/actions/add-comment.ts
            "CONFLUENCE_ADD_COMMENT" => HttpRequest::new("POST", self.v2("/footer-comments")).json(
                &json!({"pageId": page_id?, "body": body_field("storage", req_str(args, "body")?)}),
            ),
            // AP: src/lib/actions/reply-to-comment.ts
            "CONFLUENCE_REPLY_TO_COMMENT" => {
                HttpRequest::new("POST", self.v2("/footer-comments")).json(&json!({
                    "parentCommentId": path_segment(args, "parent_comment_id")?,
                    "body": body_field("storage", req_str(args, "body")?),
                }))
            }
            // AP: src/lib/actions/add-label.ts
            "CONFLUENCE_ADD_LABEL" => {
                let names = str_vec(args, "labels");
                if names.is_empty() {
                    bail!("`labels` needs at least one label name");
                }
                let prefix = match opt_str(args, "prefix") {
                    Some(p @ ("global" | "my" | "team")) => p,
                    _ => bail!("`prefix` must be global, my or team"),
                };
                let list: Vec<Value> = names
                    .iter()
                    .map(|n| json!({"prefix": prefix, "name": n}))
                    .collect();
                HttpRequest::new("POST", self.v1(&format!("/content/{}/label", page_id?)))
                    .json(&Value::Array(list))
            }
            // AP: src/lib/actions/remove-label.ts
            "CONFLUENCE_REMOVE_LABEL" => HttpRequest::new(
                "DELETE",
                format!(
                    "{}{}",
                    self.v1(&format!("/content/{}/label", page_id?)),
                    query(&[("name", Some(req_str(args, "label")?.to_string()))])
                ),
            ),
            other => bail!("unknown Confluence tool `{other}`"),
        })
    }

    #[allow(clippy::too_many_lines)]
    fn parse_response(&self, tool: &str, response: &HttpResponse) -> Result<Value> {
        let v = response.json()?;
        Ok(match tool {
            "CONFLUENCE_LIST_SPACES" => collection(&v, flatten_space),
            "CONFLUENCE_GET_SPACE" => flatten_space(&v),
            "CONFLUENCE_LIST_SPACE_PAGES" | "CONFLUENCE_LIST_PAGE_CHILDREN" => {
                collection(&v, flatten_page_ref)
            }
            "CONFLUENCE_FIND_PAGE_BY_TITLE" => {
                let items: Vec<Value> = v["results"]
                    .as_array()
                    .cloned()
                    .unwrap_or_default()
                    .iter()
                    .map(flatten_page_ref)
                    .collect();
                json!({"found": !items.is_empty(), "count": items.len(), "results": items})
            }
            "CONFLUENCE_GET_PAGE_CONTENT" | "CONFLUENCE_GET_PAGE_BY_URL"
            | "CONFLUENCE_CREATE_PAGE" | "CONFLUENCE_UPDATE_PAGE"
            | "CONFLUENCE_PUBLISH_DRAFT" => flatten_page(&v),
            "CONFLUENCE_LIST_FOOTER_COMMENTS" | "CONFLUENCE_LIST_INLINE_COMMENTS" => {
                collection(&v, flatten_comment)
            }
            "CONFLUENCE_ADD_COMMENT" | "CONFLUENCE_REPLY_TO_COMMENT" => flatten_comment(&v),
            "CONFLUENCE_LIST_ATTACHMENTS" => collection(&v, flatten_attachment),
            "CONFLUENCE_SEARCH_PAGES" => collection(&v, flatten_content),
            "CONFLUENCE_FIND_USER" => {
                let users: Vec<Value> = v["results"]
                    .as_array()
                    .cloned()
                    .unwrap_or_default()
                    .iter()
                    .map(|r| flatten_user(&r["user"]))
                    .collect();
                json!({"found": !users.is_empty(), "count": users.len(), "results": users})
            }
            "CONFLUENCE_LIST_PAGE_TEMPLATES" => {
                let items: Vec<Value> = v["results"]
                    .as_array()
                    .cloned()
                    .unwrap_or_default()
                    .iter()
                    .map(|r| {
                        json!({
                            "template_id": r["templateId"], "name": r["name"],
                            "description": r["description"], "template_type": r["templateType"],
                            "space_key": r["space"],
                        })
                    })
                    .collect();
                json!({"count": items.len(), "results": items})
            }
            "CONFLUENCE_GET_TEMPLATE" => json!({
                "template_id": v["templateId"], "name": v["name"], "description": v["description"],
                "template_type": v["templateType"], "space": v["space"],
                "body_representation": v["body"]["storage"]["representation"],
                "body_value": v["body"]["storage"]["value"],
            }),
            "CONFLUENCE_LIST_FOLDERS" => {
                let items: Vec<Value> = v["results"]
                    .as_array()
                    .cloned()
                    .unwrap_or_default()
                    .iter()
                    .map(|r| json!({"id": r["id"], "title": r["title"], "type": r["type"]}))
                    .collect();
                json!({"count": items.len(), "results": items})
            }
            "CONFLUENCE_MOVE_PAGE" => json!({"success": true}),
            "CONFLUENCE_DELETE_PAGE" => json!({"success": true}),
            "CONFLUENCE_ADD_LABEL" => {
                let items: Vec<Value> = v["results"]
                    .as_array()
                    .cloned()
                    .unwrap_or_default()
                    .iter()
                    .map(|r| json!({"prefix": r["prefix"], "name": r["name"], "id": r["label"]}))
                    .collect();
                json!({"count": items.len(), "results": items})
            }
            "CONFLUENCE_REMOVE_LABEL" => json!({"success": true}),
            other => bail!("unknown Confluence tool `{other}`"),
        })
    }
}

fn status_of(args: &Value) -> Result<&'static str> {
    match opt_str(args, "status") {
        Some("current") => Ok("current"),
        Some("draft") => Ok("draft"),
        _ => bail!("`status` must be current or draft"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn c() -> Confluence {
        Confluence::with_api_base("https://acme.atlassian.net")
    }

    #[test]
    fn urls_come_from_the_piece() {
        let x = c();
        let r = x.build_request("CONFLUENCE_GET_PAGE_CONTENT", &json!({"page_id": "12345"})).unwrap();
        assert_eq!(r.url, "https://acme.atlassian.net/wiki/api/v2/pages/12345?body-format=storage");
        let r = x.build_request("CONFLUENCE_LIST_FOLDERS", &json!({"homepage_id": "99"})).unwrap();
        assert_eq!(r.url, "https://acme.atlassian.net/wiki/rest/api/content/99/descendant/folder?start=0&limit=100");
        let r = x.build_request("CONFLUENCE_MOVE_PAGE", &json!({"page_id": "1", "target_page_id": "2", "position": "after"})).unwrap();
        assert_eq!(r.method, "PUT");
        assert_eq!(r.url, "https://acme.atlassian.net/wiki/rest/api/content/1/move/after/2");
        // The v2 root is /wiki/api/v2, the v1 root /wiki/rest/api.
        assert!(x.build_request("CONFLUENCE_LIST_SPACES", &json!({"type": "global"})).unwrap().url
            .starts_with("https://acme.atlassian.net/wiki/api/v2/spaces?limit=200"));
    }

    #[test]
    fn page_id_is_parsed_from_any_url_form() {
        assert_eq!(parse_page_id("https://a.atlassian.net/wiki/spaces/DOCS/pages/12345/T"), Some("12345"));
        assert_eq!(parse_page_id("https://a.atlassian.net/wiki/x?pageId=777&y=1"), Some("777"));
        assert_eq!(parse_page_id("https://a.atlassian.net/wiki/x"), None);
        let r = c()
            .build_request("CONFLUENCE_GET_PAGE_BY_URL", &json!({"url": "https://a.atlassian.net/wiki/pages/42", "body_format": "view"}))
            .unwrap();
        assert_eq!(r.url, "https://acme.atlassian.net/wiki/api/v2/pages/42?body-format=view");
        assert!(c().build_request("CONFLUENCE_GET_PAGE_BY_URL", &json!({"url": "https://a.atlassian.net/wiki/x"})).is_err());
    }

    #[test]
    fn validation_rejects_bad_input() {
        let x = c();
        assert!(x.build_request("CONFLUENCE_GET_PAGE_CONTENT", &json!({"page_id": "../x"})).is_err());
        assert!(x.build_request("CONFLUENCE_GET_PAGE_CONTENT", &json!({})).is_err());
        assert!(x.build_request("CONFLUENCE_CREATE_PAGE", &json!({"space_id": "1", "title": "T", "status": "nope"})).is_err());
        assert!(x.build_request("CONFLUENCE_ADD_LABEL", &json!({"page_id": "1", "labels": [], "prefix": "global"})).is_err());
        assert!(x.build_request("CONFLUENCE_MOVE_PAGE", &json!({"page_id": "1", "target_page_id": "1", "position": "append"})).is_err());
        assert!(x.build_request("CONFLUENCE_MOVE_PAGE", &json!({"page_id": "1", "target_page_id": "2", "position": "sideways"})).is_err());
    }

    #[test]
    fn bodies_match_the_piece() {
        let x = c();
        let r = x.build_request("CONFLUENCE_ADD_COMMENT", &json!({"page_id": "5", "body": "hi"})).unwrap();
        assert_eq!(serde_json::from_slice::<Value>(r.body.as_deref().unwrap()).unwrap(),
            json!({"pageId": "5", "body": {"representation": "storage", "value": "hi"}}));
        let r = x.build_request("CONFLUENCE_ADD_LABEL", &json!({"page_id": "5", "labels": ["a", "b"], "prefix": "team"})).unwrap();
        assert_eq!(serde_json::from_slice::<Value>(r.body.as_deref().unwrap()).unwrap(),
            json!([{"prefix": "team", "name": "a"}, {"prefix": "team", "name": "b"}]));
        let r = x.build_request("CONFLUENCE_UPDATE_PAGE", &json!({"page_id": "5", "current_version": 7, "title": "T", "status": "draft"})).unwrap();
        let b: Value = serde_json::from_slice(r.body.as_deref().unwrap()).unwrap();
        assert_eq!(b["version"]["number"], 8);
        assert_eq!(b["status"], "draft");
        let r = x.build_request("CONFLUENCE_PUBLISH_DRAFT", &json!({"page_id": "5", "title": "T"})).unwrap();
        let b: Value = serde_json::from_slice(r.body.as_deref().unwrap()).unwrap();
        assert_eq!(b["status"], "current");
        assert_eq!(b["version"]["number"], 1);
    }

    #[test]
    fn cql_and_encoded_query_values() {
        let x = c();
        let r = x.build_request("CONFLUENCE_FIND_USER", &json!({"keyword": "Ada \"Ada\" L", "limit": 5})).unwrap();
        assert!(r.url.contains("cql=user.fullname"), "{}", r.url);
        assert!(r.url.contains("%5C%22Ada%5C%22"), "{}", r.url);
        let r = x.build_request("CONFLUENCE_FIND_PAGE_BY_TITLE", &json!({"title": "My Page", "space_id": "77"})).unwrap();
        assert!(r.url.contains("title=My+Page"), "{}", r.url);
        assert!(r.url.contains("space-id=77"), "{}", r.url);
        assert!(r.url.contains("limit=25"), "{}", r.url);
    }

    #[test]
    fn flatteners_read_the_v2_shape() {
        let p = json!({"id": "1", "title": "T", "status": "current", "spaceId": "9",
            "version": {"number": 3}, "body": {"representation": "storage", "value": "<p>x</p>"}});
        assert_eq!(flatten_page(&p)["body_value"], "<p>x</p>");
        assert_eq!(flatten_page(&p)["version_number"], 3);
        let c2 = json!({"results": [{"id": "1", "title": "A"}, {"id": "2", "title": "B"}]});
        assert_eq!(collection(&c2, flatten_page_ref)["count"], 2);
    }
}