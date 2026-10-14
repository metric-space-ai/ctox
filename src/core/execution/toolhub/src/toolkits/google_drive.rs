//! Google Drive toolkit (Drive API v3). Action set modelled on Activepieces
//! `piece-google-drive` (MIT). Binary file upload/download flows are not ported.
use anyhow::{bail, Result};
use serde_json::{json, Map, Value};

use super::common::*;
use super::{path_segment, ToolkitAdapter};
use crate::http::{HttpRequest, HttpResponse};
use crate::model::*;

pub const SLUG: &str = "googledrive";
const DRIVE: &str = "https://www.googleapis.com/auth/drive";
const FOLDER_MIME: &str = "application/vnd.google-apps.folder";
const ROLES: [&str; 5] = ["organizer", "fileOrganizer", "writer", "commenter", "reader"];
const BOUNDARY: &str = "ctox-toolhub-drive";
const FIELDS_SEARCH: &str = "nextPageToken, files(id, name, mimeType, createdTime, modifiedTime)";
const FIELDS_LIST: &str = "nextPageToken,files(id,kind,mimeType,name,trashed,parents)";
const FIELDS_PERMISSIONS: &str = "permissions(id,type,role,emailAddress,domain)";

fn b(desc: &str, default: bool) -> Value {
    json!({"type": "boolean", "description": desc, "default": default})
}
fn enum_p(desc: &str, values: &[&str], default: &str) -> Value {
    json!({"type": "string", "description": desc, "enum": values, "default": default})
}
fn enum_opt(desc: &str, values: &[&str]) -> Value {
    json!({"type": "string", "description": desc, "enum": values})
}
fn flag(args: &Value, key: &str) -> bool {
    args.get(key).and_then(Value::as_bool).unwrap_or(false)
}
/// Whitelist check for dropdown values that end up in requests.
fn allow<'a>(key: &str, v: &'a str, allowed: &[&str]) -> Result<&'a str> {
    if allowed.contains(&v) {
        Ok(v)
    } else {
        bail!("invalid argument `{key}`")
    }
}

pub struct GoogleDrive {
    spec: ToolkitSpec,
    api_base: String,
}

impl Default for GoogleDrive {
    fn default() -> Self {
        Self::with_api_base("https://www.googleapis.com")
    }
}

