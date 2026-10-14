//! Microsoft OneDrive toolkit (Microsoft Graph drive API v1.0). Action set
//! modelled on Activepieces `microsoft-onedrive` (MIT).
//!
//! Cloud / base URL: the piece picks the Graph host from a per-connection
//! `cloud` prop (`login.microsoftonline.com` -> `https://graph.microsoft.com`,
//! `login.microsoftonline.us` -> `https://graph.microsoft.us`). CTOX has no
//! per-connection props, so the commercial cloud is the default and a US
//! Government (GCC High) connection is constructed with
//! `OneDrive::with_api_base("https://graph.microsoft.us")` — the matching
//! authorize/token host is `https://login.microsoftonline.us/common/oauth2/v2.0/*`.
//!
//! Not ported (see the module tests): file up-/downloads (`upload-file`,
//! `upload-file`, `onedrive-upload-file`, `onedrive-replace-file-content`,
//! `onedrive-create-text-file`, `download-file`, `onedrive-download-file`,
//! `onedrive-download-file-as-format`, `onedrive-download-item-version`),
//! the async copy pair (`onedrive-copy-item`, `onedrive-get-copy-status` — both
//! need the `Location` response header, which the pure request/parse seam does
//! not expose), the legacy SDK-driven human actions (`copy-file`,
//! `list-files`), and `custom_api_call`.
use anyhow::{anyhow, bail, Result};
use base64::Engine as _;
use serde_json::{json, Value};

use super::common::*;
use super::ToolkitAdapter;
use crate::http::{HttpRequest, HttpResponse};
use crate::model::*;

pub const SLUG: &str = "onedrive";
/// The single delegated scope of the piece (`lib/auth.ts`).
const FILES_RW: &str = "Files.ReadWrite";

pub struct OneDrive {
    spec: ToolkitSpec,
    api_base: String,
}

impl Default for OneDrive {
    fn default() -> Self {
        Self::with_api_base("https://graph.microsoft.com")
    }
}

/// `encodeURIComponent` semantics: `A-Za-z0-9-_.!~*'()` stay literal.
fn enc(v: &str) -> String {
    let mut out = String::with_capacity(v.len());
    for b in v.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' => out.push(b as char),
            b'-' | b'_' | b'.' | b'!' | b'~' | b'*' | b'\'' | b'(' | b')' => out.push(b as char),
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

/// One opaque Microsoft Graph identifier as a single path segment.
///
/// `path_segment` is deliberately too strict for Graph: item, version,
/// permission and bundle IDs are base64-derived and routinely contain `!` and
/// `=`. The value is therefore trimmed, length-bounded and percent-encoded
/// whole, which keeps it inside its own path segment.
fn graph_segment(args: &Value, key: &str) -> Result<String> {
    let v = args
        .get(key)
        .and_then(Value::as_str)
        .ok_or_else(|| anyhow!("missing argument `{key}`"))?
        .trim();
    if v.is_empty() || v.len() > 1000 {
        bail!("invalid argument `{key}`");
    }
    Ok(enc(v))
}

/// `?k=v&k2=v2` with percent-encoded values (OData keys keep their spelling).
fn odata_query(pairs: &[(&str, Option<String>)]) -> String {
    let mut out = String::new();
    for (k, v) in pairs {
        if let Some(v) = v {
            out.push_str(if out.is_empty() { "?" } else { "&" });
            out.push_str(k);
            out.push('=');
            out.push_str(&enc(v));
        }
    }
    out
}

fn menu(desc: &str, options: &[&str]) -> Value {
    json!({"type": "string", "enum": options, "description": desc})
}
fn boolean(desc: &str) -> Value {
    json!({"type": "boolean", "description": desc})
}

fn opt_one_of<'a>(args: &'a Value, key: &str, allowed: &[&str]) -> Result<Option<&'a str>> {
    match opt_str(args, key) {
        Some(v) if allowed.contains(&v) => Ok(Some(v)),
        Some(_) => bail!("invalid argument `{key}`"),
        None => Ok(None),
    }
}
fn req_one_of<'a>(args: &'a Value, key: &str, allowed: &[&str]) -> Result<&'a str> {
    opt_one_of(args, key, allowed)?.ok_or_else(|| anyhow!("missing argument `{key}`"))
}

// --- response mapping (`oneDriveApi.toItem` and friends) --------------------

fn item(v: &Value) -> Value {
    let present = |k: &str| v.get(k).is_some_and(|x| !x.is_null());
    let kind = if present("folder") {
        "folder"
    } else if present("package") {
        "package"
    } else if present("remoteItem") {
        "remote"
    } else {
        "file"
    };
    json!({
        "id": v["id"],
        "name": v["name"],
        "type": kind,
        "mime_type": v["file"]["mimeType"].clone(),
        "size": v["size"].clone(),
        "web_url": v["webUrl"].clone(),
        "created_at": v["createdDateTime"].clone(),
        "modified_at": v["lastModifiedDateTime"].clone(),
        "child_count": v["folder"]["childCount"].clone(),
        "parent_id": v["parentReference"]["id"].clone(),
        "parent_path": v["parentReference"]["path"].clone(),
        "drive_id": v["parentReference"]["driveId"].clone(),
        "drive_type": v["parentReference"]["driveType"].clone(),
        "created_by_name": v["createdBy"]["user"]["displayName"].clone(),
        "created_by_email": v["createdBy"]["user"]["email"].clone(),
        "modified_by_name": v["lastModifiedBy"]["user"]["displayName"].clone(),
        "modified_by_email": v["lastModifiedBy"]["user"]["email"].clone(),
        "description": v["description"].clone(),
        "deleted": v.get("deleted").is_some_and(|d| !d.is_null()),
        "remote_item_id": v["remoteItem"]["id"].clone(),
        "remote_drive_id": v["remoteItem"]["parentReference"]["driveId"].clone(),
    })
}

