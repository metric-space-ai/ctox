//! Microsoft SharePoint toolkit (Microsoft Graph v1.0: `/sites`, `/drives`).
//! Action set modelled on Activepieces `piece-microsoft-sharepoint` (MIT).
//!
//! The Activepieces piece drives the `@microsoft/microsoft-graph-client` JS SDK;
//! every action here maps onto the plain Graph REST call the SDK wraps
//! (`{base}/v1.0/…`, the SDK's default version segment — the piece's own
//! upload action builds the same URL by hand).
//!
//! Cloud environments: the piece authenticates against `https://{cloud}/common/oauth2/v2.0/…`
//! with `cloud` ∈ {`login.microsoftonline.com`, `login.microsoftonline.us`} and calls the
//! matching Graph base URL (`https://graph.microsoft.com` / `https://graph.microsoft.us`).
//! This spec targets the commercial cloud (the piece's default). US Government (GCC High)
//! tenants connect through `SharePoint::with_api_base("https://graph.microsoft.us")` with
//! a token from `login.microsoftonline.us`.
//!
//! Deviations forced by the pure `build_request`/`parse_response` seam (no I/O, one
//! request per call), mirroring how the piece resolves them at runtime:
//!
//! - `move-file` / `copy-item` resolve the literal destination `root` with a
//!   `GET /drives/{id}/root` round trip first; here the caller passes the real folder id
//!   (from `SHAREPOINT_GET_DRIVE_ROOT` or `SHAREPOINT_LIST_FOLDERS`).
//! - `copy-item` polls the async job's `Location` URL until `completed`; the hub runs one
//!   request per call, so the tool reports the accepted 202 and the caller re-checks the
//!   destination drive item.
//! - Dropdown/`DynamicProperties` sources became list tools (`SHAREPOINT_LIST_*`); the
//!   `@odata.nextLink` of a page is returned as `next_link` instead of being followed.
//! - `microsoft_sharepoint_upload_file` is not ported (file transfers are out of scope),
//!   nor are the eight triggers (webhook/polling).
use anyhow::{anyhow, bail, Result};
use serde_json::{json, Map, Value};

use super::common::*;
use super::ToolkitAdapter;
use crate::http::{HttpRequest, HttpResponse};
use crate::model::*;

pub const SLUG: &str = "sharepoint";

const SITE_READ: &str = "Sites.Read.All";
const SITE_RW: &str = "Sites.ReadWrite.All";
const SITE_MANAGE: &str = "Sites.Manage.All";
const FILES_RW: &str = "Files.ReadWrite.All";

pub struct SharePoint {
    spec: ToolkitSpec,
    api_base: String,
}

impl Default for SharePoint {
    fn default() -> Self {
        Self::with_api_base("https://graph.microsoft.com")
    }
}

/// Percent-encode one opaque value (encodeURIComponent semantics; unreserved
/// `A-Za-z0-9-_.~` stay as-is).
fn pct_encode(v: &str) -> String {
    let mut out = String::with_capacity(v.len());
    for b in v.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(b as char)
            }
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

/// Percent-encode a path value while keeping `/` as the segment separator
/// (the piece uses `encodeURI` for drive paths, which would let `?`/`#` through).
fn pct_path(v: &str) -> String {
    let mut out = String::with_capacity(v.len());
    for b in v.bytes() {
        match b {
            b'/' => out.push('/'),
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(b as char)
            }
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

/// Graph ids are opaque: site ids look like `tenant.sharepoint.com,guid,guid`,
/// drive item ids are base64-ish strings. Trimmed, length-bounded and
/// percent-encoded into one path segment.
fn graph_id_opt(args: &Value, key: &str) -> Result<Option<String>> {
    match args
        .get(key)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|v| !v.is_empty())
    {
        Some(v) if v.len() <= 1000 => Ok(Some(pct_encode(v))),
        Some(_) => bail!("invalid argument `{key}`"),
        None => Ok(None),
    }
}

fn graph_id(args: &Value, key: &str) -> Result<String> {
    graph_id_opt(args, key)?.ok_or_else(|| anyhow!("missing argument `{key}`"))
}

/// Ids that end up inside a JSON body (`parentReference`) stay raw — only
/// path/query placements are percent-encoded.
fn raw_id(args: &Value, key: &str) -> Result<String> {
    raw_id_opt(args, key)?.ok_or_else(|| anyhow!("missing argument `{key}`"))
}

fn raw_id_opt(args: &Value, key: &str) -> Result<Option<String>> {
    match args
        .get(key)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|v| !v.is_empty())
    {
        Some(v) if v.len() <= 1000 => Ok(Some(v.to_string())),
        Some(_) => bail!("invalid argument `{key}`"),
        None => Ok(None),
    }
}

/// `?k=v&k2=v2` with constant OData keys and percent-encoded values.
fn odata_query(pairs: &[(&str, Option<String>)]) -> String {
    let mut out = String::new();
    for (k, v) in pairs {
        if let Some(v) = v {
            out.push_str(if out.is_empty() { "?" } else { "&" });
            out.push_str(k);
            out.push('=');
            out.push_str(&pct_encode(v));
        }
    }
    out
}

fn menu(desc: &str, options: &[&str]) -> Value {
    json!({"type": "string", "enum": options, "description": desc})
}

fn checkbox(desc: &str) -> Value {
    json!({"type": "boolean", "description": desc, "default": false})
}

fn columns_prop(desc: &str) -> Value {
    json!({"type": "object", "description": desc, "additionalProperties": true})
}

/// Column values of a list item. Multi-choice columns are arrays; Graph needs
/// the `@odata.type` hint next to them (see the piece's `fieldWithArrayValues`).
fn fields_payload(args: &Value, key: &str) -> Result<Value> {
    let cols = args
        .get(key)
        .and_then(Value::as_object)
        .ok_or_else(|| anyhow!("missing argument `{key}` (object of column values)"))?;
    if cols.is_empty() {
        bail!("`{key}` needs at least one column value");
    }
    let mut fields = Map::new();
    for (k, v) in cols {
        if k.contains('@') {
            bail!("invalid column name `{k}`");
        }
        if v.is_array() {
            fields.insert(format!("{k}@odata.type"), json!("Collection(Edm.String)"));
        }
        fields.insert(k.clone(), v.clone());
    }
    Ok(Value::Object(fields))
}