impl GoogleDrive {
    pub fn with_api_base(api_base: &str) -> Self {
        let t = |slug, title, desc, props: Props<'_>, req: &[&str], scopes: &[&str], m| {
            tool(SLUG, slug, title, desc, props, req, scopes, m)
        };
        let spec = ToolkitSpec {
            slug: SLUG.into(),
            name: "Google Drive".into(),
            description: LocalizedText::new(
                "Dateien und Ordner in Google Drive suchen, lesen, kommentieren und verwalten.",
                "Search, read, comment on and manage files and folders in Google Drive.",
            ),
            auth_schemes: vec![
                AuthScheme::OAuth2(OAuth2Scheme {
                    authorize_url: "https://accounts.google.com/o/oauth2/auth".into(),
                    token_url: "https://oauth2.googleapis.com/token".into(),
                    default_scopes: vec![DRIVE.into(), "email".into()],
                    pkce: false,
                    scope_separator: " ".into(),
                    refreshable: true,
                    extra_authorize_params: vec![
                        ("access_type".into(), "offline".into()),
                        ("prompt".into(), "consent".into()),
                    ],
                }),
                AuthScheme::Bearer,
            ],
            tools: vec![
                t(
                    "GOOGLEDRIVE_SEARCH_FILES",
                    ("Dateien suchen", "Search files"),
                    (
                        "Durchsucht Drive nach Name, Volltext oder MIME-Typ (Drive-Query-Syntax).",
                        "Searches Drive by name, full text or MIME type (Drive query syntax).",
                    ),
                    &[
                        ("query_term", enum_p("Feld / field: name|fullText|mimeType", &["name", "fullText", "mimeType"], "name")),
                        ("operator", enum_p("Operator / operator", &["contains", "="], "contains")),
                        ("value", s("Suchwert / value to search for")),
                        ("type", enum_p("Nur Dateien oder Ordner / files or folders only", &["all", "file", "folder"], "all")),
                        ("parent_folder_id", s("Optional auf diesen Ordner einschränken / restrict to this folder")),
                        ("include_team_drives", b("Shared Drives einbeziehen / include shared drives", false)),
                        ("page_token", s("Next page token")),
                    ],
                    &["value"],
                    &[DRIVE],
                    false,
                ),
                t(
                    "GOOGLEDRIVE_GET_FILE",
                    ("Datei oder Ordner abrufen", "Get file or folder"),
                    (
                        "Liest Metadaten einer Datei oder eines Ordners anhand der ID.",
                        "Reads metadata of a file or folder by its ID.",
                    ),
                    &[("file_id", s("Datei-/Ordner-ID / file or folder ID"))],
                    &["file_id"],
                    &[DRIVE],
                    false,
                ),
                t(
                    "GOOGLEDRIVE_LIST_FILES",
                    ("Ordnerinhalt auflisten", "List folder contents"),
                    (
                        "Listet die direkten Kinder eines Ordners (eine Ebene).",
                        "Lists the direct children of a folder (one level).",
                    ),
                    &[
                        ("folder_id", s("Ordner-ID / folder ID")),
                        ("include_trashed", b("Papierkorbelemente einbeziehen / include trashed", false)),
                        ("include_team_drives", b("Shared Drives einbeziehen / include shared drives", false)),
                        ("page_token", s("Next page token")),
                    ],
                    &["folder_id"],
                    &[DRIVE],
                    false,
                ),
                t(
                    "GOOGLEDRIVE_GET_ABOUT",
                    ("Konto- & Speicherinfo", "Get account & storage info"),
                    (
                        "Konto, Speicherkontingent und unterstützte Formate.",
                        "Account identity, storage quota and supported formats.",
                    ),
                    &[],
                    &[],
                    &[DRIVE],
                    false,
                ),
                t(
                    "GOOGLEDRIVE_LIST_PERMISSIONS",
                    ("Berechtigungen auflisten", "List permissions"),
                    (
                        "Listet alle Berechtigungen einer Datei oder eines Ordners.",
                        "Lists every permission on a file or folder.",
                    ),
                    &[("file_id", s("Datei-/Ordner-ID / file or folder ID"))],
                    &["file_id"],
                    &[DRIVE],
                    false,
                ),
                t(
                    "GOOGLEDRIVE_LIST_COMMENTS",
                    ("Kommentare auflisten", "List comments"),
                    ("Listet die Kommentare zu einer Datei.", "Lists the comments on a file."),
                    &[("file_id", s("Datei-ID / file ID"))],
                    &["file_id"],
                    &[DRIVE],
                    false,
                ),
                t(
                    "GOOGLEDRIVE_LIST_REPLIES",
                    ("Antworten auflisten", "List replies"),
                    ("Listet die Antworten auf einen Kommentar.", "Lists the replies under a comment."),
                    &[
                        ("file_id", s("Datei-ID / file ID")),
                        ("comment_id", s("Kommentar-ID / comment ID")),
                    ],
                    &["file_id", "comment_id"],
                    &[DRIVE],
                    false,
                ),
                t(
                    "GOOGLEDRIVE_GET_REPLY",
                    ("Antwort lesen", "Get reply"),
                    ("Liest eine einzelne Antwort auf einen Kommentar.", "Reads a single reply on a comment."),
                    &[
                        ("file_id", s("Datei-ID / file ID")),
                        ("comment_id", s("Kommentar-ID / comment ID")),
                        ("reply_id", s("Antwort-ID / reply ID")),
                    ],
                    &["file_id", "comment_id", "reply_id"],
                    &[DRIVE],
                    false,
                ),
                t(
                    "GOOGLEDRIVE_LIST_SHARED_DRIVES",
                    ("Shared Drives auflisten", "List shared drives"),
                    ("Listet die für das Konto sichtbaren Shared Drives.", "Lists the shared drives the account can access."),
                    &[
                        ("query", s("Optionale Drive-Query / optional search query")),
                        ("page_size", int("1-100", 1, 100, 100)),
                    ],
                    &[],
                    &[DRIVE],
                    false,
                ),
                t(
                    "GOOGLEDRIVE_GET_SHARED_DRIVE",
                    ("Shared Drive abrufen", "Get shared drive"),
                    ("Metadaten eines Shared Drive anhand der ID.", "Metadata for one shared drive by ID."),
                    &[("drive_id", s("Shared-Drive-ID / shared drive ID"))],
                    &["drive_id"],
                    &[DRIVE],
                    false,
                ),
                t(
                    "GOOGLEDRIVE_CREATE_FOLDER",
                    ("Ordner anlegen", "Create folder"),
                    (
                        "Legt einen leeren Ordner an (optional in einem Elternordner).",
                        "Creates an empty folder (optionally inside a parent).",
                    ),
                    &[
                        ("folder_name", s("Name des neuen Ordners / new folder name")),
                        ("parent_folder_id", s("Leer = My Drive-Root / empty = My Drive root")),
                    ],
                    &["folder_name"],
                    &[DRIVE],
                    true,
                ),
                t(
                    "GOOGLEDRIVE_CREATE_FILE_FROM_TEXT",
                    ("Textdatei anlegen", "Create file from text"),
                    (
                        "Erstellt eine Text-, CSV- oder XML-Datei aus Text.",
                        "Creates a text, CSV or XML file from inline text.",
                    ),
                    &[
                        ("file_name", s("Dateiname inkl. Endung / file name incl. extension")),
                        ("text", s("Inhalt / content")),
                        ("content_type", enum_p("Dateityp / file type", &["plain/text", "text/csv", "text/xml"], "plain/text")),
                        ("parent_folder_id", s("Leer = My Drive-Root / empty = My Drive root")),
                    ],
                    &["file_name", "text", "content_type"],
                    &[DRIVE],
                    true,
                ),
                t(
                    "GOOGLEDRIVE_COPY_FILE",
                    ("Datei kopieren", "Copy file"),
                    (
                        "Dupliziert eine Datei in einen Zielordner (optional als Doc/Sheet).",
                        "Duplicates a file into a target folder (optionally as Doc/Sheet).",
                    ),
                    &[
                        ("file_id", s("Quelldatei-ID / source file ID")),
                        ("name", s("Name der Kopie / copy name")),
                        ("folder_id", s("Zielordner-ID / destination folder ID")),
                        ("convert_to", enum_opt("Konvertieren nach / convert to", &["application/vnd.google-apps.spreadsheet", "application/vnd.google-apps.document"])),
                    ],
                    &["file_id", "name", "folder_id"],
                    &[DRIVE],
                    true,
                ),
                t(
                    "GOOGLEDRIVE_MOVE_FILE",
                    ("Datei verschieben", "Move file"),
                    (
                        "Verschiebt eine Datei in einen Zielordner; `current_parents` (kommagetrennt, via GOOGLEDRIVE_GET_FILE) wird abgehängt.",
                        "Moves a file into a target folder; `current_parents` (comma-separated, from GOOGLEDRIVE_GET_FILE) are detached.",
                    ),
                    &[
                        ("file_id", s("Datei-ID / file ID")),
                        ("folder_id", s("Zielordner-ID / destination folder ID")),
                        ("current_parents", s("Aktuelle Eltern-IDs / current parent IDs")),
                    ],
                    &["file_id", "folder_id"],
                    &[DRIVE],
                    true,
                ),
                t(
                    "GOOGLEDRIVE_TRASH_FILE",
                    ("Datei in den Papierkorb", "Trash file"),
                    ("Verschiebt eine Datei in den Papierkorb (wiederherstellbar).", "Moves a file to the trash (reversible)."),
                    &[("file_id", s("Datei-ID / file ID"))],
                    &["file_id"],
                    &[DRIVE],
                    true,
                ),
                t(
                    "GOOGLEDRIVE_UNTRASH_FILE",
                    ("Datei wiederherstellen", "Restore file"),
                    ("Stellt eine Datei aus dem Papierkorb wieder her.", "Restores a file from the trash."),
                    &[("file_id", s("Datei-ID / file ID"))],
                    &["file_id"],
                    &[DRIVE],
                    true,
                ),
                t(
                    "GOOGLEDRIVE_DELETE_FILE",
                    ("Datei endgültig löschen", "Delete file permanently"),
                    ("Löscht eine Datei dauerhaft, ohne den Papierkorb.", "Permanently deletes a file, bypassing the trash."),
                    &[("file_id", s("Datei-ID / file ID"))],
                    &["file_id"],
                    &[DRIVE],
                    true,
                ),
                t(
                    "GOOGLEDRIVE_UPDATE_FILE_METADATA",
                    ("Metadaten aktualisieren", "Update file metadata"),
                    ("Benennt um, setzt Beschreibung oder Stern-Markierung.", "Renames, or updates description / starred flag."),
                    &[
                        ("file_id", s("Datei-/Ordner-ID / file or folder ID")),
                        ("name", s("Neuer Name / new name")),
                        ("description", s("Neue Beschreibung / new description")),
                        ("starred", json!({"type": "boolean", "description": "Mit Stern markieren / mark as starred"})),
                    ],
                    &["file_id"],
                    &[DRIVE],
                    true,
                ),
                t(
                    "GOOGLEDRIVE_SHARE_FILE",
                    ("Datei freigeben", "Share file"),
                    (
                        "Gibt einer Person per E-Mail eine Rolle auf einer Datei/einem Ordner.",
                        "Grants a person a role on a file or folder by email.",
                    ),
                    &[
                        ("file_id", s("Datei-/Ordner-ID / file or folder ID")),
                        ("user_email", s("E-Mail des Nutzers / user email")),
                        ("role", enum_p("Rolle / role", &ROLES, "reader")),
                        ("send_invitation_email", json!({"type": "boolean", "description": "Einladung per E-Mail senden / send invitation email"})),
                    ],
                    &["file_id", "user_email", "role", "send_invitation_email"],
                    &[DRIVE],
                    true,
                ),
                t(
                    "GOOGLEDRIVE_SET_PUBLIC_ACCESS",
                    ("Öffentlichen Zugriff setzen", "Set public access"),
                    ("Macht eine Datei/einen Ordner für jeden mit dem Link zugänglich.", "Makes a file or folder accessible to anyone with the link."),
                    &[
                        ("file_id", s("Datei-/Ordner-ID / file or folder ID")),
                        ("role", enum_p("Rolle / role", &["reader", "commenter", "writer"], "reader")),
                    ],
                    &["file_id", "role"],
                    &[DRIVE],
                    true,
                ),
                t(
                    "GOOGLEDRIVE_UPDATE_PERMISSION",
                    ("Rolle ändern", "Update permission role"),
                    (
                        "Ändert die Rolle einer bestehenden Berechtigung (ID via GOOGLEDRIVE_LIST_PERMISSIONS).",
                        "Changes the role of an existing permission (ID via GOOGLEDRIVE_LIST_PERMISSIONS).",
                    ),
                    &[
                        ("file_id", s("Datei-/Ordner-ID / file or folder ID")),
                        ("permission_id", s("Berechtigungs-ID / permission ID")),
                        ("new_role", enum_p("Neue Rolle / new role", &ROLES, "reader")),
                    ],
                    &["file_id", "permission_id", "new_role"],
                    &[DRIVE],
                    true,
                ),
                t(
                    "GOOGLEDRIVE_REMOVE_PERMISSION",
                    ("Zugriff entziehen", "Remove permission"),
                    ("Entzieht eine Berechtigung (ID via GOOGLEDRIVE_LIST_PERMISSIONS).", "Revokes a permission (ID via GOOGLEDRIVE_LIST_PERMISSIONS)."),
                    &[
                        ("file_id", s("Datei-/Ordner-ID / file or folder ID")),
                        ("permission_id", s("Berechtigungs-ID / permission ID")),
                    ],
                    &["file_id", "permission_id"],
                    &[DRIVE],
                    true,
                ),
                t(
                    "GOOGLEDRIVE_CREATE_COMMENT",
                    ("Kommentar anlegen", "Create comment"),
                    ("Fügt einer Datei einen Kommentar hinzu.", "Adds a comment to a file."),
                    &[
                        ("file_id", s("Datei-ID / file ID")),
                        ("content", s("Kommentartext / comment text")),
                    ],
                    &["file_id", "content"],
                    &[DRIVE],
                    true,
                ),
                t(
                    "GOOGLEDRIVE_CREATE_REPLY",
                    ("Auf Kommentar antworten", "Reply to comment"),
                    ("Antwortet auf einen Kommentar; optional auflösen/öffnen.", "Replies to a comment; optionally resolve/reopen it."),
                    &[
                        ("file_id", s("Datei-ID / file ID")),
                        ("comment_id", s("Kommentar-ID / comment ID")),
                        ("content", s("Antworttext / reply text")),
                        ("action", enum_opt("Kommentar-Status / comment state", &["resolve", "reopen"])),
                    ],
                    &["file_id", "comment_id", "content"],
                    &[DRIVE],
                    true,
                ),
                t(
                    "GOOGLEDRIVE_UPDATE_REPLY",
                    ("Antwort bearbeiten", "Update reply"),
                    ("Ändert den Text einer eigenen Antwort.", "Edits the text of one of your replies."),
                    &[
                        ("file_id", s("Datei-ID / file ID")),
                        ("comment_id", s("Kommentar-ID / comment ID")),
                        ("reply_id", s("Antwort-ID / reply ID")),
                        ("content", s("Neuer Text / new text")),
                    ],
                    &["file_id", "comment_id", "reply_id", "content"],
                    &[DRIVE],
                    true,
                ),
                t(
                    "GOOGLEDRIVE_DELETE_REPLY",
                    ("Antwort löschen", "Delete reply"),
                    ("Löscht eine Antwort auf einen Kommentar.", "Deletes a reply on a comment."),
                    &[
                        ("file_id", s("Datei-ID / file ID")),
                        ("comment_id", s("Kommentar-ID / comment ID")),
                        ("reply_id", s("Antwort-ID / reply ID")),
                    ],
                    &["file_id", "comment_id", "reply_id"],
                    &[DRIVE],
                    true,
                ),
                t(
                    "GOOGLEDRIVE_CREATE_SHARED_DRIVE",
                    ("Shared Drive anlegen", "Create shared drive"),
                    ("Legt einen neuen Shared Drive (Team Drive) an.", "Creates a new shared drive (team drive)."),
                    &[("name", s("Name / name"))],
                    &["name"],
                    &[DRIVE],
                    true,
                ),
                t(
                    "GOOGLEDRIVE_UPDATE_SHARED_DRIVE",
                    ("Shared Drive umbenennen", "Update shared drive"),
                    ("Benennt einen Shared Drive um.", "Renames a shared drive."),
                    &[
                        ("drive_id", s("Shared-Drive-ID / shared drive ID")),
                        ("name", s("Neuer Name / new name")),
                    ],
                    &["drive_id"],
                    &[DRIVE],
                    true,
                ),
                t(
                    "GOOGLEDRIVE_DELETE_SHARED_DRIVE",
                    ("Shared Drive löschen", "Delete shared drive"),
                    ("Löscht einen Shared Drive endgültig (muss leer sein).", "Permanently deletes a shared drive (must be empty)."),
                    &[("drive_id", s("Shared-Drive-ID / shared drive ID"))],
                    &["drive_id"],
                    &[DRIVE],
                    true,
                ),
                t(
                    "GOOGLEDRIVE_EMPTY_TRASH",
                    ("Papierkorb leeren", "Empty trash"),
                    (
                        "Löscht ALLE Papierkorbelemente dauerhaft (unumkehrbar); optional auf einen Shared Drive begrenzt.",
                        "Permanently deletes ALL trashed files (irreversible); optionally restricted to one shared drive.",
                    ),
                    &[("drive_id", s("Optional: Shared Drive-ID / optional shared drive ID"))],
                    &[],
                    &[DRIVE],
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
        format!("{}/drive/v3{path}", self.api_base)
    }

    fn files(&self, path: &str) -> String {
        self.url(&format!("/files{path}"))
    }

    /// Multipart/related body (metadata + media) for `uploadType=multipart`.
    fn multipart(meta: &Value, media: &str, content_type: &str) -> Result<Vec<u8>> {
        let text = format!(
            "--{BOUNDARY}\r\nContent-Type: application/json\r\n\r\n{meta}\r\n\
             --{BOUNDARY}\r\nContent-Type: {content_type}\r\n\r\n{media}\r\n--{BOUNDARY}--\r\n"
        );
        if media.contains(BOUNDARY) {
            bail!("argument `text` contains the multipart boundary");
        }
        Ok(text.into_bytes())
    }
}

impl ToolkitAdapter for GoogleDrive {
    fn spec(&self) -> &ToolkitSpec {
        &self.spec
    }

    fn build_request(&self, tool: &str, args: &Value) -> Result<HttpRequest> {
        Ok(match tool {
            "GOOGLEDRIVE_SEARCH_FILES" => {
                let mut q = format!(
                    "{} {} '{}'",
                    opt_str(args, "query_term").unwrap_or("name"),
                    opt_str(args, "operator").unwrap_or("contains"),
                    req_str(args, "value")?
                );
                if let Some(p) = opt_str(args, "parent_folder_id") {
                    q.push_str(&format!(" and '{p}' in parents"));
                }
                match opt_str(args, "type").unwrap_or("all") {
                    "file" => q.push_str(&format!(" and mimeType!='{FOLDER_MIME}'")),
                    "folder" => q.push_str(&format!(" and mimeType='{FOLDER_MIME}'")),
                    _ => {}
                }
                let team = flag(args, "include_team_drives");
                let qstr = query(&[
                    ("q", Some(q)),
                    ("fields", Some(FIELDS_SEARCH.into())),
                    ("supportsAllDrives", Some("true".into())),
                    ("includeItemsFromAllDrives", Some(team.to_string())),
                    ("corpora", Some(if team { "allDrives" } else { "user" }.into())),
                    ("pageSize", Some("1000".into())),
                    ("pageToken", opt_str(args, "page_token").map(String::from)),
                ]);
                HttpRequest::new("GET", self.files(&qstr))
            }
            "GOOGLEDRIVE_GET_FILE" => HttpRequest::new(
                "GET",
                self.files(&format!(
                    "/{}?supportsAllDrives=true",
                    path_segment(args, "file_id")?
                )),
            ),
            "GOOGLEDRIVE_LIST_FILES" => {
                let mut q = format!("'{}' in parents", req_str(args, "folder_id")?);
                if !flag(args, "include_trashed") {
                    q.push_str(" and trashed=false");
                }
                let team = flag(args, "include_team_drives");
                let qstr = query(&[
                    ("q", Some(q)),
                    ("fields", Some(FIELDS_LIST.into())),
                    ("supportsAllDrives", Some("true".into())),
                    ("includeItemsFromAllDrives", Some(team.to_string())),
                    ("corpora", Some(if team { "allDrives" } else { "user" }.into())),
                    ("pageSize", Some("1000".into())),
                    ("pageToken", opt_str(args, "page_token").map(String::from)),
                ]);
                HttpRequest::new("GET", self.files(&qstr))
            }
            "GOOGLEDRIVE_GET_ABOUT" => HttpRequest::new(
                "GET",
                self.url(&format!(
                    "/about{}",
                    query(&[(
                        "fields",
                        Some("user,storageQuota,maxImportSizes,exportFormats".into())
                    )])
                )),
            ),
            "GOOGLEDRIVE_LIST_PERMISSIONS" => HttpRequest::new(
                "GET",
                self.files(&format!(
                    "/{}/permissions{}",
                    path_segment(args, "file_id")?,
                    query(&[
                        ("supportsAllDrives", Some("true".into())),
                        ("fields", Some(FIELDS_PERMISSIONS.into())),
                    ])
                )),
            ),
            "GOOGLEDRIVE_LIST_COMMENTS" => HttpRequest::new(
                "GET",
                self.files(&format!(
                    "/{}/comments{}",
                    path_segment(args, "file_id")?,
                    query(&[
                        ("fields", Some("*".into())),
                        ("includeDeleted", Some("false".into())),
                    ])
                )),
            ),
            "GOOGLEDRIVE_LIST_REPLIES" => HttpRequest::new(
                "GET",
                self.files(&format!(
                    "/{}/comments/{}/replies{}",
                    path_segment(args, "file_id")?,
                    path_segment(args, "comment_id")?,
                    query(&[("fields", Some("*".into()))])
                )),
            ),
            "GOOGLEDRIVE_GET_REPLY" => HttpRequest::new(
                "GET",
                self.files(&format!(
                    "/{}/comments/{}/replies/{}?fields=*",
                    path_segment(args, "file_id")?,
                    path_segment(args, "comment_id")?,
                    path_segment(args, "reply_id")?
                )),
            ),
            "GOOGLEDRIVE_LIST_SHARED_DRIVES" => {
                let qstr = query(&[
                    ("q", opt_str(args, "query").map(String::from)),
                    (
                        "pageSize",
                        args.get("page_size")
                            .and_then(Value::as_u64)
                            .map(|p| p.to_string()),
                    ),
                ]);
                HttpRequest::new("GET", self.url(&format!("/drives{qstr}")))
            }
            "GOOGLEDRIVE_GET_SHARED_DRIVE" => HttpRequest::new(
                "GET",
                self.url(&format!("/drives/{}", path_segment(args, "drive_id")?)),
            ),
            "GOOGLEDRIVE_CREATE_FOLDER" => {
                let mut body = json!({"name": req_str(args, "folder_name")?, "mimeType": FOLDER_MIME});
                if let Some(p) = opt_str(args, "parent_folder_id") {
                    body["parents"] = json!([p]);
                }
                HttpRequest::new("POST", self.files("?supportsAllDrives=true")).json(&body)
            }
            "GOOGLEDRIVE_CREATE_FILE_FROM_TEXT" => {
                let name = req_str(args, "file_name")?;
                let text = req_str(args, "text")?;
                let ctype = allow(
                    "content_type",
                    opt_str(args, "content_type").unwrap_or("plain/text"),
                    &["plain/text", "text/csv", "text/xml"],
                )?;
                let mut meta = json!({"name": name, "mimeType": ctype});
                if let Some(p) = opt_str(args, "parent_folder_id") {
                    meta["parents"] = json!([p]);
                }
                let mut req = HttpRequest::new(
                    "POST",
                    format!(
                        "{}/upload/drive/v3/files{}",
                        self.api_base,
                        query(&[
                            ("uploadType", Some("multipart".into())),
                            ("supportsAllDrives", Some("true".into())),
                        ])
                    ),
                )
                .json(&json!({}));
                req.body = Some(Self::multipart(&meta, text, ctype)?);
                req.headers.insert(
                    "content-type".into(),
                    format!("multipart/related; boundary={BOUNDARY}"),
                );
                req
            }
            "GOOGLEDRIVE_COPY_FILE" => {
                let mut body = json!({"name": req_str(args, "name")?, "parents": [req_str(args, "folder_id")?]});
                if let Some(m) = opt_str(args, "convert_to") {
                    body["mimeType"] = json!(allow("convert_to", m, &["application/vnd.google-apps.spreadsheet", "application/vnd.google-apps.document"])?);
                }
                HttpRequest::new(
                    "POST",
                    self.files(&format!(
                        "/{}/copy?supportsAllDrives=true",
                        path_segment(args, "file_id")?
                    )),
                )
                .json(&body)
            }
            "GOOGLEDRIVE_MOVE_FILE" => {
                let qstr = query(&[
                    ("addParents", Some(req_str(args, "folder_id")?.to_string())),
                    ("removeParents", opt_str(args, "current_parents").map(String::from)),
                    ("fields", Some("*".into())),
                    ("supportsAllDrives", Some("true".into())),
                ]);
                HttpRequest::new(
                    "PATCH",
                    self.files(&format!("/{}{qstr}", path_segment(args, "file_id")?)),
                )
            }
            "GOOGLEDRIVE_TRASH_FILE" => HttpRequest::new(
                "PATCH",
                self.files(&format!(
                    "/{}?supportsAllDrives=true",
                    path_segment(args, "file_id")?
                )),
            )
            .json(&json!({"trashed": true})),
            "GOOGLEDRIVE_UNTRASH_FILE" => HttpRequest::new(
                "PATCH",
                self.files(&format!(
                    "/{}?supportsAllDrives=true",
                    path_segment(args, "file_id")?
                )),
            )
            .json(&json!({"trashed": false})),
            "GOOGLEDRIVE_DELETE_FILE" => HttpRequest::new(
                "DELETE",
                self.files(&format!(
                    "/{}?supportsAllDrives=true",
                    path_segment(args, "file_id")?
                )),
            ),
            "GOOGLEDRIVE_UPDATE_FILE_METADATA" => {
                let mut body = Map::new();
                if let Some(n) = opt_str(args, "name") {
                    body.insert("name".into(), json!(n));
                }
                if let Some(d) = args.get("description").and_then(Value::as_str) {
                    body.insert("description".into(), json!(d));
                }
                if let Some(s) = args.get("starred").and_then(Value::as_bool) {
                    body.insert("starred".into(), json!(s));
                }
                if body.is_empty() {
                    bail!("missing argument `name`, `description` or `starred`");
                }
                HttpRequest::new(
                    "PATCH",
                    self.files(&format!(
                        "/{}?supportsAllDrives=true",
                        path_segment(args, "file_id")?
                    )),
                )
                .json(&Value::Object(body))
            }
            "GOOGLEDRIVE_SHARE_FILE" => {
                let body = json!({
                    "type": "user",
                    "role": allow("role", req_str(args, "role")?, &ROLES)?,
                    "emailAddress": req_str(args, "user_email")?,
                });
                let qstr = query(&[
                    ("supportsAllDrives", Some("true".into())),
                    (
                        "sendNotificationEmail",
                        Some(flag(args, "send_invitation_email").to_string()),
                    ),
                ]);
                HttpRequest::new(
                    "POST",
                    self.files(&format!(
                        "/{}/permissions{qstr}",
                        path_segment(args, "file_id")?
                    )),
                )
                .json(&body)
            }
            "GOOGLEDRIVE_SET_PUBLIC_ACCESS" => {
                let body = json!({
                    "type": "anyone",
                    "role": allow(
                        "role",
                        opt_str(args, "role").unwrap_or("reader"),
                        &["reader", "commenter", "writer"]
                    )?,
                });
                HttpRequest::new(
                    "POST",
                    self.files(&format!(
                        "/{}/permissions?supportsAllDrives=true",
                        path_segment(args, "file_id")?
                    )),
                )
                .json(&body)
            }
            "GOOGLEDRIVE_UPDATE_PERMISSION" => {
                let body = json!({"role": allow("new_role", req_str(args, "new_role")?, &ROLES)?});
                HttpRequest::new(
                    "PATCH",
                    self.files(&format!(
                        "/{}/permissions/{}?supportsAllDrives=true",
                        path_segment(args, "file_id")?,
                        path_segment(args, "permission_id")?
                    )),
                )
                .json(&body)
            }
            "GOOGLEDRIVE_REMOVE_PERMISSION" => HttpRequest::new(
                "DELETE",
                self.files(&format!(
                    "/{}/permissions/{}?supportsAllDrives=true",
                    path_segment(args, "file_id")?,
                    path_segment(args, "permission_id")?
                )),
            ),
            "GOOGLEDRIVE_CREATE_COMMENT" => HttpRequest::new(
                "POST",
                self.files(&format!(
                    "/{}/comments?fields=*",
                    path_segment(args, "file_id")?
                )),
            )
            .json(&json!({"content": req_str(args, "content")?})),
            "GOOGLEDRIVE_CREATE_REPLY" => {
                let mut body = json!({"content": req_str(args, "content")?});
                if let Some(a) = opt_str(args, "action") {
                    body["action"] = json!(allow("action", a, &["resolve", "reopen"])?);
                }
                HttpRequest::new(
                    "POST",
                    self.files(&format!(
                        "/{}/comments/{}/replies?fields=*",
                        path_segment(args, "file_id")?,
                        path_segment(args, "comment_id")?
                    )),
                )
                .json(&body)
            }
            "GOOGLEDRIVE_UPDATE_REPLY" => HttpRequest::new(
                "PATCH",
                self.files(&format!(
                    "/{}/comments/{}/replies/{}?fields=*",
                    path_segment(args, "file_id")?,
                    path_segment(args, "comment_id")?,
                    path_segment(args, "reply_id")?
                )),
            )
            .json(&json!({"content": req_str(args, "content")?})),
            "GOOGLEDRIVE_DELETE_REPLY" => HttpRequest::new(
                "DELETE",
                self.files(&format!(
                    "/{}/comments/{}/replies/{}",
                    path_segment(args, "file_id")?,
                    path_segment(args, "comment_id")?,
                    path_segment(args, "reply_id")?
                )),
            ),
            "GOOGLEDRIVE_CREATE_SHARED_DRIVE" => HttpRequest::new(
                "POST",
                self.url(&format!(
                    "/drives{}",
                    query(&[(
                        "requestId",
                        // drives.create needs an idempotency key (400 otherwise).
                        Some(crate::pkce::random_token(16))
                    )])
                )),
            )
            .json(&json!({"name": req_str(args, "name")?})),
            "GOOGLEDRIVE_UPDATE_SHARED_DRIVE" => {
                let mut body = Map::new();
                if let Some(n) = opt_str(args, "name") {
                    body.insert("name".into(), json!(n));
                }
                HttpRequest::new(
                    "PATCH",
                    self.url(&format!("/drives/{}", path_segment(args, "drive_id")?)),
                )
                .json(&Value::Object(body))
            }
            "GOOGLEDRIVE_DELETE_SHARED_DRIVE" => HttpRequest::new(
                "DELETE",
                self.url(&format!("/drives/{}", path_segment(args, "drive_id")?)),
            ),
            "GOOGLEDRIVE_EMPTY_TRASH" => HttpRequest::new(
                "DELETE",
                self.url(&format!(
                    "/files/trash{}",
                    query(&[("driveId", opt_str(args, "drive_id").map(String::from))])
                )),
            ),
            other => bail!("unknown Google Drive tool `{other}`"),
        })
    }

    fn parse_response(&self, tool: &str, response: &HttpResponse) -> Result<Value> {
        let v = response.json()?;
        Ok(match tool {
            "GOOGLEDRIVE_SEARCH_FILES" => {
                json!({"files": v["files"], "next_page_token": v["nextPageToken"]})
            }
            "GOOGLEDRIVE_LIST_FILES" => json!({"kind": v["kind"], "incomplete_search": v["incompleteSearch"],
                "files": v["files"], "next_page_token": v["nextPageToken"]}),
            "GOOGLEDRIVE_LIST_PERMISSIONS" => count_list(&v, "permissions"),
            "GOOGLEDRIVE_LIST_COMMENTS" => count_list(&v, "comments"),
            "GOOGLEDRIVE_LIST_REPLIES" => count_list(&v, "replies"),
            "GOOGLEDRIVE_LIST_SHARED_DRIVES" => count_list(&v, "drives"),
            "GOOGLEDRIVE_DELETE_FILE" | "GOOGLEDRIVE_DELETE_REPLY"
            | "GOOGLEDRIVE_DELETE_SHARED_DRIVE" => json!({"deleted": true}),
            "GOOGLEDRIVE_REMOVE_PERMISSION" => json!({"removed": true}),
            "GOOGLEDRIVE_EMPTY_TRASH" => json!({"success": true}),
            _ => v,
        })
    }
}

fn count_list(v: &Value, key: &str) -> Value {
    let empty = Vec::new();
    let items = v[key].as_array().unwrap_or(&empty);
    json!({ (key): items, "count": items.len() })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn requests_and_guards() {
        let d = GoogleDrive::with_api_base("https://g.test");
        let r = d
            .build_request(
                "GOOGLEDRIVE_SEARCH_FILES",
                &json!({"query_term": "name", "operator": "contains", "value": "Rep",
                    "type": "folder", "parent_folder_id": "root1"}),
            )
            .unwrap();
        assert!(r.url.contains("q=name+contains+%27Rep%27+and+%27root1%27+in+parents+and+mimeType%3D%27application%2Fvnd.google-apps.folder%27"), "{}", r.url);
        assert!(r.url.contains("corpora=user"));
        assert!(d
            .build_request("GOOGLEDRIVE_GET_FILE", &json!({"file_id": "../x"}))
            .is_err());
        let r = d
            .build_request(
                "GOOGLEDRIVE_CREATE_FILE_FROM_TEXT",
                &json!({"file_name": "a.csv", "text": "x,y", "content_type": "text/csv",
                    "parent_folder_id": "p1"}),
            )
            .unwrap();
        assert!(r.url.contains("/upload/drive/v3/files?uploadType=multipart"));
        let body = String::from_utf8(r.body.unwrap()).unwrap();
        assert!(body.contains("Content-Type: text/csv\r\n\r\nx,y"));
        assert!(body.contains("\"name\":\"a.csv\""));
        assert_eq!(
            r.headers["content-type"],
            format!("multipart/related; boundary={BOUNDARY}")
        );
        assert!(d
            .build_request(
                "GOOGLEDRIVE_CREATE_FILE_FROM_TEXT",
                &json!({"file_name": "a.csv", "text": "--ctox-toolhub-drive", "content_type": "text/csv"})
            )
            .is_err());
        // Metadata update without any field to set is rejected.
        assert!(d
            .build_request("GOOGLEDRIVE_UPDATE_FILE_METADATA", &json!({"file_id": "f1"}))
            .is_err());
        let r = d
            .build_request("GOOGLEDRIVE_EMPTY_TRASH", &json!({}))
            .unwrap();
        assert_eq!(r.url, "https://g.test/drive/v3/files/trash");
        // Role dropdown values are whitelisted.
        assert!(d
            .build_request(
                "GOOGLEDRIVE_SHARE_FILE",
                &json!({"file_id": "f1", "user_email": "a@b", "role": "root", "send_invitation_email": false})
            )
            .is_err());
        let empty = HttpResponse { status: 204, body: Vec::new(), session_id: None };
        assert_eq!(
            d.parse_response("GOOGLEDRIVE_EMPTY_TRASH", &empty).unwrap(),
            json!({"success": true})
        );
    }
}