fn items_page(v: &Value, map: fn(&Value) -> Value) -> Value {
    let items: Vec<Value> = v["value"].as_array().map(|a| a.iter().map(map).collect()).unwrap_or_default();
    json!({
        "items": items.clone(),
        "count": items.len(),
        "next_page_token": v["@odata.nextLink"].clone(),
    })
}

fn drive(v: &Value) -> Value {
    json!({
        "id": v["id"],
        "name": v["name"].clone(),
        "drive_type": v["driveType"].clone(),
        "web_url": v["webUrl"].clone(),
        "created_at": v["createdDateTime"].clone(),
        "modified_at": v["lastModifiedDateTime"].clone(),
        "owner_name": v["owner"]["user"]["displayName"].clone(),
        "owner_email": v["owner"]["user"]["email"].clone(),
        "quota_total": v["quota"]["total"].clone(),
        "quota_used": v["quota"]["used"].clone(),
        "quota_remaining": v["quota"]["remaining"].clone(),
        "quota_deleted": v["quota"]["deleted"].clone(),
        "quota_state": v["quota"]["state"].clone(),
    })
}

fn permission(v: &Value) -> Value {
    let grantee = if v.get("grantedToV2").is_some_and(|x| !x.is_null()) {
        &v["grantedToV2"]["user"]
    } else {
        &v["grantedTo"]["user"]
    };
    let identities = if v.get("grantedToIdentitiesV2").is_some_and(|x| !x.is_null()) {
        &v["grantedToIdentitiesV2"]
    } else {
        &v["grantedToIdentities"]
    };
    let names: Vec<String> = identities
        .as_array()
        .map(|a| {
            a.iter()
                .filter_map(|i| {
                    let e = i["user"]["email"].as_str();
                    let d = i["user"]["displayName"].as_str();
                    e.or(d).map(String::from)
                })
                .collect()
        })
        .unwrap_or_default();
    json!({
        "id": v["id"],
        "roles": v["roles"].clone(),
        "link_type": v["link"]["type"].clone(),
        "link_scope": v["link"]["scope"].clone(),
        "link_web_url": v["link"]["webUrl"].clone(),
        "link_prevents_download": v["link"]["preventsDownload"].clone(),
        "granted_to_name": grantee["displayName"].clone(),
        "granted_to_email": grantee["email"].clone(),
        "granted_to_id": grantee["id"].clone(),
        "granted_to_identities": names.join(", "),
        "invitation_email": v["invitation"]["email"].clone(),
        "inherited": v.get("inheritedFrom").is_some_and(|x| !x.is_null()),
        "inherited_from_id": v["inheritedFrom"]["id"].clone(),
        "inherited_from_path": v["inheritedFrom"]["path"].clone(),
        "has_password": v["hasPassword"].clone(),
        "expiration_at": v["expirationDateTime"].clone(),
        "share_id": v["shareId"].clone(),
    })
}

fn version(v: &Value) -> Value {
    json!({
        "version_id": v["id"],
        "modified_at": v["lastModifiedDateTime"].clone(),
        "size": v["size"].clone(),
        "modified_by_name": v["lastModifiedBy"]["user"]["displayName"].clone(),
        "modified_by_email": v["lastModifiedBy"]["user"]["email"].clone(),
    })
}

fn thumbnail(v: &Value) -> Value {
    let size = |k: &str| json!({"url": v[k]["url"].clone(), "width": v[k]["width"].clone(), "height": v[k]["height"].clone()});
    json!({
        "thumbnail_set_id": v["id"].clone(),
        "small": size("small"),
        "medium": size("medium"),
        "large": size("large"),
    })
}

fn bundle(v: &Value) -> Value {
    let mut o = item(v);
    o["bundle_child_count"] = v["bundle"]["childCount"].clone();
    o["is_album"] = json!(v["bundle"]["album"].is_object());
    o
}

impl OneDrive {
    pub fn with_api_base(api_base: &str) -> Self {
        let t = |slug, title, desc, props: Props<'_>, req: &[&str], mutating| {
            tool(SLUG, slug, title, desc, props, req, &[FILES_RW], mutating)
        };
        fn item_id() -> (&'static str, Value) {
            ("item_id", s("Element-ID aus Suche/Ordnerinhalt, oder \"root\". / item ID from search or list folder contents, or \"root\""))
        }
        fn path_prop() -> (&'static str, Value) {
            ("path", s("Pfad ab Drive-Wurzel, z.B. Documents/report.docx / path from the drive root, e.g. Documents/report.docx"))
        }
        fn page_token() -> (&'static str, Value) {
            ("page_token", s("nextPageToken des vorherigen Aufrufs / nextPageToken of the previous call"))
        }