/// Uniform result for the Graph list calls: one page plus its `@odata.nextLink`.
fn page(value: &Value, map: fn(&Value) -> Value) -> Value {
    let items: Vec<Value> = value["value"]
        .as_array()
        .map(|a| a.iter().map(map).collect())
        .ok_or_else(|| anyhow!("Graph response without `value`"))
        .unwrap_or_default();
    json!({
        "items": items,
        "count": items.len(),
        "next_link": value["@odata.nextLink"],
    })
}

impl SharePoint {
    pub fn with_api_base(api_base: &str) -> Self {
        let t = |slug, title, desc, props: Props<'_>, req: &[&str], scopes: &[&str], m| {
            tool(SLUG, slug, title, desc, props, req, scopes, m)
        };
        let spec = ToolkitSpec {
            slug: SLUG.into(),
            name: "Microsoft SharePoint".into(),
            description: LocalizedText::new(
                "SharePoint-Sites, Dokumentbibliotheken, Ordner und Listen lesen und ändern.",
                "Read and change SharePoint sites, document libraries, folders and lists.",
            ),
            auth_schemes: vec![AuthScheme::OAuth2(OAuth2Scheme {
                authorize_url: "https://login.microsoftonline.com/common/oauth2/v2.0/authorize"
                    .into(),
                token_url: "https://login.microsoftonline.com/common/oauth2/v2.0/token".into(),
                default_scopes: vec![
                    "openid".into(),
                    "email".into(),
                    "profile".into(),
                    "offline_access".into(),
                    SITE_READ.into(),
                    SITE_RW.into(),
                    SITE_MANAGE.into(),
                    FILES_RW.into(),
                ],
                pkce: false,
                scope_separator: " ".into(),
                refreshable: true,
                extra_authorize_params: vec![],
            })],
            tools: vec![
                // --- search / read --------------------------------------------------
                t(
                    "SHAREPOINT_FIND_SITE",
                    ("Site suchen", "Find site"),
                    (
                        "Durchsucht Sites des Mandanten nach einem Namen oder Stichwort.",
                        "Searches the tenant's sites by name or keyword.",
                    ),
                    &[("search_term", s("Site-Name oder Stichwort"))],
                    &["search_term"],
                    &[SITE_READ],
                    false,
                ),
                t(
                    "SHAREPOINT_GET_SITE",
                    ("Site-Informationen lesen", "Get site information"),
                    (
                        "Liest Metadaten einer Site (Titel, URL, Beschreibung …).",
                        "Reads the metadata of one site (title, URL, description, …).",
                    ),
                    &[
                        ("site_id", s("Site-ID (aus SHAREPOINT_FIND_SITE)")),
                        ("select", s("Kommaliste zurückgegebener Felder, z. B. id,displayName,webUrl")),
                    ],
                    &["site_id"],
                    &[SITE_READ],
                    false,
                ),
                t(
                    "SHAREPOINT_LIST_SITES",
                    ("Sites auflisten", "List sites"),
                    (
                        "Listet alle Sites auf, auf die das Konto Zugriff hat.",
                        "Lists all sites the account can access.",
                    ),
                    &[],
                    &[],
                    &[SITE_READ],
                    false,
                ),
                t(
                    "SHAREPOINT_LIST_DRIVES",
                    ("Dokumentbibliotheken auflisten", "List drives"),
                    (
                        "Listet die Dokumentbibliotheken (Drives) einer Site.",
                        "Lists the document libraries (drives) of a site.",
                    ),
                    &[("site_id", s("Site-ID (aus SHAREPOINT_FIND_SITE)"))],
                    &["site_id"],
                    &[SITE_READ],
                    false,
                ),
                t(
                    "SHAREPOINT_LIST_FOLDERS",
                    ("Ordner auflisten", "List folders"),
                    (
                        "Listet die Unterordner auf oberster Ebene einer Bibliothek.",
                        "Lists the top-level subfolders of a drive.",
                    ),
                    &[
                        ("site_id", s("Site-ID")),
                        ("drive_id", s("Drive-ID (aus SHAREPOINT_LIST_DRIVES)")),
                    ],
                    &["site_id", "drive_id"],
                    &[FILES_RW],
                    false,
                ),
                t(
                    "SHAREPOINT_LIST_FILES",
                    ("Dateien auflisten", "List files"),
                    (
                        "Listet die Dateien auf oberster Ebene einer Bibliothek.",
                        "Lists the top-level files of a drive.",
                    ),
                    &[("drive_id", s("Drive-ID (aus SHAREPOINT_LIST_DRIVES)"))],
                    &["drive_id"],
                    &[FILES_RW],
                    false,
                ),
                t(
                    "SHAREPOINT_GET_DRIVE_ROOT",
                    ("Wurzelordner lesen", "Get drive root"),
                    (
                        "Liest den Wurzelordner einer Bibliothek (liefert die Ordner-ID für \"root\").",
                        "Reads the root folder of a drive (resolves the folder id for \"root\").",
                    ),
                    &[("drive_id", s("Drive-ID (aus SHAREPOINT_LIST_DRIVES)"))],
                    &["drive_id"],
                    &[FILES_RW],
                    false,
                ),
                t(
                    "SHAREPOINT_GET_FOLDER_CONTENTS",
                    ("Ordnerinhalte lesen", "Get folder contents"),
                    (
                        "Listet Dateien und Unterordner eines Ordners, seitenweise.",
                        "Lists the files and subfolders of one folder, one page.",
                    ),
                    &[
                        ("site_id", s("Site-ID")),
                        ("folder_id", s("Ordner-ID oder \"root\" für die oberste Ebene")),
                        ("include_custom_fields", checkbox("SharePoint-Spalten mitliefern ($expand=listItem(expand=fields))")),
                        ("top", json!({"type": "integer", "description": "Seitengröße / page size; Graph-Standard ist 200", "minimum": 1, "maximum": 2000})),
                        ("select", s("Kommaliste zurückgegebener Felder, z. B. id,name,size,webUrl")),
                        ("orderby", s("Sortierung, z. B. \"name asc\" oder \"lastModifiedDateTime desc\"")),
                    ],
                    &["site_id", "folder_id"],
                    &[SITE_READ, FILES_RW],
                    false,
                ),
                t(
                    "SHAREPOINT_FIND_FILE",
                    ("Datei finden", "Find file"),
                    (
                        "Findet eine Datei über exakten Pfad oder Namenssuche.",
                        "Finds a file by exact path or by name search.",
                    ),
                    &[
                        ("drive_id", s("Drive-ID (aus SHAREPOINT_LIST_DRIVES)")),
                        ("find_method", menu("\"path\" = exakter Pfad, \"search\" = Namenssuche", &["path", "search"])),
                        ("file_path", s("Pfad ab Wurzel, z. B. Documents/Reports/Q1-Report.docx (nur bei \"path\")")),
                        ("search_query", s("Dateiname oder Stichwort (nur bei \"search\")")),
                        ("search_folder_id", s("Nur in diesem Ordner suchen (nur bei \"search\")")),
                    ],
                    &["drive_id", "find_method"],
                    &[FILES_RW],
                    false,
                ),
                // --- lists ----------------------------------------------------------
                t(
                    "SHAREPOINT_LIST_LISTS",
                    ("Listen auflisten", "List lists"),
                    (
                        "Listet die Listen einer Site mit ID und Name.",
                        "Lists the lists of a site with id and name.",
                    ),
                    &[("site_id", s("Site-ID (aus SHAREPOINT_FIND_SITE)"))],
                    &["site_id"],
                    &[SITE_READ],
                    false,
                ),
                t(
                    "SHAREPOINT_LIST_COLUMNS",
                    ("Listenspalten auflisten", "List columns"),
                    (
                        "Listet die Spalten (Feldschema) einer Liste.",
                        "Lists the columns (field schema) of a list.",
                    ),
                    &[
                        ("site_id", s("Site-ID")),
                        ("list_id", s("Listen-ID (aus SHAREPOINT_LIST_LISTS)")),
                    ],
                    &["site_id", "list_id"],
                    &[SITE_READ],
                    false,
                ),
                t(
                    "SHAREPOINT_LIST_LIST_ITEMS",
                    ("Listeneinträge auflisten", "List list items"),
                    (
                        "Listet die Einträge einer Liste mit Titel.",
                        "Lists the items of a list with their title.",
                    ),
                    &[
                        ("site_id", s("Site-ID")),
                        ("list_id", s("Listen-ID (aus SHAREPOINT_LIST_LISTS)")),
                    ],
                    &["site_id", "list_id"],
                    &[SITE_READ],
                    false,
                ),
                t(
                    "SHAREPOINT_FIND_LIST_ITEM",
                    ("Listeneintrag finden", "Find list item"),
                    (
                        "Findet Listeneinträge über exakte Übereinstimmung des Titels.",
                        "Finds list items by exact Title match.",
                    ),
                    &[
                        ("site_id", s("Site-ID")),
                        ("list_id", s("Listen-ID (aus SHAREPOINT_LIST_LISTS)")),
                        ("search_value", s("Exakter Titel des Eintrags")),
                    ],
                    &["site_id", "list_id", "search_value"],
                    &[SITE_READ],
                    false,
                ),
                t(
                    "SHAREPOINT_LIST_PAGES",
                    ("Seiten auflisten", "List pages"),
                    (
                        "Listet die Site-Seiten einer Site mit ID und Titel.",
                        "Lists the site pages of a site with id and title.",
                    ),
                    &[("site_id", s("Site-ID (aus SHAREPOINT_FIND_SITE)"))],
                    &["site_id"],
                    &[SITE_READ],
                    false,
                ),
                // --- write ----------------------------------------------------------
                t(
                    "SHAREPOINT_CREATE_FOLDER",
                    ("Ordner anlegen", "Create folder"),
                    (
                        "Legt einen Ordner unter einem Pfad an; braucht --confirm.",
                        "Creates a folder under a path; requires --confirm.",
                    ),
                    &[
                        ("drive_id", s("Drive-ID (aus SHAREPOINT_LIST_DRIVES)")),
                        ("parent_folder", s("Elternordner mit Schluss-Schrägstrich, z. B. \"/demo/\"; \"/\" = Wurzel")),
                        ("folder_name", s("Name des neuen Ordners")),
                    ],
                    &["drive_id", "parent_folder", "folder_name"],
                    &[FILES_RW],
                    true,
                ),
                t(
                    "SHAREPOINT_CREATE_LIST",
                    ("Liste anlegen", "Create list"),
                    (
                        "Legt eine neue Liste auf einer Site an; braucht --confirm.",
                        "Creates a new list on a site; requires --confirm.",
                    ),
                    &[
                        ("site_id", s("Site-ID")),
                        ("display_name", s("Listenname")),
                        ("description", s("Listenbeschreibung")),
                    ],
                    &["site_id", "display_name", "description"],
                    &[SITE_RW],
                    true,
                ),
                t(
                    "SHAREPOINT_CREATE_LIST_ITEM",
                    ("Listeneintrag anlegen", "Create list item"),
                    (
                        "Fügt einer Liste einen Eintrag mit Spaltenwerten hinzu; braucht --confirm.",
                        "Adds an item with column values to a list; requires --confirm.",
                    ),
                    &[
                        ("site_id", s("Site-ID")),
                        ("list_id", s("Listen-ID (aus SHAREPOINT_LIST_LISTS)")),
                        ("list_columns", columns_prop("Spaltenwerte der Liste, z. B. {\"Title\": \"Statusbericht\"}; Mehrfachauswahl als Array (Spaltenschema: SHAREPOINT_LIST_COLUMNS)")),
                    ],
                    &["site_id", "list_id", "list_columns"],
                    &[SITE_RW],
                    true,
                ),
                t(
                    "SHAREPOINT_UPDATE_LIST_ITEM",
                    ("Listeneintrag ändern", "Update list item"),
                    (
                        "Setzt Spaltenwerte eines bestehenden Eintrags; braucht --confirm.",
                        "Sets column values of an existing item; requires --confirm.",
                    ),
                    &[
                        ("site_id", s("Site-ID")),
                        ("list_id", s("Listen-ID")),
                        ("list_item_id", s("Eintrags-ID (aus SHAREPOINT_LIST_LIST_ITEMS)")),
                        ("list_columns", columns_prop("Zu setzende Spaltenwerte; Mehrfachauswahl als Array")),
                    ],
                    &["site_id", "list_id", "list_item_id", "list_columns"],
                    &[SITE_RW],
                    true,
                ),
                t(
                    "SHAREPOINT_DELETE_LIST_ITEM",
                    ("Listeneintrag löschen", "Delete list item"),
                    (
                        "Löscht einen Eintrag aus einer Liste; braucht --confirm.",
                        "Deletes an item from a list; requires --confirm.",
                    ),
                    &[
                        ("site_id", s("Site-ID")),
                        ("list_id", s("Listen-ID")),
                        ("list_item_id", s("Eintrags-ID (aus SHAREPOINT_LIST_LIST_ITEMS)")),
                    ],
                    &["site_id", "list_id", "list_item_id"],
                    &[SITE_RW],
                    true,
                ),
                t(
                    "SHAREPOINT_MOVE_FILE",
                    ("Datei verschieben", "Move file"),
                    (
                        "Verschiebt eine Datei in einen anderen Ordner derselben Bibliothek; braucht --confirm.",
                        "Moves a file to another folder of the same drive; requires --confirm.",
                    ),
                    &[
                        ("drive_id", s("Drive-ID (aus SHAREPOINT_LIST_DRIVES)")),
                        ("file_id", s("Datei-ID (aus SHAREPOINT_LIST_FILES)")),
                        ("destination_folder_id", s("Zielordner-ID; \"root\" vorher mit SHAREPOINT_GET_DRIVE_ROOT auflösen")),
                        ("new_name", s("Neuer Name beim Verschieben")),
                    ],
                    &["drive_id", "file_id", "destination_folder_id"],
                    &[FILES_RW],
                    true,
                ),
                t(
                    "SHAREPOINT_COPY_ITEM_WITHIN_SITE",
                    ("Datei/Ordner kopieren (innerhalb der Site)", "Copy file or folder (within site)"),
                    (
                        "Kopiert ein Element in einen Ordner derselben Bibliothek; braucht --confirm.",
                        "Copies an item into a folder of the same drive; requires --confirm.",
                    ),
                    &[
                        ("drive_id", s("Drive-ID (aus SHAREPOINT_LIST_DRIVES)")),
                        ("item_id", s("Element-ID (Datei oder Ordner)")),
                        ("destination_folder_id", s("Zielordner-ID; \"root\" vorher mit SHAREPOINT_GET_DRIVE_ROOT auflösen")),
                        ("new_name", s("Name der Kopie; leer = Originalname")),
                        ("conflict_behavior", menu("Verhalten bei Namenskonflikt", &["fail", "replace", "rename"])),
                    ],
                    &["drive_id", "item_id", "destination_folder_id", "conflict_behavior"],
                    &[FILES_RW],
                    true,
                ),
                t(
                    "SHAREPOINT_COPY_ITEM_ACROSS_SITES",
                    ("Datei/Ordner kopieren (siteübergreifend)", "Copy file or folder (across sites)"),
                    (
                        "Kopiert ein Element in eine andere Site desselben Mandanten; braucht --confirm.",
                        "Copies an item to another site of the same tenant; requires --confirm.",
                    ),
                    &[
                        ("drive_id", s("Quell-Drive-ID")),
                        ("item_id", s("Element-ID (Datei oder Ordner)")),
                        ("destination_drive_id", s("Ziel-Drive-ID")),
                        ("destination_folder_id", s("Zielordner-ID; leer = Wurzel des Ziel-Drives")),
                        ("new_name", s("Name der Kopie; leer = Originalname")),
                        ("conflict_behavior", menu("Verhalten bei Namenskonflikt", &["fail", "replace", "rename"])),
                    ],
                    &["drive_id", "item_id", "destination_drive_id", "conflict_behavior"],
                    &[FILES_RW],
                    true,
                ),
                t(
                    "SHAREPOINT_PUBLISH_PAGE",
                    ("Seite veröffentlichen", "Publish page"),
                    (
                        "Veröffentlicht eine Site-Seite für alle Nutzer; braucht --confirm.",
                        "Publishes a site page for all users; requires --confirm.",
                    ),
                    &[
                        ("site_id", s("Site-ID")),
                        ("page_id", s("Seiten-ID (aus SHAREPOINT_LIST_PAGES)")),
                    ],
                    &["site_id", "page_id"],
                    &[SITE_RW, SITE_MANAGE],
                    true,
                ),
            ],
        };
        Self {
            spec,
            api_base: api_base.trim_end_matches('/').to_string(),
        }
    }

    /// `{base}/v1.0{path}` — the JS SDK's default version segment (the piece's
    /// upload action builds the identical URL by hand).
    fn url(&self, path: &str) -> String {
        format!("{}/v1.0{path}", self.api_base)
    }

    fn conflict_behavior<'a>(args: &'a Value, key: &'a str) -> Result<&'a str> {
        let v = req_str(args, key)?;
        if ["fail", "replace", "rename"].contains(&v) {
            Ok(v)
        } else {
            bail!("`{key}` must be fail, replace or rename")
        }
    }

    /// Folder path below `root:` as the piece concatenates it
    /// (`parentFolder + folderName`); encoded per segment.
    fn folder_path(args: &Value) -> Result<String> {
        let parent = req_str(args, "parent_folder")?.trim();
        let name = req_str(args, "folder_name")?.trim();
        if name.is_empty() {
            bail!("`folder_name` cannot be empty");
        }
        if !parent.is_empty() && !parent.starts_with('/') {
            bail!("`parent_folder` must start with \"/\", for example \"/demo/\"");
        }
        let joined = format!("{parent}{name}");
        if joined.trim_matches('/').is_empty() {
            bail!("`folder_name` cannot be empty");
        }
        Ok(pct_path(&joined))
    }
}

#[allow(clippy::too_many_lines)]
impl SharePoint {
    fn request(&self, tool: &str, args: &Value) -> Result<HttpRequest> {
        Ok(match tool {
            // AP: src/lib/actions/find-site.ts
            "SHAREPOINT_FIND_SITE" => HttpRequest::new(
                "GET",
                self.url(&format!(
                    "/sites?search={}&$select=displayName,id,name,webUrl",
                    pct_encode(req_str(args, "search_term")?.trim())
                )),
            ),
            // AP: src/lib/actions/get-site-information.ts
            "SHAREPOINT_GET_SITE" => {
                let select = opt_str(args, "select").map(|s| {
                    s.split(',')
                        .map(str::trim)
                        .filter(|f| !f.is_empty())
                        .collect::<Vec<_>>()
                        .join(",")
                });
                HttpRequest::new(
                    "GET",
                    self.url(&format!(
                        "/sites/{}{}",
                        graph_id(args, "site_id")?,
                        odata_query(&[("$select", select)]),
                    )),
                )
            }
            // AP: src/lib/common/index.ts (siteId dropdown)
            "SHAREPOINT_LIST_SITES" => HttpRequest::new(
                "GET",
                self.url("/sites?search=*&$select=displayName,id,name"),
            ),
            // AP: src/lib/common/index.ts (driveId dropdown)
            "SHAREPOINT_LIST_DRIVES" => HttpRequest::new(
                "GET",
                self.url(&format!(
                    "/sites/{}/drives?$select=id,name",
                    graph_id(args, "site_id")?
                )),
            ),
            // AP: src/lib/common/index.ts (destinationFolderId / folderId dropdowns)
            "SHAREPOINT_LIST_FOLDERS" => HttpRequest::new(
                "GET",
                self.url(&format!(
                    "/sites/{}/drives/{}/root/children{}",
                    graph_id(args, "site_id")?,
                    graph_id(args, "drive_id")?,
                    odata_query(&[
                        ("$filter", Some("folder ne null".to_string())),
                        ("$select", Some("id,name".to_string())),
                    ]),
                )),
            ),
            // AP: src/lib/actions/move-file.ts (fileId dropdown)
            "SHAREPOINT_LIST_FILES" => HttpRequest::new(
                "GET",
                self.url(&format!(
                    "/drives/{}/root/children?$select=id,name,file",
                    graph_id(args, "drive_id")?
                )),
            ),
            // AP: src/lib/actions/copy-item-within-site.ts, move-file.ts (root resolution)
            "SHAREPOINT_GET_DRIVE_ROOT" => HttpRequest::new(
                "GET",
                self.url(&format!("/drives/{}/root", graph_id(args, "drive_id")?)),
            ),
            // AP: src/lib/actions/get-folder-contents.ts
            "SHAREPOINT_GET_FOLDER_CONTENTS" => {
                let base = if opt_str(args, "folder_id") == Some("root") {
                    format!("/sites/{}/drive/root/children", graph_id(args, "site_id")?)
                } else {
                    format!(
                        "/sites/{}/drive/items/{}/children",
                        graph_id(args, "site_id")?,
                        graph_id(args, "folder_id")?
                    )
                };
                let top = args
                    .get("top")
                    .and_then(Value::as_u64)
                    .map(|t| t.clamp(1, 2000).to_string());
                let expand = if args.get("include_custom_fields").and_then(Value::as_bool)
                    == Some(true)
                {
                    "?$expand=listItem(expand=fields)"
                } else {
                    ""
                };
                let select = opt_str(args, "select").map(String::from);
                let orderby = opt_str(args, "orderby").map(String::from);
                let rest = odata_query(&[("$top", top), ("$select", select), ("$orderby", orderby)]);
                let query = match (expand.is_empty(), rest.is_empty()) {
                    (true, true) => String::new(),
                    (true, false) => rest,
                    (false, true) => expand.to_string(),
                    (false, false) => format!("{expand}&{}", rest.trim_start_matches('?')),
                };
                HttpRequest::new("GET", self.url(&base) + &query)
            }
            // AP: src/lib/actions/find-file.ts
            "SHAREPOINT_FIND_FILE" => {
                let drive = graph_id(args, "drive_id")?;
                match req_str(args, "find_method")? {
                    "path" => {
                        let raw = req_str(args, "file_path")?.trim();
                        let path = raw.strip_prefix('/').unwrap_or(raw);
                        if path.is_empty() {
                            bail!("`file_path` is required when find_method is \"path\"");
                        }
                        HttpRequest::new(
                            "GET",
                            self.url(&format!("/drives/{drive}/root:/{}", pct_path(path))),
                        )
                    }
                    "search" => {
                        let q = pct_encode(req_str(args, "search_query")?.trim());
                        if q.is_empty() {
                            bail!("`search_query` is required when find_method is \"search\"");
                        }
                        let scope = match graph_id_opt(args, "search_folder_id")? {
                            Some(folder) => format!("items/{folder}/"),
                            None => "root/".to_string(),
                        };
                        HttpRequest::new(
                            "GET",
                            self.url(&format!("/drives/{drive}/{scope}search(q='{q}')")),
                        )
                    }
                    other => bail!("`find_method` must be path or search, got \"{other}\""),
                }
            }
            // AP: src/lib/actions/search-list-item.ts
            "SHAREPOINT_FIND_LIST_ITEM" => {
                // OData escaping of single quotes, as in the piece; the whole
                // `filter` expression is percent-encoded like the SDK's query string.
                let title = req_str(args, "search_value")?.replace('\'', "''");
                let mut req = HttpRequest::new(
                    "GET",
                    self.url(&format!(
                        "/sites/{}/lists/{}/items?$expand=fields&filter={}",
                        graph_id(args, "site_id")?,
                        graph_id(args, "list_id")?,
                        pct_encode(&format!("fields/Title eq '{title}'")),
                    )),
                );
                req.headers.insert(
                    "prefer".into(),
                    "HonorNonIndexedQueriesWarningMayFailRandomly".into(),
                );
                req
            }
            // AP: src/lib/common/index.ts (listId dropdown)
            "SHAREPOINT_LIST_LISTS" => HttpRequest::new(
                "GET",
                self.url(&format!(
                    "/sites/{}/lists?$select=displayName,id",
                    graph_id(args, "site_id")?
                )),
            ),
            // AP: src/lib/common/index.ts (listColumns dynamic props)
            "SHAREPOINT_LIST_COLUMNS" => HttpRequest::new(
                "GET",
                self.url(&format!(
                    "/sites/{}/lists/{}/columns",
                    graph_id(args, "site_id")?,
                    graph_id(args, "list_id")?
                )),
            ),
            // AP: src/lib/common/index.ts (listItemId dropdown)
            "SHAREPOINT_LIST_LIST_ITEMS" => HttpRequest::new(
                "GET",
                self.url(&format!(
                    "/sites/{}/lists/{}/items?$select=id&$expand=fields($select=Title)",
                    graph_id(args, "site_id")?,
                    graph_id(args, "list_id")?
                )),
            ),
            // AP: src/lib/common/index.ts (pageId dropdown)
            "SHAREPOINT_LIST_PAGES" => HttpRequest::new(
                "GET",
                self.url(&format!(
                    "/sites/{}/pages?$select=id,title",
                    graph_id(args, "site_id")?
                )),
            ),
            // AP: src/lib/actions/create-folder.ts
            "SHAREPOINT_CREATE_FOLDER" => HttpRequest::new(
                "PATCH",
                self.url(&format!(
                    "/drives/{}/root:{}",
                    graph_id(args, "drive_id")?,
                    Self::folder_path(args)?
                )),
            )
            .json(&json!({"folder": {}})),
            // AP: src/lib/actions/create-list.ts
            "SHAREPOINT_CREATE_LIST" => HttpRequest::new(
                "POST",
                self.url(&format!("/sites/{}/lists", graph_id(args, "site_id")?)),
            )
            .json(&json!({
                "displayName": req_str(args, "display_name")?,
                "description": req_str(args, "description")?,
            })),
            // AP: src/lib/actions/create-list-item.ts
            "SHAREPOINT_CREATE_LIST_ITEM" => HttpRequest::new(
                "POST",
                self.url(&format!(
                    "/sites/{}/lists/{}/items",
                    graph_id(args, "site_id")?,
                    graph_id(args, "list_id")?
                )),
            )
            .json(&json!({"fields": fields_payload(args, "list_columns")?})),
            // AP: src/lib/actions/update-list-item.ts
            "SHAREPOINT_UPDATE_LIST_ITEM" => HttpRequest::new(
                "PATCH",
                self.url(&format!(
                    "/sites/{}/lists/{}/items/{}/fields",
                    graph_id(args, "site_id")?,
                    graph_id(args, "list_id")?,
                    graph_id(args, "list_item_id")?
                )),
            )
            .json(&fields_payload(args, "list_columns")?),
            // AP: src/lib/actions/delete-list-item.ts
            "SHAREPOINT_DELETE_LIST_ITEM" => HttpRequest::new(
                "DELETE",
                self.url(&format!(
                    "/sites/{}/lists/{}/items/{}",
                    graph_id(args, "site_id")?,
                    graph_id(args, "list_id")?,
                    graph_id(args, "list_item_id")?
                )),
            ),
            // AP: src/lib/actions/move-file.ts
            "SHAREPOINT_MOVE_FILE" => {
                let mut body = json!({
                    "parentReference": {"id": raw_id(args, "destination_folder_id")?},
                });
                if let Some(name) = opt_str(args, "new_name") {
                    body["name"] = json!(name);
                }
                HttpRequest::new(
                    "PATCH",
                    self.url(&format!(
                        "/drives/{}/items/{}",
                        graph_id(args, "drive_id")?,
                        graph_id(args, "file_id")?
                    )),
                )
                .json(&body)
            }
            // AP: src/lib/actions/copy-item-within-site.ts
            "SHAREPOINT_COPY_ITEM_WITHIN_SITE" => {
                let behavior = Self::conflict_behavior(args, "conflict_behavior")?;
                let mut body = json!({
                    "parentReference": {"id": raw_id(args, "destination_folder_id")?},
                });
                if let Some(name) = opt_str(args, "new_name") {
                    body["name"] = json!(name);
                }
                HttpRequest::new(
                    "POST",
                    self.url(&format!(
                        "/drives/{}/items/{}/copy?@microsoft.graph.conflictBehavior={}",
                        graph_id(args, "drive_id")?,
                        graph_id(args, "item_id")?,
                        behavior
                    )),
                )
                .json(&body)
            }
            // AP: src/lib/actions/copy-item.ts
            "SHAREPOINT_COPY_ITEM_ACROSS_SITES" => {
                let behavior = Self::conflict_behavior(args, "conflict_behavior")?;
                let mut parent = json!({"driveId": raw_id(args, "destination_drive_id")?});
                if let Some(folder) = raw_id_opt(args, "destination_folder_id")? {
                    parent["id"] = json!(folder);
                }
                let mut body = json!({"parentReference": parent});
                if let Some(name) = opt_str(args, "new_name") {
                    body["name"] = json!(name);
                }
                HttpRequest::new(
                    "POST",
                    self.url(&format!(
                        "/drives/{}/items/{}/copy?@microsoft.graph.conflictBehavior={}",
                        graph_id(args, "drive_id")?,
                        graph_id(args, "item_id")?,
                        behavior
                    )),
                )
                .json(&body)
            }
            // AP: src/lib/actions/publish-page.ts
            "SHAREPOINT_PUBLISH_PAGE" => HttpRequest::new(
                "POST",
                self.url(&format!(
                    "/sites/{}/pages/{}/microsoft.graph.sitePage/publish",
                    graph_id(args, "site_id")?,
                    graph_id(args, "page_id")?
                )),
            )
            .json(&json!({})),
            other => bail!("unknown SharePoint tool `{other}`"),
        })
    }
}