        let spec = ToolkitSpec {
            slug: SLUG.into(),
            name: "Microsoft OneDrive".into(),
            description: LocalizedText::new(
                "OneDrive-Dateien und -Ordner lesen, suchen und verwalten (Microsoft Graph).",
                "Read, search and manage OneDrive files and folders (Microsoft Graph).",
            ),
            auth_schemes: vec![AuthScheme::OAuth2(OAuth2Scheme {
                authorize_url: "https://login.microsoftonline.com/common/oauth2/v2.0/authorize".into(),
                token_url: "https://login.microsoftonline.com/common/oauth2/v2.0/token".into(),
                default_scopes: vec![
                    FILES_RW.into(),
                    "offline_access".into(),
                    "openid".into(),
                    "email".into(),
                    "profile".into(),
                ],
                pkce: false,
                scope_separator: " ".into(),
                refreshable: true,
                extra_authorize_params: vec![],
            })],
            tools: vec![
                // --- read / search -------------------------------------------------
                t(
                    "ONEDRIVE_GET_ITEM",
                    ("Datei oder Ordner abrufen", "Get file or folder"),
                    (
                        "Metadaten einer Datei oder eines Ordners über ID oder Pfad.",
                        "Reads the metadata of one file or folder by ID or path.",
                    ),
                    &[item_id(), path_prop()],
                    &[],
                    false,
                ),
                t(
                    "ONEDRIVE_GET_DRIVE",
                    ("Laufwerk abrufen", "Get drive"),
                    (
                        "Liest das verbundene OneDrive-Laufwerk inkl. Typ und Speicherkontingent.",
                        "Reads the connected OneDrive drive including its type and storage quota.",
                    ),
                    &[],
                    &[],
                    false,
                ),
                t(
                    "ONEDRIVE_GET_SPECIAL_FOLDER",
                    ("Sonderordner abrufen", "Get special folder"),
                    (
                        "Löst einen bekannten OneDrive-Ordner (Documents, Photos, ...) inklusive ID auf.",
                        "Resolves a well-known OneDrive folder (Documents, Photos, ...) to its ID.",
                    ),
                    &[(
                        "folder",
                        menu("Sonderordner / well-known folder", &["documents", "photos", "cameraroll", "music", "recordings"]),
                    )],
                    &["folder"],
                    false,
                ),
                t(
                    "ONEDRIVE_LIST_DRIVES",
                    ("Eigene Laufwerke auflisten", "List my drives"),
                    (
                        "Listet die Laufwerke des angemeldeten Kontos.",
                        "Lists the drives that belong to the signed-in user.",
                    ),
                    &[page_token()],
                    &[],
                    false,
                ),
                t(
                    "ONEDRIVE_LIST_FOLDER_CHILDREN",
                    ("Ordnerinhalt auflisten", "List folder contents"),
                    (
                        "Listet die direkten Kinder eines Ordners (ohne Angabe: die Drive-Wurzel).",
                        "Lists the direct children of a folder (the drive root when none is given).",
                    ),
                    &[
                        ("folder_id", s("Ordner-ID; leer = Drive-Wurzel / folder ID, empty = drive root")),
                        ("folder_path", s("Ordnerpfad ab Drive-Wurzel / folder path from the drive root")),
                        ("page_size", int("Elemente pro Seite (1-999) / items per page (1-999)", 1, 999, 100)),
                        page_token(),
                    ],
                    &[],
                    false,
                ),
                t(
                    "ONEDRIVE_SEARCH_ITEMS",
                    ("Dateien und Ordner suchen", "Search files and folders"),
                    (
                        "Durchsucht OneDrive nach Name, Metadaten oder Inhalt.",
                        "Searches OneDrive by name, metadata or content.",
                    ),
                    &[
                        ("query", s("Suchtext / search text")),
                        ("scope", menu("Suchumfang / search scope", &["my_files", "include_shared"])),
                        ("page_size", int("Treffer pro Seite (1-999) / results per page (1-999)", 1, 999, 50)),
                        page_token(),
                    ],
                    &["query"],
                    false,
                ),
                t(
                    "ONEDRIVE_LIST_DRIVE_CHANGES",
                    ("Laufwerkänderungen auflisten (Delta)", "List drive changes (delta)"),
                    (
                        "Listet geänderte Elemente seit einem gespeicherten Delta-Token, Seite für Seite.",
                        "Lists items changed in the drive since a saved delta token, one page per call.",
                    ),
                    &[
                        ("page_token", s("nextPageToken oder deltaToken eines vorherigen Aufrufs / nextPageToken or deltaToken of a previous call")),
                        ("start_from_now", boolean("Ohne Token nur ein deltaToken liefern / without a token return only a deltaToken")),
                    ],
                    &[],
                    false,
                ),
                t(
                    "ONEDRIVE_LIST_ITEM_PERMISSIONS",
                    ("Berechtigungen auflisten", "List file permissions"),
                    (
                        "Listet auf, wer Zugriff auf eine Datei oder einen Ordner hat, inklusive Freigabelinks.",
                        "Lists who has access to a file or folder, including sharing links.",
                    ),
                    &[item_id(), path_prop(), page_token()],
                    &[],
                    false,
                ),
                t(
                    "ONEDRIVE_LIST_ITEM_THUMBNAILS",
                    ("Miniaturansichten abrufen", "Get file thumbnails"),
                    (
                        "Liefert die Vorschaubild-URLs (klein, mittel, groß) einer Datei.",
                        "Returns the thumbnail image URLs (small, medium, large) of a file.",
                    ),
                    &[item_id(), path_prop()],
                    &[],
                    false,
                ),
                t(
                    "ONEDRIVE_LIST_ITEM_VERSIONS",
                    ("Dateiversionen auflisten", "List file versions"),
                    (
                        "Listet die gespeicherten Versionen einer Datei, neueste zuerst.",
                        "Lists the saved versions of a file, newest first.",
                    ),
                    &[item_id(), path_prop(), page_token()],
                    &[],
                    false,
                ),
                t(
                    "ONEDRIVE_RESOLVE_SHARING_LINK",
                    ("Freigabelink auflösen", "Resolve sharing link"),
                    (
                        "Ermittelt die Datei bzw. den Ordner hinter einem OneDrive- oder SharePoint-Link.",
                        "Looks up the file or folder behind a OneDrive or SharePoint sharing link.",
                    ),
                    &[("sharing_url", s("Freigabelink oder Share-Token (u!/s!) / sharing URL or share token (u!/s!)"))],
                    &["sharing_url"],
                    false,
                ),
                t(
                    "ONEDRIVE_LIST_BUNDLES",
                    ("Alben und Bundles auflisten", "List albums and bundles"),
                    (
                        "Listet Fotoalben und Bundles eines persönlichen OneDrive (nur Microsoft-Konten).",
                        "Lists the photo albums and bundles of a personal OneDrive (Microsoft accounts only).",
                    ),
                    &[page_token()],
                    &[],
                    false,
                ),
                // --- write ---------------------------------------------------------
                t(
                    "ONEDRIVE_CREATE_FOLDER",
                    ("Ordner anlegen", "Create folder"),
                    (
                        "Legt einen Ordner an oder gibt einen vorhandenen Ordner gleichen Namens zurück.",
                        "Creates a folder or returns the existing folder with that name.",
                    ),
                    &[
                        ("parent_folder_id", s("Übergeordnete Ordner-ID; leer = Drive-Wurzel / parent folder ID, empty = drive root")),
                        ("name", s("Ordnername (ein Name, kein Pfad) / folder name (a single name, not a path)")),
                    ],
                    &["name"],
                    true,
                ),
                t(
                    "ONEDRIVE_UPDATE_ITEM",
                    ("Datei oder Ordner umbenennen", "Rename / update file or folder"),
                    (
                        "Benennt eine Datei oder einen Ordner um und/oder setzt die Beschreibung.",
                        "Renames a file or folder and/or sets its description.",
                    ),
                    &[
                        item_id(),
                        path_prop(),
                        ("name", s("Neuer Name, leer = unverändert / new name, empty keeps it")),
                        ("description", s("Neue Beschreibung (nur persönliches OneDrive) / new description (personal OneDrive only)")),
                    ],
                    &[],
                    true,
                ),
                t(
                    "ONEDRIVE_MOVE_ITEM",
                    ("Datei oder Ordner verschieben", "Move file or folder"),
                    (
                        "Verschiebt eine Datei oder einen Ordner in einen anderen Ordner.",
                        "Moves a file or folder into another folder of the same drive.",
                    ),
                    &[
                        item_id(),
                        path_prop(),
                        ("destination_folder_id", s("Zielordner-ID (Wurzel-ID über ONEDRIVE_GET_ITEM mit item_id=\"root\") / destination folder ID (root ID via ONEDRIVE_GET_ITEM with item_id=\"root\")")),
                        ("new_name", s("Optionaler neuer Name / optional new name")),
                    ],
                    &["destination_folder_id"],
                    true,
                ),
                t(
                    "ONEDRIVE_DELETE_ITEM",
                    ("Datei oder Ordner löschen", "Delete file or folder"),
                    (
                        "Verschiebt eine Datei oder einen Ordner in den Papierkorb.",
                        "Moves a file or folder to the recycle bin.",
                    ),
                    &[item_id(), path_prop()],
                    &[],
                    true,
                ),
                t(
                    "ONEDRIVE_CREATE_SHARING_LINK",
                    ("Freigabelink erstellen", "Create sharing link"),
                    (
                        "Erstellt einen Freigabelink (Ansicht, Bearbeiten, Einbetten) für ein Element.",
                        "Creates a sharing link (view, edit, embed) for an item.",
                    ),
                    &[
                        item_id(),
                        path_prop(),
                        ("link_type", menu("Linktyp / link type", &["view", "edit", "embed"])),
                        ("scope", menu("Wer den Link öffnen darf / who can open the link", &["anonymous", "organization", "users"])),
                        ("password", s("Optionales Passwort (nur persönliches OneDrive) / optional password (personal OneDrive only)")),
                        ("expiration_date_time", s("Optionales Ablaufdatum (ISO 8601) / optional expiration (ISO 8601)")),
                    ],
                    &["link_type"],
                    true,
                ),
                t(
                    "ONEDRIVE_INVITE_TO_ITEM",
                    ("Mit Personen teilen", "Share with people (invite)"),
                    (
                        "Gibt bestimmten Personen Lese- oder Schreibzugriff auf eine Datei oder einen Ordner.",
                        "Grants specific people read or write access to a file or folder.",
                    ),
                    &[
                        item_id(),
                        path_prop(),
                        ("recipients", str_list("E-Mail-Adressen der Empfänger / recipient email addresses")),
                        ("role", menu("Rolle / role", &["read", "write"])),
                        ("send_invitation", boolean("OneDrive-E-Mail senden / let OneDrive send the invitation email")),
                        ("message", s("Text der Einladungs-E-Mail / message of the invitation email")),
                    ],
                    &["recipients", "role"],
                    true,
                ),
                t(
                    "ONEDRIVE_UPDATE_ITEM_PERMISSION",
                    ("Berechtigungsrolle ändern", "Update permission role"),
                    (
                        "Setzt eine bestehende Berechtigung auf Lesen oder Schreiben.",
                        "Sets an existing permission to read or write.",
                    ),
                    &[
                        item_id(),
                        path_prop(),
                        ("permission_id", s("Berechtigungs-ID aus ONEDRIVE_LIST_ITEM_PERMISSIONS / permission ID from ONEDRIVE_LIST_ITEM_PERMISSIONS")),
                        ("role", menu("Neue Rolle / new role", &["read", "write"])),
                    ],
                    &["permission_id", "role"],
                    true,
                ),
                t(
                    "ONEDRIVE_DELETE_ITEM_PERMISSION",
                    ("Berechtigung entfernen", "Remove permission (stop sharing)"),
                    (
                        "Entfernt eine Berechtigung bzw. widerruft einen Freigabelink eines Elements.",
                        "Removes a permission or revokes a sharing link of an item.",
                    ),
                    &[
                        item_id(),
                        path_prop(),
                        ("permission_id", s("Berechtigungs-ID aus ONEDRIVE_LIST_ITEM_PERMISSIONS / permission ID from ONEDRIVE_LIST_ITEM_PERMISSIONS")),
                    ],
                    &["permission_id"],
                    true,
                ),
                t(
                    "ONEDRIVE_RESTORE_ITEM_VERSION",
                    ("Dateiversion wiederherstellen", "Restore file version"),
                    (
                        "Macht eine frühere Version einer Datei zur aktuellen Version.",
                        "Restores a previous version of a file as the current version.",
                    ),
                    &[
                        item_id(),
                        path_prop(),
                        ("version_id", s("Versions-ID aus ONEDRIVE_LIST_ITEM_VERSIONS / version ID from ONEDRIVE_LIST_ITEM_VERSIONS")),
                    ],
                    &["version_id"],
                    true,
                ),
                t(
                    "ONEDRIVE_CHECKOUT_ITEM",
                    ("Datei auschecken", "Check out file"),
                    (
                        "Checkt eine Datei aus, damit sie bearbeitet werden kann.",
                        "Checks a file out for editing.",
                    ),
                    &[item_id(), path_prop()],
                    &[],
                    true,
                ),
                t(
                    "ONEDRIVE_CHECKIN_ITEM",
                    ("Datei einchecken", "Check in file"),
                    (
                        "Checkt eine ausgecheckte Datei ein und veröffentlicht die Änderungen.",
                        "Checks a checked-out file in and publishes the changes.",
                    ),
                    &[
                        item_id(),
                        path_prop(),
                        ("comment", s("Optionaler Check-in-Kommentar / optional check-in comment")),
                    ],
                    &[],
                    true,
                ),
                t(
                    "ONEDRIVE_DISCARD_CHECKOUT",
                    ("Auschecken verwerfen", "Discard check-out"),
                    (
                        "Verwirft die Änderungen einer ausgecheckten Datei.",
                        "Throws away the changes of a checked-out file.",
                    ),
                    &[item_id(), path_prop()],
                    &[],
                    true,
                ),
                t(
                    "ONEDRIVE_CREATE_BUNDLE",
                    ("Bundle anlegen", "Create bundle"),
                    (
                        "Gruppiert vorhandene Dateien in einem persönlichen OneDrive zu einem Bundle.",
                        "Groups existing files in a personal OneDrive into a bundle.",
                    ),
                    &[
                        ("name", s("Bundle-Name / bundle name")),
                        ("item_ids", str_list("IDs von mindestens zwei Dateien / IDs of at least two files")),
                    ],
                    &["name", "item_ids"],
                    true,
                ),
                t(
                    "ONEDRIVE_ADD_ITEM_TO_BUNDLE",
                    ("Element zum Bundle hinzufügen", "Add item to bundle"),
                    (
                        "Fügt eine vorhandene Datei einem Bundle oder Album hinzu.",
                        "Adds an existing file to a bundle or album.",
                    ),
                    &[
                        ("bundle_id", s("Bundle-ID aus ONEDRIVE_LIST_BUNDLES / bundle ID from ONEDRIVE_LIST_BUNDLES")),
                        ("item_id", s("ID der hinzuzufügenden Datei / ID of the file to add")),
                    ],
                    &["bundle_id", "item_id"],
                    true,
                ),
                t(
                    "ONEDRIVE_REMOVE_ITEM_FROM_BUNDLE",
                    ("Element aus Bundle entfernen", "Remove item from bundle"),
                    (
                        "Entfernt eine Datei aus einem Bundle oder Album, ohne sie zu löschen.",
                        "Removes a file from a bundle or album without deleting it.",
                    ),
                    &[
                        ("bundle_id", s("Bundle-ID aus ONEDRIVE_LIST_BUNDLES / bundle ID from ONEDRIVE_LIST_BUNDLES")),
                        ("item_id", s("ID der zu entfernenden Datei / ID of the file to remove")),
                    ],
                    &["bundle_id", "item_id"],
                    true,
                ),
            ],
        };
        Self { spec, api_base: api_base.trim_end_matches('/').into() }
    }

    fn url(&self, path: &str) -> String {
        format!("{}/v1.0{path}", self.api_base)
    }

    /// `oneDriveApi.itemPath` — ID first, else path relative to the drive root.
    fn item_path(&self, args: &Value) -> Result<String> {
        if let Some(id) = opt_str(args, "item_id") {
            let id = id.trim();
            if !id.is_empty() {
                return Ok(if id == "root" {
                    "/me/drive/root".to_string()
                } else {
                    format!("/me/drive/items/{}", enc(id))
                });
            }
        }
        if let Some(p) = opt_str(args, "path") {
            let trimmed = p.trim().trim_matches('/');
            if !trimmed.is_empty() {
                let segs: Vec<String> = trimmed.split('/').map(enc).collect();
                return Ok(format!("/me/drive/root:/{}:", segs.join("/")));
            }
        }
        bail!("provide either an item ID or a path")
    }

    /// `oneDriveApi.folderPath` — an absent/empty folder ID means the drive root.
    fn folder_path(&self, args: &Value, key: &str) -> String {
        match opt_str(args, key) {
            Some(id) if !id.trim().is_empty() => self
                .item_path(&json!({ "item_id": id.trim() }))
                .unwrap_or_else(|_| "/me/drive/root".into()),
            _ => "/me/drive/root".into(),
        }
    }

    /// `oneDriveApi.request`'s `resolveUrl`: an `@odata.nextLink` / delta link is
    /// followed verbatim, but only when it really belongs to this connection.
    fn page_link(&self, args: &Value) -> Result<Option<String>> {
        let Some(token) = opt_str(args, "page_token") else {
            return Ok(None);
        };
        let token = token.trim();
        if token.is_empty() {
            return Ok(None);
        }
        let prefix = format!("{}/v1.0/", self.api_base);
        if !token.starts_with(&prefix) {
            bail!("The token is not a valid OneDrive link for this connection. Pass the nextPageToken/deltaToken of a previous call unchanged.");
        }
        Ok(Some(token.to_string()))
    }
}