/// Columns of a list item with the flattened type tags of the piece's
/// `listColumns` dynamic props.
fn flatten_column(c: &Value) -> Value {
    let kind = ["boolean", "text", "dateTime", "choice", "number", "currency"]
        .into_iter()
        .find(|k| c[k].is_object())
        .map(|k| json!(k))
        .unwrap_or(Value::Null);
    json!({
        "id": c["id"],
        "name": c["name"],
        "display_name": c["displayName"],
        "description": c["description"],
        "read_only": c["readOnly"],
        "required": c["required"],
        "type": kind,
        "choices": c["choice"]["choices"],
    })
}

fn flatten_list_item(item: &Value) -> Value {
    json!({
        "id": item["id"],
        "title": item["fields"]["Title"],
        "web_url": item["webUrl"],
        "created_at": item["createdDateTime"],
        "updated_at": item["lastModifiedDateTime"],
        "fields": item["fields"],
    })
}

impl ToolkitAdapter for SharePoint {
    fn spec(&self) -> &ToolkitSpec {
        &self.spec
    }

    fn build_request(&self, tool: &str, args: &Value) -> Result<HttpRequest> {
        self.request(tool, args)
    }

    fn parse_response(&self, tool: &str, response: &HttpResponse) -> Result<Value> {
        let body = response.json()?;
        let missing = |what: &str| anyhow!("Graph response without {what}");
        Ok(match tool {
            "SHAREPOINT_FIND_SITE" => {
                let sites: Vec<Value> = body["value"]
                    .as_array()
                    .ok_or_else(|| missing("`value`"))?
                    .iter()
                    .map(|s| {
                        json!({"id": s["id"], "display_name": s["displayName"],
                            "name": s["name"], "web_url": s["webUrl"]})
                    })
                    .collect();
                json!({"sites": sites, "count": sites.len(), "next_link": body["@odata.nextLink"]})
            }
            "SHAREPOINT_GET_SITE" | "SHAREPOINT_GET_DRIVE_ROOT" => body,
            "SHAREPOINT_LIST_SITES" => page(&body, |s| {
                json!({"id": s["id"], "display_name": s["displayName"]})
            }),
            "SHAREPOINT_LIST_DRIVES" => page(&body, |d| json!({"id": d["id"], "name": d["name"]})),
            "SHAREPOINT_LIST_FOLDERS" => {
                page(&body, |f| json!({"id": f["id"], "name": f["name"]}))
            }
            // The piece keeps only entries with a `file` facet (its dropdown filter).
            "SHAREPOINT_LIST_FILES" => {
                let files: Vec<Value> = body["value"]
                    .as_array()
                    .ok_or_else(|| missing("`value`"))?
                    .iter()
                    .filter(|f| f["file"].is_object())
                    .map(|f| json!({"id": f["id"], "name": f["name"], "size": f["size"]}))
                    .collect();
                json!({"items": files, "count": files.len(), "next_link": body["@odata.nextLink"]})
            }
            "SHAREPOINT_GET_FOLDER_CONTENTS" => {
                let items: Vec<Value> = body["value"]
                    .as_array()
                    .ok_or_else(|| missing("`value`"))?
                    .clone();
                json!({
                    "items": items.clone(),
                    "count": items.len(),
                    "next_link": body["@odata.nextLink"],
                })
            }
            // A 404 in path mode is an empty result in the piece, not an error.
            "SHAREPOINT_FIND_FILE" => {
                if response.status == 404 {
                    json!({"success": true, "files": []})
                } else if body["value"].is_array() {
                    json!({"success": true, "files": body["value"], "next_link": body["@odata.nextLink"]})
                } else {
                    json!({"success": true, "files": [body]})
                }
            }
            "SHAREPOINT_LIST_LISTS" => page(&body, |l| {
                json!({"id": l["id"], "display_name": l["displayName"], "web_url": l["webUrl"]})
            }),
            "SHAREPOINT_LIST_COLUMNS" => page(&body, flatten_column),
            "SHAREPOINT_LIST_LIST_ITEMS" => page(&body, flatten_list_item),
            "SHAREPOINT_FIND_LIST_ITEM" => page(&body, flatten_list_item),
            "SHAREPOINT_LIST_PAGES" => page(&body, |p| {
                json!({
                    "id": p["id"],
                    "title": if p["title"].is_null() { json!(format!("Page {}", p["id"].as_str().unwrap_or(""))) } else { p["title"].clone() },
                })
            }),
            "SHAREPOINT_CREATE_FOLDER" | "SHAREPOINT_CREATE_LIST" | "SHAREPOINT_CREATE_LIST_ITEM"
            | "SHAREPOINT_UPDATE_LIST_ITEM" => body,
            "SHAREPOINT_DELETE_LIST_ITEM" => {
                if body.is_null() {
                    json!({"deleted": true})
                } else {
                    json!({"deleted": true, "item": body})
                }
            }
            "SHAREPOINT_MOVE_FILE" => {
                json!({"success": true, "message": "File moved successfully", "item": body})
            }
            // Graph answers the initial copy request with `202 Accepted`, an empty
            // body and the monitor URL in the `Location` header; a small payload can
            // also be answered synchronously with the new item.
            "SHAREPOINT_COPY_ITEM_WITHIN_SITE" | "SHAREPOINT_COPY_ITEM_ACROSS_SITES" => {
                if body.is_null() {
                    json!({"accepted": true, "status": "in_progress"})
                } else if body["id"].is_string() {
                    json!({"accepted": true, "status": "completed", "item": body})
                } else {
                    json!({"accepted": true, "status": "in_progress", "operation": body})
                }
            }
            "SHAREPOINT_PUBLISH_PAGE" => {
                json!({"success": true, "message": "Page published successfully"})
            }
            other => bail!("unknown SharePoint tool `{other}`"),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sp() -> SharePoint {
        SharePoint::with_api_base("https://graph.test")
    }

    #[test]
    fn opaque_ids_and_paths_are_encoded() {
        let s = sp();
        // Site ids carry commas: `tenant.sharepoint.com,guid,guid`.
        let r = s
            .build_request(
                "SHAREPOINT_GET_SITE",
                &json!({"site_id": "contoso.sharepoint.com,2C712604-1370-44E7-A1F5-426573FDA80A,2D2244C3-2515-48EA-B5FE-587B55A1956C"}),
            )
            .unwrap();
        assert_eq!(
            r.url,
            "https://graph.test/v1.0/sites/contoso.sharepoint.com%2C2C712604-1370-44E7-A1F5-426573FDA80A%2C2D2244C3-2515-48EA-B5FE-587B55A1956C"
        );

        let r = s
            .build_request(
                "SHAREPOINT_CREATE_FOLDER",
                &json!({"drive_id": "d1", "parent_folder": "/General/Projects/",
                    "folder_name": "Q3 Reports"}),
            )
            .unwrap();
        assert_eq!(r.url, "https://graph.test/v1.0/drives/d1/root:/General/Projects/Q3%20Reports");
        assert_eq!(r.method, "PATCH");
        let body: Value = serde_json::from_slice(r.body.as_deref().unwrap()).unwrap();
        assert_eq!(body, json!({"folder": {}}));

        // `?`, `#` and `%` in a folder name cannot change the URL shape.
        let r = s
            .build_request(
                "SHAREPOINT_FIND_FILE",
                &json!({"drive_id": "d1", "find_method": "path", "file_path": "Docs/a?b#c.docx"}),
            )
            .unwrap();
        assert_eq!(r.url, "https://graph.test/v1.0/drives/d1/root:/Docs/a%3Fb%23c.docx");

        let r = s
            .build_request(
                "SHAREPOINT_FIND_FILE",
                &json!({"drive_id": "d1", "find_method": "search",
                    "search_query": "Q1 Report'", "search_folder_id": "f1"}),
            )
            .unwrap();
        assert_eq!(
            r.url,
            "https://graph.test/v1.0/drives/d1/items/f1/search(q='Q1%20Report%27')"
        );
    }

    #[test]
    fn validation() {
        let s = sp();
        assert!(s
            .build_request(
                "SHAREPOINT_FIND_FILE",
                &json!({"drive_id": "d1", "find_method": "path"})
            )
            .is_err());
        assert!(s
            .build_request(
                "SHAREPOINT_FIND_FILE",
                &json!({"drive_id": "d1", "find_method": "search"})
            )
            .is_err());
        assert!(s
            .build_request(
                "SHAREPOINT_FIND_FILE",
                &json!({"drive_id": "d1", "find_method": "regex"})
            )
            .is_err());
        assert!(s
            .build_request(
                "SHAREPOINT_CREATE_FOLDER",
                &json!({"drive_id": "d1", "parent_folder": "demo/", "folder_name": "x"})
            )
            .is_err());
        assert!(s
            .build_request(
                "SHAREPOINT_CREATE_FOLDER",
                &json!({"drive_id": "d1", "parent_folder": "/", "folder_name": " "})
            )
            .is_err());
        assert!(s
            .build_request(
                "SHAREPOINT_COPY_ITEM_WITHIN_SITE",
                &json!({"drive_id": "d1", "item_id": "i1", "destination_folder_id": "f1",
                    "conflict_behavior": "overwrite"})
            )
            .is_err());
        assert!(s
            .build_request(
                "SHAREPOINT_CREATE_LIST_ITEM",
                &json!({"site_id": "s1", "list_id": "l1", "list_columns": {}})
            )
            .is_err());
        assert!(s
            .build_request("SHAREPOINT_GET_FOLDER_CONTENTS", &json!({"site_id": "s1"}))
            .is_err());
    }

    #[test]
    fn fields_and_headers() {
        let s = sp();
        let r = s
            .build_request(
                "SHAREPOINT_CREATE_LIST_ITEM",
                &json!({"site_id": "s1", "list_id": "l1",
                    "list_columns": {"Title": "Report", "Tags": ["a", "b"]}}),
            )
            .unwrap();
        let body: Value = serde_json::from_slice(r.body.as_deref().unwrap()).unwrap();
        assert_eq!(body["fields"]["Title"], "Report");
        assert_eq!(body["fields"]["Tags@odata.type"], "Collection(Edm.String)");
        assert_eq!(body["fields"]["Tags"], json!(["a", "b"]));

        let r = s
            .build_request(
                "SHAREPOINT_FIND_LIST_ITEM",
                &json!({"site_id": "s1", "list_id": "l1", "search_value": "O'Brien"}),
            )
            .unwrap();
        assert_eq!(
            r.url,
            "https://graph.test/v1.0/sites/s1/lists/l1/items?$expand=fields&filter=fields%2FTitle%20eq%20%27O%27%27Brien%27"
        );
        assert_eq!(
            r.headers.get("prefer").map(String::as_str),
            Some("HonorNonIndexedQueriesWarningMayFailRandomly")
        );

        let r = s
            .build_request(
                "SHAREPOINT_PUBLISH_PAGE",
                &json!({"site_id": "s1", "page_id": "p1"}),
            )
            .unwrap();
        assert_eq!(
            r.url,
            "https://graph.test/v1.0/sites/s1/pages/p1/microsoft.graph.sitePage/publish"
        );
    }

    #[test]
    fn parsing() {
        let s = sp();
        let resp = HttpResponse {
            status: 404,
            body: b"{\"error\": {\"code\": \"itemNotFound\"}}".to_vec(),
            session_id: None,
        };
        let out = s
            .parse_response("SHAREPOINT_FIND_FILE", &resp)
            .expect("404 is an empty result");
        assert_eq!(out, json!({"success": true, "files": []}));

        let resp = HttpResponse {
            status: 202,
            body: Vec::new(),
            session_id: None,
        };
        let out = s.parse_response("SHAREPOINT_COPY_ITEM_WITHIN_SITE", &resp).unwrap();
        assert_eq!(out["accepted"], json!(true));
        assert_eq!(out["status"], "in_progress");

        let resp = HttpResponse {
            status: 204,
            body: Vec::new(),
            session_id: None,
        };
        let out = s.parse_response("SHAREPOINT_DELETE_LIST_ITEM", &resp).unwrap();
        assert_eq!(out, json!({"deleted": true}));
    }
}