impl ToolkitAdapter for OneDrive {
    fn spec(&self) -> &ToolkitSpec {
        &self.spec
    }

    fn build_request(&self, tool: &str, args: &Value) -> Result<HttpRequest> {
        Ok(match tool {
            // AP: src/lib/actions/onedrive-get-item.ts
            "ONEDRIVE_GET_ITEM" => {
                HttpRequest::new("GET", self.url(&self.item_path(args)?))
            }
            // AP: src/lib/actions/onedrive-get-drive.ts
            "ONEDRIVE_GET_DRIVE" => HttpRequest::new("GET", self.url("/me/drive")),
            // AP: src/lib/actions/onedrive-get-special-folder.ts
            "ONEDRIVE_GET_SPECIAL_FOLDER" => HttpRequest::new(
                "GET",
                self.url(&format!(
                    "/me/drive/special/{}",
                    crate::toolkits::path_segment(args, "folder")?
                )),
            ),
            // AP: src/lib/actions/onedrive-list-drives.ts
            "ONEDRIVE_LIST_DRIVES" => match self.page_link(args)? {
                Some(link) => HttpRequest::new("GET", link),
                None => HttpRequest::new("GET", self.url("/me/drives")),
            },
            // AP: src/lib/actions/onedrive-list-folder-children.ts
            "ONEDRIVE_LIST_FOLDER_CHILDREN" => match self.page_link(args)? {
                Some(link) => HttpRequest::new("GET", link),
                None => {
                    let folder = match opt_str(args, "folder_id") {
                        Some(id) if !id.trim().is_empty() => self
                            .item_path(&json!({ "item_id": id.trim() }))?,
                        _ => match opt_str(args, "folder_path") {
                            Some(p) if !p.trim().is_empty() => {
                                self.item_path(&json!({ "path": p }))?
                            }
                            _ => "/me/drive/root".to_string(),
                        },
                    };
                    let q = odata_query(&[(
                        "$top",
                        Some(opt_u64(args, "page_size", 100, 1, 999).to_string()),
                    )]);
                    HttpRequest::new("GET", self.url(&format!("{folder}/children{q}")))
                }
            },
            // AP: src/lib/actions/onedrive-search-items.ts
            "ONEDRIVE_SEARCH_ITEMS" => match self.page_link(args)? {
                Some(link) => HttpRequest::new("GET", link),
                None => {
                    let root = match opt_str(args, "scope") {
                        Some("include_shared") => "/me/drive",
                        _ => "/me/drive/root",
                    };
                    let text = enc(&req_str(args, "query")?.replace('\'', "''"));
                    let q = odata_query(&[(
                        "$top",
                        Some(opt_u64(args, "page_size", 50, 1, 999).to_string()),
                    )]);
                    HttpRequest::new(
                        "GET",
                        self.url(&format!("{root}/search(q='{text}'){q}")),
                    )
                }
            },
            // AP: src/lib/actions/onedrive-list-drive-changes.ts
            "ONEDRIVE_LIST_DRIVE_CHANGES" => match self.page_link(args)? {
                Some(link) => HttpRequest::new("GET", link),
                None => {
                    let start_from_now = args
                        .get("start_from_now")
                        .and_then(Value::as_bool)
                        .unwrap_or(false);
                    let q = if start_from_now {
                        odata_query(&[("token", Some("latest".into()))])
                    } else {
                        String::new()
                    };
                    HttpRequest::new("GET", self.url(&format!("/me/drive/root/delta{q}")))
                }
            },
            // AP: src/lib/actions/onedrive-list-item-permissions.ts
            "ONEDRIVE_LIST_ITEM_PERMISSIONS" => match self.page_link(args)? {
                Some(link) => HttpRequest::new("GET", link),
                None => HttpRequest::new("GET", self.url(&format!("{}/permissions", self.item_path(args)?))),
            },
            // AP: src/lib/actions/onedrive-list-item-thumbnails.ts
            "ONEDRIVE_LIST_ITEM_THUMBNAILS" => {
                HttpRequest::new("GET", self.url(&format!("{}/thumbnails", self.item_path(args)?)))
            }
            // AP: src/lib/actions/onedrive-list-item-versions.ts
            "ONEDRIVE_LIST_ITEM_VERSIONS" => match self.page_link(args)? {
                Some(link) => HttpRequest::new("GET", link),
                None => HttpRequest::new("GET", self.url(&format!("{}/versions", self.item_path(args)?))),
            },
            // AP: src/lib/actions/onedrive-resolve-sharing-link.ts
            "ONEDRIVE_RESOLVE_SHARING_LINK" => {
                let value = req_str(args, "sharing_url")?.trim();
                let share_id = if value.starts_with("u!") || value.starts_with("s!") {
                    value.to_string()
                } else {
                    format!(
                        "u!{}",
                        base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(value.as_bytes())
                    )
                };
                HttpRequest::new(
                    "GET",
                    self.url(&format!("/shares/{}/driveItem", enc(&share_id))),
                )
                .header("prefer", "redeemSharingLinkIfNecessary")
            }
            // AP: src/lib/actions/onedrive-list-bundles.ts
            "ONEDRIVE_LIST_BUNDLES" => match self.page_link(args)? {
                Some(link) => HttpRequest::new("GET", link),
                None => HttpRequest::new("GET", self.url("/me/drive/bundles")),
            },
            // AP: src/lib/actions/onedrive-create-folder.ts
            "ONEDRIVE_CREATE_FOLDER" => {
                let name = req_str(args, "name")?.trim().to_string();
                if name.is_empty() {
                    bail!("invalid argument `name`");
                }
                let parent = self.folder_path(args, "parent_folder_id");
                HttpRequest::new("POST", self.url(&format!("{parent}/children")))
                    .json(&json!({
                        "name": name,
                        "folder": {},
                        "@microsoft.graph.conflictBehavior": "fail",
                    }))
            }
            // AP: src/lib/actions/onedrive-update-item.ts
            "ONEDRIVE_UPDATE_ITEM" => {
                let mut body = serde_json::Map::new();
                if let Some(name) = opt_str(args, "name").map(str::trim).filter(|n| !n.is_empty()) {
                    body.insert("name".into(), json!(name));
                }
                if let Some(desc) = opt_str(args, "description") {
                    body.insert("description".into(), json!(desc));
                }
                if body.is_empty() {
                    bail!("provide a new name or a description to update");
                }
                HttpRequest::new("PATCH", self.url(&self.item_path(args)?))
                    .json(&Value::Object(body))
            }
            // AP: src/lib/actions/onedrive-move-item.ts
            "ONEDRIVE_MOVE_ITEM" => {
                let mut body = json!({
                    "parentReference": {"id": graph_segment(args, "destination_folder_id")?}
                });
                if let Some(name) = opt_str(args, "new_name").map(str::trim).filter(|n| !n.is_empty()) {
                    body["name"] = json!(name);
                }
                HttpRequest::new("PATCH", self.url(&self.item_path(args)?)).json(&body)
            }
            // AP: src/lib/actions/onedrive-delete-item.ts
            "ONEDRIVE_DELETE_ITEM" => {
                HttpRequest::new("DELETE", self.url(&self.item_path(args)?))
            }
            // AP: src/lib/actions/onedrive-create-sharing-link.ts
            "ONEDRIVE_CREATE_SHARING_LINK" => {
                let mut body = json!({
                    "type": req_one_of(args, "link_type", &["view", "edit", "embed"])?,
                    "retainInheritedPermissions": true,
                });
                if let Some(scope) = opt_one_of(args, "scope", &["anonymous", "organization", "users"])? {
                    body["scope"] = json!(scope);
                }
                if let Some(pw) = opt_str(args, "password") {
                    body["password"] = json!(pw);
                }
                if let Some(exp) = opt_str(args, "expiration_date_time") {
                    body["expirationDateTime"] = json!(exp);
                }
                HttpRequest::new(
                    "POST",
                    self.url(&format!("{}/createLink", self.item_path(args)?)),
                )
                .json(&body)
            }
            // AP: src/lib/actions/onedrive-invite-to-item.ts
            "ONEDRIVE_INVITE_TO_ITEM" => {
                let emails: Vec<String> = str_vec(args, "recipients")
                    .into_iter()
                    .map(|e| e.trim().to_string())
                    .filter(|e| !e.is_empty())
                    .collect();
                if emails.is_empty() {
                    bail!("provide at least one recipient email address");
                }
                let mut body = json!({
                    "recipients": emails.iter().map(|e| json!({"email": e})).collect::<Vec<_>>(),
                    "roles": [req_one_of(args, "role", &["read", "write"])?],
                    "requireSignIn": true,
                    "sendInvitation": args.get("send_invitation").and_then(Value::as_bool).unwrap_or(false),
                    "retainInheritedPermissions": true,
                });
                if let Some(message) = opt_str(args, "message") {
                    body["message"] = json!(message);
                }
                HttpRequest::new("POST", self.url(&format!("{}/invite", self.item_path(args)?)))
                    .json(&body)
            }
            // AP: src/lib/actions/onedrive-update-item-permission.ts
            "ONEDRIVE_UPDATE_ITEM_PERMISSION" => {
                let role = req_one_of(args, "role", &["read", "write"])?;
                let permission_id = graph_segment(args, "permission_id")?;
                HttpRequest::new(
                    "PATCH",
                    self.url(&format!(
                        "{}/permissions/{permission_id}",
                        self.item_path(args)?
                    )),
                )
                .json(&json!({"roles": [role]}))
            }
            // AP: src/lib/actions/onedrive-delete-item-permission.ts
            "ONEDRIVE_DELETE_ITEM_PERMISSION" => {
                let permission_id = graph_segment(args, "permission_id")?;
                HttpRequest::new(
                    "DELETE",
                    self.url(&format!(
                        "{}/permissions/{permission_id}",
                        self.item_path(args)?
                    )),
                )
            }
            // AP: src/lib/actions/onedrive-restore-item-version.ts
            "ONEDRIVE_RESTORE_ITEM_VERSION" => {
                let version_id = graph_segment(args, "version_id")?;
                HttpRequest::new(
                    "POST",
                    self.url(&format!(
                        "{}/versions/{version_id}/restoreVersion",
                        self.item_path(args)?
                    )),
                )
            }
            // AP: src/lib/actions/onedrive-checkout-item.ts
            "ONEDRIVE_CHECKOUT_ITEM" => {
                HttpRequest::new("POST", self.url(&format!("{}/checkout", self.item_path(args)?)))
            }
            // AP: src/lib/actions/onedrive-checkin-item.ts
            "ONEDRIVE_CHECKIN_ITEM" => {
                let mut body = serde_json::Map::new();
                if let Some(comment) = opt_str(args, "comment") {
                    body.insert("comment".into(), json!(comment));
                }
                HttpRequest::new("POST", self.url(&format!("{}/checkin", self.item_path(args)?)))
                    .json(&Value::Object(body))
            }
            // AP: src/lib/actions/onedrive-discard-checkout.ts
            "ONEDRIVE_DISCARD_CHECKOUT" => HttpRequest::new(
                "POST",
                self.url(&format!("{}/discardCheckout", self.item_path(args)?)),
            ),
            // AP: src/lib/actions/onedrive-create-bundle.ts
            "ONEDRIVE_CREATE_BUNDLE" => {
                let ids: Vec<String> = str_vec(args, "item_ids")
                    .iter()
                    .map(|id| id.trim().to_string())
                    .filter(|id| !id.is_empty())
                    .collect();
                if ids.len() < 2 {
                    bail!("provide at least two item IDs");
                }
                HttpRequest::new("POST", self.url("/me/drive/bundles")).json(&json!({
                    "name": req_str(args, "name")?,
                    "@microsoft.graph.conflictBehavior": "rename",
                    "bundle": {},
                    "children": ids.iter().map(|id| json!({"id": id})).collect::<Vec<_>>(),
                }))
            }
            // AP: src/lib/actions/onedrive-add-item-to-bundle.ts
            "ONEDRIVE_ADD_ITEM_TO_BUNDLE" => {
                let bundle_id = graph_segment(args, "bundle_id")?;
                let item_id = graph_segment(args, "item_id")?;
                HttpRequest::new(
                    "POST",
                    self.url(&format!("/me/drive/bundles/{bundle_id}/children")),
                )
                .json(&json!({"id": item_id}))
            }
            // AP: src/lib/actions/onedrive-remove-item-from-bundle.ts
            "ONEDRIVE_REMOVE_ITEM_FROM_BUNDLE" => {
                let bundle_id = graph_segment(args, "bundle_id")?;
                let item_id = graph_segment(args, "item_id")?;
                HttpRequest::new(
                    "DELETE",
                    self.url(&format!(
                        "/me/drive/bundles/{bundle_id}/children/{item_id}"
                    )),
                )
            }
            other => bail!("unknown OneDrive tool `{other}`"),
        })
    }

    fn parse_response(&self, tool: &str, response: &HttpResponse) -> Result<Value> {
        let v = response.json()?;
        let ok = |extra: Value| {
            let mut o = json!({"success": true});
            o.as_object_mut()
                .expect("object")
                .extend(extra.as_object().cloned().unwrap_or_default());
            o
        };
        Ok(match tool {
            "ONEDRIVE_GET_ITEM" | "ONEDRIVE_GET_SPECIAL_FOLDER" | "ONEDRIVE_UPDATE_ITEM"
            | "ONEDRIVE_MOVE_ITEM" => item(&v),
            "ONEDRIVE_RESOLVE_SHARING_LINK" => {
                let mut o = item(&v);
                o["share_id"] = v["shareId"].clone();
                o
            }
            "ONEDRIVE_GET_DRIVE" => drive(&v),
            "ONEDRIVE_LIST_DRIVES" => items_page(&v, drive),
            "ONEDRIVE_LIST_FOLDER_CHILDREN" | "ONEDRIVE_SEARCH_ITEMS" => items_page(&v, item),
            "ONEDRIVE_LIST_DRIVE_CHANGES" => {
                let mut page = items_page(&v, item);
                page["delta_token"] = v["@odata.deltaLink"].clone();
                page
            }
            "ONEDRIVE_LIST_ITEM_PERMISSIONS" => items_page(&v, permission),
            "ONEDRIVE_LIST_ITEM_THUMBNAILS" => items_page(&v, thumbnail),
            "ONEDRIVE_LIST_ITEM_VERSIONS" => items_page(&v, version),
            "ONEDRIVE_LIST_BUNDLES" => items_page(&v, bundle),
            "ONEDRIVE_CREATE_FOLDER" => {
                let mut o = item(&v);
                o["created"] = json!(true);
                o
            }
            "ONEDRIVE_CREATE_BUNDLE" => bundle(&v),
            "ONEDRIVE_CREATE_SHARING_LINK" | "ONEDRIVE_UPDATE_ITEM_PERMISSION" => permission(&v),
            "ONEDRIVE_INVITE_TO_ITEM" => {
                let page = items_page(&v, permission);
                ok(json!({
                    "items": page["items"].clone(),
                    "count": page["count"].clone(),
                    "next_page_token": page["next_page_token"].clone(),
                }))
            }
            "ONEDRIVE_DELETE_ITEM"
            | "ONEDRIVE_DELETE_ITEM_PERMISSION"
            | "ONEDRIVE_RESTORE_ITEM_VERSION"
            | "ONEDRIVE_CHECKOUT_ITEM"
            | "ONEDRIVE_CHECKIN_ITEM"
            | "ONEDRIVE_DISCARD_CHECKOUT"
            | "ONEDRIVE_ADD_ITEM_TO_BUNDLE"
            | "ONEDRIVE_REMOVE_ITEM_FROM_BUNDLE" => ok(json!({})),
            other => bail!("unknown OneDrive tool `{other}`"),
        })
    }
}