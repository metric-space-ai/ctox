//! Microsoft Outlook toolkit (Microsoft Graph mail API v1.0). Action set modelled
//! on Activepieces `microsoft-outlook` (MIT).
//!
//! Deviation from the TypeScript piece forced by the pure request/parse seam:
//! actions that merged categories needed a read-then-patch round trip; here the
//! caller passes the current categories (`current_categories`, from Get Message)
//! and the same merge logic runs locally. File uploads (attachments) are not
//! ported, like all file-transfer tools.
use anyhow::{anyhow, bail, Result};
use base64::Engine as _;
use serde_json::{json, Map, Value};

use super::common::*;
use super::ToolkitAdapter;
use crate::http::{HttpRequest, HttpResponse};
use crate::model::*;

pub const SLUG: &str = "outlook";
const MAIL_RW: &str = "Mail.ReadWrite";
const MAIL_SEND: &str = "Mail.Send";
const USER_READ: &str = "User.Read";

/// Mirrors `outlookAtomicCommon.messageSelect`.
const SELECT_MESSAGE: &str = "id,subject,bodyPreview,from,sender,toRecipients,ccRecipients,bccRecipients,replyTo,receivedDateTime,sentDateTime,createdDateTime,lastModifiedDateTime,hasAttachments,isRead,isDraft,importance,categories,flag,webLink,parentFolderId,conversationId,internetMessageId";
const SELECT_FOLDER: &str =
    "id,displayName,parentFolderId,childFolderCount,unreadItemCount,totalItemCount,isHidden";
const SELECT_ATTACHMENT: &str = "id,name,contentType,size,isInline,lastModifiedDateTime";
const MAIL_TIPS_OPTIONS: &str = "automaticReplies, mailboxFullStatus, customMailTip, externalMemberCount, maxMessageSize, deliveryRestriction, moderationStatus, recipientScope, totalMemberCount";
const TEXT_BODY_PREFER: &str = "outlook.body-content-type=\"text\"";
/// Microsoft Graph allows at most 20 sub-requests per `$batch` call.
const BATCH_LIMIT: usize = 20;

pub struct Outlook {
    spec: ToolkitSpec,
    api_base: String,
}

impl Default for Outlook {
    fn default() -> Self {
        Self::with_api_base("https://graph.microsoft.com")
    }
}

/// Percent-encode one opaque value for use inside a URL path or query
/// (encodeURIComponent semantics; unreserved `A-Za-z0-9-_.~` stay as-is).
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

/// Microsoft Graph IDs are opaque and routinely contain `=` (base64 padding),
/// which `path_segment` rejects. They are validated here by trimming, bounding
/// the length and percent-encoding every structural byte, so they cannot break
/// out of their path segment.
fn graph_id(args: &Value, key: &str) -> Result<String> {
    let v = args
        .get(key)
        .and_then(Value::as_str)
        .ok_or_else(|| anyhow!("missing argument `{key}`"))?;
    let v = v.trim();
    if v.is_empty() || v.len() > 1000 {
        bail!("invalid argument `{key}`");
    }
    Ok(pct_encode(v))
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

/// Graph recipient array (`outlookCommon.toRecipients`).
fn addr_list(addresses: Vec<String>) -> Value {
    Value::Array(
        addresses
            .into_iter()
            .map(|a| json!({"emailAddress": {"address": a.trim()}}))
            .collect(),
    )
}

/// Message payload shared by send/create-draft (`subject`, `body`, recipients).
fn mail_payload(args: &Value) -> Result<Value> {
    let body_format = req_one_of(args, "body_format", &["text", "html"])?;
    Ok(json!({
        "subject": req_str(args, "subject")?,
        "body": {"contentType": body_format, "content": req_str(args, "body")?},
        "toRecipients": addr_list(str_vec(args, "recipients")),
        "ccRecipients": addr_list(str_vec(args, "cc_recipients")),
        "bccRecipients": addr_list(str_vec(args, "bcc_recipients")),
    }))
}

/// Writable message properties for PATCH (single + batch update).
fn update_payload(args: &Value) -> Result<Value> {
    let mut m = Map::new();
    if let Some(v) = opt_one_of(args, "is_read", &["true", "false"])? {
        m.insert("isRead".into(), json!(v == "true"));
    }
    if let Some(v) = opt_one_of(args, "flag_status", &["flagged", "notFlagged", "complete"])? {
        m.insert("flag".into(), json!({"flagStatus": v}));
    }
    if let Some(v) = opt_one_of(args, "importance", &["low", "normal", "high"])? {
        m.insert("importance".into(), json!(v));
    }
    if let Some(v) = opt_str(args, "subject") {
        m.insert("subject".into(), json!(v));
    }
    if let Some(v) = opt_str(args, "body") {
        m.insert(
            "body".into(),
            json!({"contentType": req_one_of(args, "body_format", &["text", "html"])?, "content": v}),
        );
    }
    if m.is_empty() {
        bail!("no fields to update were supplied");
    }
    Ok(Value::Object(m))
}

/// Merge categories like the TypeScript piece: union with existing, then remove.
fn merged_categories(args: &Value, add: bool) -> Result<Vec<String>> {
    let key = if add { "categories" } else { "categories_to_remove" };
    let changed = str_vec(args, key);
    if changed.is_empty() {
        bail!("missing argument `{key}`");
    }
    let existing = str_vec(args, "current_categories");
    let mut merged: Vec<String> = Vec::new();
    for c in existing.into_iter().chain(changed) {
        if !merged.contains(&c) {
            merged.push(c);
        }
    }
    if add {
        return Ok(merged);
    }
    let remove = str_vec(args, key);
    Ok(merged.into_iter().filter(|c| !remove.contains(c)).collect())
}

fn attachment_meta(a: &Value) -> Value {
    json!({"id": a["id"], "name": a["name"], "content_type": a["contentType"], "size": a["size"],
        "is_inline": a["isInline"], "last_modified": a["lastModifiedDateTime"],
        "attachment_type": a["@odata.type"]})
}

/// Reduce one Graph `$batch` response to succeeded/failed lists. `request_id` N
/// corresponds to the Nth input ID.
fn parse_batch(v: &Value, with_new_id: bool) -> Value {
    let mut succeeded = Vec::new();
    let mut failed = Vec::new();
    for r in v["responses"].as_array().map(|a| a.as_slice()).unwrap_or(&[]) {
        let id = r["id"].as_str().unwrap_or("").to_string();
        let status = r["status"].as_u64().unwrap_or(0);
        if (200..300).contains(&status) {
            let mut item = json!({"request_id": id});
            if with_new_id {
                item["new_message_id"] = r["body"]["id"].clone();
            }
            succeeded.push(item);
        } else {
            let code = r["body"]["error"]["code"].as_str().unwrap_or("");
            let msg = r["body"]["error"]["message"].as_str().unwrap_or("");
            let error = match (code.is_empty(), msg.is_empty()) {
                (true, true) => format!("HTTP {status}"),
                (false, true) => code.to_string(),
                _ => format!("{code}: {msg}"),
            };
            failed.push(json!({"request_id": id, "status": status, "error": error}));
        }
    }
    json!({
        "success": failed.is_empty(),
        "requested_count": succeeded.len() + failed.len(),
        "succeeded_count": succeeded.len(),
        "failed_count": failed.len(),
        "succeeded": succeeded,
        "failed": failed,
    })
}

impl Outlook {
    pub fn with_api_base(api_base: &str) -> Self {
        let msg_props: Vec<(&str, Value)> = vec![
            ("recipients", str_list("Empfänger / recipients")),
            ("cc_recipients", str_list("CC / cc")),
            ("bcc_recipients", str_list("BCC / bcc")),
            ("subject", s("Betreff / subject")),
            (
                "body_format",
                menu("Textformat des Bodys / body content type", &["text", "html"]),
            ),
            ("body", s("Text (Text oder HTML) / body (text or html)")),
        ];
        let t = |slug, title, desc, props: Props<'_>, req: &[&str], scopes: &[&str], m| {
            tool(SLUG, slug, title, desc, props, req, scopes, m)
        };
        let spec = ToolkitSpec {
            slug: SLUG.into(),
            name: "Microsoft Outlook".into(),
            description: LocalizedText::new(
                "Outlook-Postfach lesen, durchsuchen, entwerfen und senden (Microsoft Graph).",
                "Read, search, draft and send from an Outlook mailbox (Microsoft Graph).",
            ),
            auth_schemes: vec![AuthScheme::OAuth2(OAuth2Scheme {
                authorize_url: "https://login.microsoftonline.com/common/oauth2/v2.0/authorize".into(),
                token_url: "https://login.microsoftonline.com/common/oauth2/v2.0/token".into(),
                default_scopes: vec![
                    MAIL_RW.into(),
                    MAIL_SEND.into(),
                    "Calendars.Read".into(),
                    "offline_access".into(),
                    USER_READ.into(),
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
                    "OUTLOOK_GET_MY_PROFILE",
                    ("Mein Profil abrufen", "Get my profile"),
                    (
                        "Liest Identität und primäre Adresse des verbundenen Kontos.",
                        "Reads the identity of the connected account.",
                    ),
                    &[],
                    &[],
                    &[USER_READ],
                    false,
                ),
                t(
                    "OUTLOOK_LIST_MESSAGES",
                    ("Nachrichten auflisten", "List messages"),
                    (
                        "Listet Nachrichten des Postfachs oder eines Ordners auf.",
                        "Lists messages of the mailbox or a single folder.",
                    ),
                    &[
                        ("folder_id", s("Ordner-ID oder bekannter Name / folder ID or well-known name")),
                        ("filter", s("Optionaler OData-$filter-Ausdruck / optional OData $filter expression")),
                        ("max_results", int("1-100 / 1-100", 1, 100, 25)),
                        ("skip", int("Anzahl zu überspringender Nachrichten / messages to skip", 0, 100_000, 0)),
                    ],
                    &[],
                    &[MAIL_RW],
                    false,
                ),
                t(
                    "OUTLOOK_SEARCH_MESSAGES",
                    ("Nachrichten suchen", "Search messages"),
                    (
                        "Volltextsuche (KQL) über Nachrichten, z.B. `from:x subject:y`.",
                        "Full-text (KQL) search over messages, e.g. `from:x subject:y`.",
                    ),
                    &[
                        ("search_query", s("Suchbegriffe / keywords or KQL terms")),
                        ("folder_id", s("Nur in diesem Ordner suchen / search this folder only")),
                        ("max_results", int("1-1000 / 1-1000", 1, 1000, 25)),
                    ],
                    &["search_query"],
                    &[MAIL_RW],
                    false,
                ),
                t(
                    "OUTLOOK_GET_MESSAGE",
                    ("Nachricht lesen", "Read message"),
                    (
                        "Liest eine einzelne Nachricht anhand ihrer ID.",
                        "Reads a single message by its ID.",
                    ),
                    &[("message_id", s("Nachrichten-ID / message ID"))],
                    &["message_id"],
                    &[MAIL_RW],
                    false,
                ),
                t(
                    "OUTLOOK_GET_MESSAGE_MIME",
                    ("MIME-Inhalt laden", "Get message MIME"),
                    (
                        "Lädt den rohen RFC-822-Inhalt (.eml) einer Nachricht.",
                        "Downloads the raw RFC-822 (.eml) content of a message.",
                    ),
                    &[("message_id", s("Nachrichten-ID / message ID"))],
                    &["message_id"],
                    &[MAIL_RW],
                    false,
                ),
                t(
                    "OUTLOOK_LIST_MAIL_FOLDERS",
                    ("E-Mail-Ordner auflisten", "List mail folders"),
                    (
                        "Listet die obersten E-Mail-Ordner des Postfachs auf.",
                        "Lists the top-level mail folders of the mailbox.",
                    ),
                    &[("max_results", int("1-100 / 1-100", 1, 100, 50))],
                    &[],
                    &[MAIL_RW],
                    false,
                ),
                t(
                    "OUTLOOK_LIST_CHILD_MAIL_FOLDERS",
                    ("Unterordner auflisten", "List child mail folders"),
                    (
                        "Listet die Unterordner eines E-Mail-Ordners.",
                        "Lists the sub-folders of one mail folder.",
                    ),
                    &[
                        ("folder_id", s("Übergeordnete Ordner-ID / parent folder ID")),
                        ("max_results", int("1-100 / 1-100", 1, 100, 50)),
                    ],
                    &["folder_id"],
                    &[MAIL_RW],
                    false,
                ),
                t(
                    "OUTLOOK_GET_MAIL_FOLDER",
                    ("E-Mail-Ordner lesen", "Get mail folder"),
                    (
                        "Liest einen Ordner anhand ID oder bekanntem Namen (inbox, drafts, …).",
                        "Reads one folder by ID or well-known name (inbox, drafts, …).",
                    ),
                    &[("folder_id", s("Ordner-ID oder bekannter Name / folder ID or well-known name"))],
                    &["folder_id"],
                    &[MAIL_RW],
                    false,
                ),
                t(
                    "OUTLOOK_LIST_MESSAGE_ATTACHMENTS",
                    ("Anhänge auflisten", "List message attachments"),
                    (
                        "Listet die Anhänge einer Nachricht ohne deren Inhalt.",
                        "Lists a message's attachments without their content.",
                    ),
                    &[("message_id", s("Nachrichten-ID / message ID"))],
                    &["message_id"],
                    &[MAIL_RW],
                    false,
                ),
                t(
                    "OUTLOOK_GET_MESSAGE_ATTACHMENT",
                    ("Anhang lesen", "Get message attachment"),
                    (
                        "Liest die Metadaten eines Anhangs.",
                        "Reads the metadata of one attachment.",
                    ),
                    &[
                        ("message_id", s("Nachrichten-ID / message ID")),
                        ("attachment_id", s("Anhang-ID / attachment ID")),
                    ],
                    &["message_id", "attachment_id"],
                    &[MAIL_RW],
                    false,
                ),
                t(
                    "OUTLOOK_GET_MAIL_TIPS",
                    ("E-Mail-Hinweise abrufen", "Get mail tips"),
                    (
                        "Prüft Empfänger auf Abwesenheit und Zustell-Hinweise.",
                        "Checks recipients for auto-replies and delivery tips.",
                    ),
                    &[("email_addresses", str_list("Empfängeradressen / recipient addresses"))],
                    &["email_addresses"],
                    &[MAIL_RW],
                    false,
                ),
                t(
                    "OUTLOOK_GET_SUPPORTED_TIME_ZONES",
                    ("Zeitzonen abrufen", "Get supported time zones"),
                    (
                        "Listet die von Outlook unterstützten Zeitzonen.",
                        "Lists the time zones Outlook supports for this account.",
                    ),
                    &[],
                    &[],
                    &[MAIL_RW],
                    false,
                ),
                t(
                    "OUTLOOK_LIST_FOCUSED_INBOX_OVERRIDES",
                    ("Fokus-Posteingang-Regeln auflisten", "List focused inbox overrides"),
                    (
                        "Listet Absender-Regeln des Fokus-Posteingangs.",
                        "Lists the Focused Inbox sender overrides.",
                    ),
                    &[],
                    &[],
                    &[MAIL_RW],
                    false,
                ),
                t(
                    "OUTLOOK_LIST_MESSAGES_DELTA",
                    ("Nachrichtenänderungen (Delta)", "List message changes (delta)"),
                    (
                        "Zählt neue, geänderte und gelöschte Nachrichten seit dem letzten Delta-Link auf.",
                        "Tracks created, updated and deleted messages since a delta link.",
                    ),
                    &[
                        ("folder_id", s("Zu beobachtender Ordner / folder to track")),
                        ("delta_link", s("Delta-Link eines vorherigen Aufrufs / delta link from a previous call")),
                    ],
                    &["folder_id"],
                    &[MAIL_RW],
                    false,
                ),
                // --- compose / send ------------------------------------------------
                t(
                    "OUTLOOK_SEND_EMAIL",
                    ("E-Mail senden", "Send email"),
                    (
                        "Verfasst und sendet eine neue E-Mail; braucht --confirm.",
                        "Composes and sends a new email; requires --confirm.",
                    ),
                    &msg_props,
                    &["recipients", "subject", "body_format", "body"],
                    &[MAIL_SEND],
                    true,
                ),
                t(
                    "OUTLOOK_CREATE_DRAFT",
                    ("Entwurf anlegen", "Create draft"),
                    (
                        "Legt eine ungesendete Nachricht (Entwurf) an.",
                        "Creates an unsent message (draft).",
                    ),
                    &[
                        ("recipients", str_list("Empfänger / recipients")),
                        ("cc_recipients", str_list("CC / cc")),
                        ("bcc_recipients", str_list("BCC / bcc")),
                        ("subject", s("Betreff / subject")),
                        ("body_format", menu("Textformat / body content type", &["text", "html"])),
                        ("body", s("Text / body")),
                        ("folder_id", s("Zielordner, leer = Entwürfe / target folder, empty = Drafts")),
                    ],
                    &["subject", "body_format", "body"],
                    &[MAIL_RW],
                    true,
                ),
                t(
                    "OUTLOOK_SEND_DRAFT",
                    ("Entwurf senden", "Send draft"),
                    ("Sendet einen vorhandenen Entwurf.", "Sends an existing draft."),
                    &[("message_id", s("Entwurf-ID / draft message ID"))],
                    &["message_id"],
                    &[MAIL_SEND],
                    true,
                ),
                t(
                    "OUTLOOK_REPLY_TO_MESSAGE",
                    ("Auf Nachricht antworten", "Reply to message"),
                    ("Antwortet dem Absender einer Nachricht.", "Replies to the sender of a message."),
                    &[
                        ("message_id", s("Nachrichten-ID / message ID")),
                        ("comment", s("Antworttext / reply text")),
                        ("cc_recipients", str_list("Zusätzliche CC / additional cc")),
                    ],
                    &["message_id", "comment"],
                    &[MAIL_SEND],
                    true,
                ),
                t(
                    "OUTLOOK_REPLY_ALL_TO_MESSAGE",
                    ("Allen antworten", "Reply all to message"),
                    (
                        "Antwortet Absender und allen Empfängern.",
                        "Replies to the sender and all recipients.",
                    ),
                    &[
                        ("message_id", s("Nachrichten-ID / message ID")),
                        ("comment", s("Antworttext / reply text")),
                    ],
                    &["message_id", "comment"],
                    &[MAIL_SEND],
                    true,
                ),
                t(
                    "OUTLOOK_FORWARD_MESSAGE",
                    ("Nachricht weiterleiten", "Forward message"),
                    (
                        "Leitet eine Nachricht an neue Empfänger weiter.",
                        "Forwards a message to new recipients.",
                    ),
                    &[
                        ("message_id", s("Nachrichten-ID / message ID")),
                        ("recipients", str_list("Empfänger / recipients")),
                        ("comment", s("Optionaler Kommentar / optional comment")),
                    ],
                    &["message_id", "recipients"],
                    &[MAIL_SEND],
                    true,
                ),
                // --- organize messages ---------------------------------------------
                t(
                    "OUTLOOK_MOVE_MESSAGE",
                    ("Nachricht verschieben", "Move message"),
                    (
                        "Verschiebt eine Nachricht in einen anderen Ordner (neue ID).",
                        "Moves a message into another folder (new ID).",
                    ),
                    &[
                        ("message_id", s("Nachrichten-ID / message ID")),
                        ("destination_folder_id", s("Zielordner / destination folder ID")),
                    ],
                    &["message_id", "destination_folder_id"],
                    &[MAIL_RW],
                    true,
                ),
                t(
                    "OUTLOOK_COPY_MESSAGE",
                    ("Nachricht kopieren", "Copy message"),
                    (
                        "Kopiert eine Nachricht in einen anderen Ordner.",
                        "Copies a message into another folder.",
                    ),
                    &[
                        ("message_id", s("Nachrichten-ID / message ID")),
                        ("destination_folder_id", s("Zielordner / destination folder ID")),
                    ],
                    &["message_id", "destination_folder_id"],
                    &[MAIL_RW],
                    true,
                ),
                t(
                    "OUTLOOK_DELETE_MESSAGE",
                    ("Nachricht löschen", "Delete message"),
                    (
                        "Verschiebt eine Nachricht in „Gelöschte Elemente“.",
                        "Moves a message to Deleted Items.",
                    ),
                    &[("message_id", s("Nachrichten-ID / message ID"))],
                    &["message_id"],
                    &[MAIL_RW],
                    true,
                ),
                t(
                    "OUTLOOK_PERMANENTLY_DELETE_MESSAGE",
                    ("Nachricht endgültig löschen", "Permanently delete message"),
                    (
                        "Löscht eine Nachricht unwiederbringlich; braucht --confirm.",
                        "Permanently deletes a message; requires --confirm.",
                    ),
                    &[
                        ("message_id", s("Nachrichten-ID / message ID")),
                        ("confirm", boolean("Muss aktiviert sein / must be enabled")),
                    ],
                    &["message_id", "confirm"],
                    &[MAIL_RW],
                    true,
                ),
                t(
                    "OUTLOOK_SET_MESSAGE_READ_STATUS",
                    ("Lesestatus setzen", "Mark message read / unread"),
                    (
                        "Markiert eine Nachricht als gelesen oder ungelesen.",
                        "Marks a message as read or unread.",
                    ),
                    &[
                        ("message_id", s("Nachrichten-ID / message ID")),
                        ("is_read", menu("Lesestatus / read state", &["true", "false"])),
                    ],
                    &["message_id", "is_read"],
                    &[MAIL_RW],
                    true,
                ),
                t(
                    "OUTLOOK_UPDATE_MESSAGE",
                    ("Nachricht aktualisieren", "Update message"),
                    (
                        "Ändert Lesestatus, Kennzeichnung, Wichtigkeit, Betreff oder Text; Kategorien über ADD/REMOVE_MESSAGE_CATEGORIES.",
                        "Updates read state, flag, importance, subject or body; categories via ADD/REMOVE_MESSAGE_CATEGORIES.",
                    ),
                    &[
                        ("message_id", s("Nachrichten-ID / message ID")),
                        ("is_read", menu("Lesestatus / read state", &["true", "false"])),
                        ("flag_status", menu("Kennzeichnung / flag state", &["flagged", "notFlagged", "complete"])),
                        ("importance", menu("Wichtigkeit / importance", &["low", "normal", "high"])),
                        ("subject", s("Neuer Betreff (nur Entwurf) / new subject (draft only)")),
                        ("body", s("Neuer Text (nur Entwurf) / new body (draft only)")),
                        ("body_format", menu("Textformat des neuen Bodys / new body format", &["text", "html"])),
                    ],
                    &["message_id"],
                    &[MAIL_RW],
                    true,
                ),
                t(
                    "OUTLOOK_ADD_MESSAGE_CATEGORIES",
                    ("Kategorien hinzufügen", "Add categories to message"),
                    (
                        "Fügt Kategorien hinzu; bestehende bleiben erhalten (current_categories aus Get Message).",
                        "Adds categories while keeping the existing ones (current_categories from Get Message).",
                    ),
                    &[
                        ("message_id", s("Nachrichten-ID / message ID")),
                        ("categories", str_list("Hinzuzufügende Kategorien / categories to add")),
                        ("current_categories", str_list("Bisherige Kategorien / categories already on the message")),
                    ],
                    &["message_id", "categories"],
                    &[MAIL_RW],
                    true,
                ),
                t(
                    "OUTLOOK_REMOVE_MESSAGE_CATEGORIES",
                    ("Kategorien entfernen", "Remove categories from message"),
                    (
                        "Entfernt Kategorien und behält die übrigen (current_categories aus Get Message).",
                        "Removes categories and keeps the rest (current_categories from Get Message).",
                    ),
                    &[
                        ("message_id", s("Nachrichten-ID / message ID")),
                        ("categories_to_remove", str_list("Zu entfernende Kategorien / categories to remove")),
                        ("current_categories", str_list("Bisherige Kategorien / categories already on the message")),
                    ],
                    &["message_id", "categories_to_remove"],
                    &[MAIL_RW],
                    true,
                ),
                t(
                    "OUTLOOK_BATCH_MOVE_MESSAGES",
                    ("Nachrichten stapelverschieben", "Batch move messages"),
                    (
                        "Verschiebt bis zu 20 Nachrichten per Graph-$batch in einen Ordner.",
                        "Moves up to 20 messages into one folder via a Graph $batch call.",
                    ),
                    &[
                        ("message_ids", str_list("Nachrichten-IDs / message IDs")),
                        ("destination_folder_id", s("Zielordner / destination folder ID")),
                    ],
                    &["message_ids", "destination_folder_id"],
                    &[MAIL_RW],
                    true,
                ),
                t(
                    "OUTLOOK_BATCH_UPDATE_MESSAGES",
                    ("Nachrichten stapelweise ändern", "Batch update messages"),
                    (
                        "Setzt Lesestatus/Kennzeichnung/Wichtigkeit für bis zu 20 Nachrichten.",
                        "Applies read/flag/importance changes to up to 20 messages.",
                    ),
                    &[
                        ("message_ids", str_list("Nachrichten-IDs / message IDs")),
                        ("is_read", menu("Lesestatus / read state", &["true", "false"])),
                        ("flag_status", menu("Kennzeichnung / flag state", &["flagged", "notFlagged", "complete"])),
                        ("importance", menu("Wichtigkeit / importance", &["low", "normal", "high"])),
                    ],
                    &["message_ids"],
                    &[MAIL_RW],
                    true,
                ),
                // --- folders --------------------------------------------------------
                t(
                    "OUTLOOK_CREATE_MAIL_FOLDER",
                    ("E-Mail-Ordner anlegen", "Create mail folder"),
                    ("Legt einen E-Mail-Ordner an.", "Creates a mail folder."),
                    &[
                        ("display_name", s("Ordnername / folder name")),
                        ("parent_folder_id", s("Übergeordneter Ordner, leer = oberste Ebene / parent folder, empty = top level")),
                    ],
                    &["display_name"],
                    &[MAIL_RW],
                    true,
                ),
                t(
                    "OUTLOOK_RENAME_MAIL_FOLDER",
                    ("E-Mail-Ordner umbenennen", "Rename mail folder"),
                    ("Benennt einen E-Mail-Ordner um.", "Renames a mail folder."),
                    &[
                        ("folder_id", s("Ordner-ID / folder ID")),
                        ("display_name", s("Neuer Name / new name")),
                    ],
                    &["folder_id", "display_name"],
                    &[MAIL_RW],
                    true,
                ),
                t(
                    "OUTLOOK_DELETE_MAIL_FOLDER",
                    ("E-Mail-Ordner löschen", "Delete mail folder"),
                    ("Löscht einen Ordner samt Inhalt.", "Deletes a folder and everything inside it."),
                    &[("folder_id", s("Ordner-ID / folder ID"))],
                    &["folder_id"],
                    &[MAIL_RW],
                    true,
                ),
                t(
                    "OUTLOOK_MOVE_MAIL_FOLDER",
                    ("E-Mail-Ordner verschieben", "Move mail folder"),
                    (
                        "Hängt einen Ordner unter einen anderen Ordner.",
                        "Moves a folder under another folder.",
                    ),
                    &[
                        ("folder_id", s("Ordner-ID / folder ID")),
                        ("destination_folder_id", s("Neuer Elternordner / new parent folder ID")),
                    ],
                    &["folder_id", "destination_folder_id"],
                    &[MAIL_RW],
                    true,
                ),
                t(
                    "OUTLOOK_COPY_MAIL_FOLDER",
                    ("E-Mail-Ordner kopieren", "Copy mail folder"),
                    (
                        "Kopiert einen Ordner in einen Zielordner.",
                        "Copies a folder into a destination folder.",
                    ),
                    &[
                        ("folder_id", s("Ordner-ID / folder ID")),
                        ("destination_folder_id", s("Zielordner / destination folder ID")),
                    ],
                    &["folder_id", "destination_folder_id"],
                    &[MAIL_RW],
                    true,
                ),
                // --- focused inbox overrides ----------------------------------------
                t(
                    "OUTLOOK_CREATE_FOCUSED_INBOX_OVERRIDE",
                    ("Fokus-Regel anlegen", "Create focused inbox override"),
                    (
                        "Sortiert Absender dauerhaft in Fokus oder „Andere“.",
                        "Pins future mail from a sender to Focused or Other.",
                    ),
                    &[
                        ("sender_email_address", s("Absenderadresse / sender address")),
                        ("classify_as", menu("Einordnung / classify as", &["focused", "other"])),
                        ("sender_name", s("Optionaler Anzeigename / optional display name")),
                    ],
                    &["sender_email_address", "classify_as"],
                    &[MAIL_RW],
                    true,
                ),
                t(
                    "OUTLOOK_UPDATE_FOCUSED_INBOX_OVERRIDE",
                    ("Fokus-Regel ändern", "Update focused inbox override"),
                    (
                        "Ändert die Einordnung einer Fokus-Regel.",
                        "Changes an override's classification.",
                    ),
                    &[
                        ("override_id", s("Regel-ID / override ID")),
                        ("classify_as", menu("Einordnung / classify as", &["focused", "other"])),
                    ],
                    &["override_id", "classify_as"],
                    &[MAIL_RW],
                    true,
                ),
                t(
                    "OUTLOOK_DELETE_FOCUSED_INBOX_OVERRIDE",
                    ("Fokus-Regel löschen", "Delete focused inbox override"),
                    ("Entfernt eine Absender-Regel.", "Removes a sender override."),
                    &[("override_id", s("Regel-ID / override ID"))],
                    &["override_id"],
                    &[MAIL_RW],
                    true,
                ),
                t(
                    "OUTLOOK_DELETE_MESSAGE_ATTACHMENT",
                    ("Anhang löschen", "Delete message attachment"),
                    ("Entfernt einen Anhang endgültig.", "Removes an attachment permanently."),
                    &[
                        ("message_id", s("Nachrichten-ID / message ID")),
                        ("attachment_id", s("Anhang-ID / attachment ID")),
                    ],
                    &["message_id", "attachment_id"],
                    &[MAIL_RW],
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
        format!("{}/v1.0/me{path}", self.api_base)
    }

    /// Validate a delta link returned by a previous delta call (origin and path
    /// pattern `/{v1.0|beta}/me/mailFolders/*/messages/delta`).
    fn resolve_delta_link(&self, link: &str) -> Result<String> {
        let expectation = "expected an absolute https URL on this Graph endpoint with path \
                           /{v1.0|beta}/me/mailFolders/{folderId}/messages/delta";
        let link = link.trim();
        let parsed = url::Url::parse(link).map_err(|_| anyhow!("invalid delta_link: {expectation}"))?;
        let base_origin = url::Url::parse(&self.api_base)
            .map_err(|_| anyhow!("invalid api_base"))?
            .origin();
        if parsed.origin() != base_origin || parsed.scheme() != "https" {
            bail!("delta_link points at another endpoint: {expectation}");
        }
        let segs: Vec<&str> = parsed.path().split('/').filter(|s| !s.is_empty()).collect();
        let ok = segs.len() == 6
            && (segs[0] == "v1.0" || segs[0] == "beta")
            && segs[1] == "me"
            && segs[2] == "mailFolders"
            && !segs[3].is_empty()
            && segs[4] == "messages"
            && segs[5] == "delta";
        if !ok {
            bail!("delta_link addresses another resource: {expectation}");
        }
        Ok(link.to_string())
    }
}

impl ToolkitAdapter for Outlook {
    fn spec(&self) -> &ToolkitSpec {
        &self.spec
    }

    fn build_request(&self, tool: &str, args: &Value) -> Result<HttpRequest> {
        Ok(match tool {
            "OUTLOOK_GET_MY_PROFILE" => {
                HttpRequest::new("GET", self.url("?$select=id,displayName,mail,userPrincipalName"))
            }
            "OUTLOOK_LIST_MESSAGES" => {
                let path = match opt_str(args, "folder_id") {
                    Some(_) => format!("/mailFolders/{}/messages", graph_id(args, "folder_id")?),
                    None => "/messages".to_string(),
                };
                let q = odata_query(&[
                    ("$select", Some(SELECT_MESSAGE.to_string())),
                    (
                        "$top",
                        Some(opt_u64(args, "max_results", 25, 1, 100).to_string()),
                    ),
                    ("$filter", opt_str(args, "filter").map(String::from)),
                    (
                        "$orderby",
                        if opt_str(args, "filter").is_none() {
                            Some("receivedDateTime desc".to_string())
                        } else {
                            None
                        },
                    ),
                    (
                        "$skip",
                        match args.get("skip").and_then(Value::as_u64) {
                            Some(0) | None => None,
                            Some(n) => Some(n.to_string()),
                        },
                    ),
                ]);
                HttpRequest::new("GET", self.url(&format!("{path}{q}")))
                    .header("prefer", TEXT_BODY_PREFER)
            }
            "OUTLOOK_SEARCH_MESSAGES" => {
                let path = match opt_str(args, "folder_id") {
                    Some(_) => format!("/mailFolders/{}/messages", graph_id(args, "folder_id")?),
                    None => "/messages".to_string(),
                };
                let q = odata_query(&[
                    (
                        "$search",
                        Some(format!("\"{}\"", req_str(args, "search_query")?)),
                    ),
                    (
                        "$top",
                        Some(opt_u64(args, "max_results", 25, 1, 1000).to_string()),
                    ),
                ]);
                HttpRequest::new("GET", self.url(&format!("{path}{q}")))
                    .header("consistencylevel", "eventual")
                    .header("prefer", TEXT_BODY_PREFER)
            }
            "OUTLOOK_GET_MESSAGE" => HttpRequest::new(
                "GET",
                self.url(&format!("/messages/{}", graph_id(args, "message_id")?)),
            )
            .header("prefer", TEXT_BODY_PREFER),
            "OUTLOOK_GET_MESSAGE_MIME" => HttpRequest::new(
                "GET",
                self.url(&format!("/messages/{}/$value", graph_id(args, "message_id")?)),
            ),
            "OUTLOOK_LIST_MAIL_FOLDERS" => {
                let q = odata_query(&[
                    ("includeHiddenFolders", Some("true".into())),
                    ("$select", Some(SELECT_FOLDER.to_string())),
                    ("$top", Some(opt_u64(args, "max_results", 50, 1, 100).to_string())),
                ]);
                HttpRequest::new("GET", self.url(&format!("/mailFolders{q}")))
            }
            "OUTLOOK_LIST_CHILD_MAIL_FOLDERS" => {
                let q = odata_query(&[
                    ("includeHiddenFolders", Some("true".into())),
                    ("$select", Some(SELECT_FOLDER.to_string())),
                    ("$top", Some(opt_u64(args, "max_results", 50, 1, 100).to_string())),
                ]);
                HttpRequest::new(
                    "GET",
                    self.url(&format!(
                        "/mailFolders/{}/childFolders{q}",
                        graph_id(args, "folder_id")?
                    )),
                )
            }
            "OUTLOOK_GET_MAIL_FOLDER" => {
                let q = odata_query(&[("$select", Some(SELECT_FOLDER.to_string()))]);
                HttpRequest::new(
                    "GET",
                    self.url(&format!("/mailFolders/{}{q}", graph_id(args, "folder_id")?)),
                )
            }
            "OUTLOOK_LIST_MESSAGE_ATTACHMENTS" => {
                let q = odata_query(&[("$select", Some(SELECT_ATTACHMENT.to_string()))]);
                HttpRequest::new(
                    "GET",
                    self.url(&format!(
                        "/messages/{}/attachments{q}",
                        graph_id(args, "message_id")?
                    )),
                )
            }
            "OUTLOOK_GET_MESSAGE_ATTACHMENT" => {
                let q = odata_query(&[("$select", Some(SELECT_ATTACHMENT.to_string()))]);
                HttpRequest::new(
                    "GET",
                    self.url(&format!(
                        "/messages/{}/attachments/{}{q}",
                        graph_id(args, "message_id")?,
                        graph_id(args, "attachment_id")?
                    )),
                )
            }
            "OUTLOOK_GET_MAIL_TIPS" => {
                let addresses = str_vec(args, "email_addresses");
                if addresses.is_empty() {
                    bail!("`email_addresses` must contain at least one address");
                }
                HttpRequest::new("POST", self.url("/getMailTips"))
                    .json(&json!({"EmailAddresses": addresses, "MailTipsOptions": MAIL_TIPS_OPTIONS}))
            }
            "OUTLOOK_GET_SUPPORTED_TIME_ZONES" => {
                HttpRequest::new("GET", self.url("/outlook/supportedTimeZones"))
            }
            "OUTLOOK_LIST_FOCUSED_INBOX_OVERRIDES" => {
                HttpRequest::new("GET", self.url("/inferenceClassification/overrides"))
            }
            "OUTLOOK_LIST_MESSAGES_DELTA" => {
                let url = match opt_str(args, "delta_link") {
                    Some(_) => self.resolve_delta_link(req_str(args, "delta_link")?)?,
                    None => self.url(&format!(
                        "/mailFolders/{}/messages/delta{}",
                        graph_id(args, "folder_id")?,
                        odata_query(&[("$select", Some(SELECT_MESSAGE.to_string()))])
                    )),
                };
                HttpRequest::new("GET", url).header("prefer", "odata.maxpagesize=50")
            }
            "OUTLOOK_SEND_EMAIL" => {
                if str_vec(args, "recipients").is_empty() {
                    bail!("missing argument `recipients`");
                }
                HttpRequest::new("POST", self.url("/sendMail")).json(&json!({
                    "message": mail_payload(args)?,
                    "saveToSentItems": "true",
                }))
            }
            "OUTLOOK_CREATE_DRAFT" => {
                let payload = mail_payload(args)?;
                let url = match opt_str(args, "folder_id") {
                    Some(_) => format!(
                        "/mailFolders/{}/messages",
                        graph_id(args, "folder_id")?
                    ),
                    None => "/messages".to_string(),
                };
                HttpRequest::new("POST", self.url(&url)).json(&payload)
            }
            "OUTLOOK_SEND_DRAFT" => HttpRequest::new(
                "POST",
                self.url(&format!("/messages/{}/send", graph_id(args, "message_id")?)),
            )
            .json(&json!({})),
            "OUTLOOK_REPLY_TO_MESSAGE" => {
                let mut payload = json!({"comment": req_str(args, "comment")?});
                let cc = str_vec(args, "cc_recipients");
                if !cc.is_empty() {
                    payload["message"] = json!({"ccRecipients": addr_list(cc)});
                }
                HttpRequest::new(
                    "POST",
                    self.url(&format!("/messages/{}/reply", graph_id(args, "message_id")?)),
                )
                .json(&payload)
            }
            "OUTLOOK_REPLY_ALL_TO_MESSAGE" => HttpRequest::new(
                "POST",
                self.url(&format!(
                    "/messages/{}/replyAll",
                    graph_id(args, "message_id")?
                )),
            )
            .json(&json!({"comment": req_str(args, "comment")?})),
            "OUTLOOK_FORWARD_MESSAGE" => {
                if str_vec(args, "recipients").is_empty() {
                    bail!("missing argument `recipients`");
                }
                HttpRequest::new(
                    "POST",
                    self.url(&format!(
                        "/messages/{}/forward",
                        graph_id(args, "message_id")?
                    )),
                )
                .json(&json!({
                    "comment": opt_str(args, "comment").unwrap_or(""),
                    "toRecipients": addr_list(str_vec(args, "recipients")),
                }))
            }
            "OUTLOOK_MOVE_MESSAGE" => HttpRequest::new(
                "POST",
                self.url(&format!("/messages/{}/move", graph_id(args, "message_id")?)),
            )
            .json(&json!({"destinationId": req_str(args, "destination_folder_id")?})),
            "OUTLOOK_COPY_MESSAGE" => HttpRequest::new(
                "POST",
                self.url(&format!("/messages/{}/copy", graph_id(args, "message_id")?)),
            )
            .json(&json!({"destinationId": req_str(args, "destination_folder_id")?})),
            "OUTLOOK_DELETE_MESSAGE" => HttpRequest::new(
                "DELETE",
                self.url(&format!("/messages/{}", graph_id(args, "message_id")?)),
            ),
            "OUTLOOK_PERMANENTLY_DELETE_MESSAGE" => {
                if !args.get("confirm").and_then(Value::as_bool).unwrap_or(false) {
                    bail!("`confirm` must be true to permanently delete a message");
                }
                HttpRequest::new(
                    "POST",
                    self.url(&format!(
                        "/messages/{}/permanentDelete",
                        graph_id(args, "message_id")?
                    )),
                )
                .json(&json!({}))
            }
            "OUTLOOK_SET_MESSAGE_READ_STATUS" => HttpRequest::new(
                "PATCH",
                self.url(&format!("/messages/{}", graph_id(args, "message_id")?)),
            )
            .json(&json!({"isRead": req_one_of(args, "is_read", &["true", "false"])? == "true"}))
            .header("prefer", TEXT_BODY_PREFER),
            "OUTLOOK_UPDATE_MESSAGE" => HttpRequest::new(
                "PATCH",
                self.url(&format!("/messages/{}", graph_id(args, "message_id")?)),
            )
            .json(&update_payload(args)?)
            .header("prefer", TEXT_BODY_PREFER),
            "OUTLOOK_ADD_MESSAGE_CATEGORIES" => HttpRequest::new(
                "PATCH",
                self.url(&format!("/messages/{}", graph_id(args, "message_id")?)),
            )
            .json(&json!({"categories": merged_categories(args, true)?}))
            .header("prefer", TEXT_BODY_PREFER),
            "OUTLOOK_REMOVE_MESSAGE_CATEGORIES" => HttpRequest::new(
                "PATCH",
                self.url(&format!("/messages/{}", graph_id(args, "message_id")?)),
            )
            .json(&json!({"categories": merged_categories(args, false)?}))
            .header("prefer", TEXT_BODY_PREFER),
            "OUTLOOK_BATCH_MOVE_MESSAGES" => {
                let ids = str_vec(args, "message_ids");
                if ids.is_empty() {
                    bail!("missing argument `message_ids`");
                }
                if ids.len() > BATCH_LIMIT {
                    bail!("Microsoft Graph allows at most {BATCH_LIMIT} sub-requests per $batch; split the list");
                }
                let dest = req_str(args, "destination_folder_id")?;
                let requests: Vec<Value> = ids
                    .iter()
                    .enumerate()
                    .map(|(i, id)| {
                        json!({
                            "id": (i + 1).to_string(),
                            "method": "POST",
                            "url": format!("/me/messages/{}/move", pct_encode(id.trim())),
                            "headers": {"Content-Type": "application/json"},
                            "body": {"destinationId": dest},
                        })
                    })
                    .collect();
                HttpRequest::new("POST", self.url("/$batch")).json(&json!({"requests": requests}))
            }
            "OUTLOOK_BATCH_UPDATE_MESSAGES" => {
                let ids = str_vec(args, "message_ids");
                if ids.is_empty() {
                    bail!("missing argument `message_ids`");
                }
                if ids.len() > BATCH_LIMIT {
                    bail!("Microsoft Graph allows at most {BATCH_LIMIT} sub-requests per $batch; split the list");
                }
                let payload = update_payload(args)?;
                let requests: Vec<Value> = ids
                    .iter()
                    .enumerate()
                    .map(|(i, id)| {
                        json!({
                            "id": (i + 1).to_string(),
                            "method": "PATCH",
                            "url": format!("/me/messages/{}", pct_encode(id.trim())),
                            "headers": {"Content-Type": "application/json"},
                            "body": payload,
                        })
                    })
                    .collect();
                HttpRequest::new("POST", self.url("/$batch")).json(&json!({"requests": requests}))
            }
            "OUTLOOK_CREATE_MAIL_FOLDER" => {
                let url = match opt_str(args, "parent_folder_id") {
                    Some(_) => format!(
                        "/mailFolders/{}/childFolders",
                        graph_id(args, "parent_folder_id")?
                    ),
                    None => "/mailFolders".to_string(),
                };
                HttpRequest::new("POST", self.url(&url))
                    .json(&json!({"displayName": req_str(args, "display_name")?}))
            }
            "OUTLOOK_RENAME_MAIL_FOLDER" => HttpRequest::new(
                "PATCH",
                self.url(&format!("/mailFolders/{}", graph_id(args, "folder_id")?)),
            )
            .json(&json!({"displayName": req_str(args, "display_name")?})),
            "OUTLOOK_DELETE_MAIL_FOLDER" => HttpRequest::new(
                "DELETE",
                self.url(&format!("/mailFolders/{}", graph_id(args, "folder_id")?)),
            ),
            "OUTLOOK_MOVE_MAIL_FOLDER" => HttpRequest::new(
                "POST",
                self.url(&format!(
                    "/mailFolders/{}/move",
                    graph_id(args, "folder_id")?
                )),
            )
            .json(&json!({"destinationId": req_str(args, "destination_folder_id")?})),
            "OUTLOOK_COPY_MAIL_FOLDER" => HttpRequest::new(
                "POST",
                self.url(&format!(
                    "/mailFolders/{}/copy",
                    graph_id(args, "folder_id")?
                )),
            )
            .json(&json!({"destinationId": req_str(args, "destination_folder_id")?})),
            "OUTLOOK_CREATE_FOCUSED_INBOX_OVERRIDE" => {
                let mut sender = Map::new();
                if let Some(name) = opt_str(args, "sender_name") {
                    sender.insert("name".into(), json!(name));
                }
                sender.insert(
                    "address".into(),
                    json!(req_str(args, "sender_email_address")?),
                );
                HttpRequest::new("POST", self.url("/inferenceClassification/overrides"))
                    .json(&json!({
                        "classifyAs": req_one_of(args, "classify_as", &["focused", "other"])?,
                        "senderEmailAddress": Value::Object(sender),
                    }))
            }
            "OUTLOOK_UPDATE_FOCUSED_INBOX_OVERRIDE" => HttpRequest::new(
                "PATCH",
                self.url(&format!(
                    "/inferenceClassification/overrides/{}",
                    graph_id(args, "override_id")?
                )),
            )
            .json(&json!({"classifyAs": req_one_of(args, "classify_as", &["focused", "other"])?})),
            "OUTLOOK_DELETE_FOCUSED_INBOX_OVERRIDE" => HttpRequest::new(
                "DELETE",
                self.url(&format!(
                    "/inferenceClassification/overrides/{}",
                    graph_id(args, "override_id")?
                )),
            ),
            "OUTLOOK_DELETE_MESSAGE_ATTACHMENT" => HttpRequest::new(
                "DELETE",
                self.url(&format!(
                    "/messages/{}/attachments/{}",
                    graph_id(args, "message_id")?,
                    graph_id(args, "attachment_id")?
                )),
            ),
            other => bail!("unknown Outlook tool `{other}`"),
        })
    }

    fn parse_response(&self, tool: &str, response: &HttpResponse) -> Result<Value> {
        let v = response.json()?;
        Ok(match tool {
            "OUTLOOK_GET_MY_PROFILE" => json!({"id": v["id"], "display_name": v["displayName"],
                "mail": v["mail"], "user_principal_name": v["userPrincipalName"]}),
            "OUTLOOK_LIST_MESSAGES" => page(&v, "messages"),
            "OUTLOOK_SEARCH_MESSAGES" => {
                let mut page = page(&v, "messages");
                page["found"] = json!(v["value"].as_array().is_some_and(|a| !a.is_empty()));
                page
            }
            "OUTLOOK_GET_MESSAGE" => v,
            "OUTLOOK_GET_MESSAGE_MIME" => {
                let bytes = &response.body;
                json!({"file_name": "message.eml", "content_type": "message/rfc822",
                    "size": bytes.len(),
                    "content_base64": base64::engine::general_purpose::STANDARD.encode(bytes)})
            }
            "OUTLOOK_LIST_MAIL_FOLDERS" => page(&v, "folders"),
            "OUTLOOK_LIST_CHILD_MAIL_FOLDERS" => page(&v, "folders"),
            "OUTLOOK_GET_MAIL_FOLDER" => v,
            "OUTLOOK_LIST_MESSAGE_ATTACHMENTS" => json!({
                "attachments": v["value"].as_array().map(|a| a.iter().map(attachment_meta).collect::<Vec<_>>())
                    .unwrap_or_default(),
                "count": v["value"].as_array().map(|a| a.len()).unwrap_or(0),
            }),
            "OUTLOOK_GET_MESSAGE_ATTACHMENT" => attachment_meta(&v),
            "OUTLOOK_GET_MAIL_TIPS" => json!({"mail_tips": v["value"],
                "count": v["value"].as_array().map(|a| a.len()).unwrap_or(0)}),
            "OUTLOOK_GET_SUPPORTED_TIME_ZONES" => json!({"time_zones": v["value"],
                "count": v["value"].as_array().map(|a| a.len()).unwrap_or(0)}),
            "OUTLOOK_LIST_FOCUSED_INBOX_OVERRIDES" => json!({"overrides": v["value"],
                "count": v["value"].as_array().map(|a| a.len()).unwrap_or(0)}),
            "OUTLOOK_LIST_MESSAGES_DELTA" => {
                let changes: Vec<Value> = v["value"]
                    .as_array()
                    .map(|a| {
                        a.iter()
                            .map(|c| {
                                let mut o = c.clone();
                                o["removed"] = json!(c.get("@removed").is_some());
                                o["removed_reason"] = c["@removed"]["reason"].clone();
                                if let Value::Object(map) = &mut o {
                                    map.remove("@removed");
                                }
                                o
                            })
                            .collect()
                    })
                    .unwrap_or_default();
                json!({"changes": changes, "count": changes.len(),
                    "has_more": v["@odata.nextLink"].is_string(),
                    "next_link": v["@odata.nextLink"], "delta_link": v["@odata.deltaLink"]})
            }
            "OUTLOOK_SEND_EMAIL" => json!({"success": true, "message": "Email sent successfully."}),
            "OUTLOOK_CREATE_DRAFT" => v,
            "OUTLOOK_SEND_DRAFT" => json!({"success": true, "message": "Draft sent successfully."}),
            "OUTLOOK_REPLY_TO_MESSAGE" => json!({"success": true, "message": "Reply sent successfully."}),
            "OUTLOOK_REPLY_ALL_TO_MESSAGE" => {
                json!({"success": true, "message": "Reply-all sent successfully."})
            }
            "OUTLOOK_FORWARD_MESSAGE" => {
                json!({"success": true, "message": "Message forwarded successfully."})
            }
            "OUTLOOK_MOVE_MESSAGE" | "OUTLOOK_COPY_MESSAGE" => {
                json!({"success": true, "new_message_id": v["id"], "message": v})
            }
            "OUTLOOK_DELETE_MESSAGE" => {
                json!({"success": true, "message": "Message moved to Deleted Items."})
            }
            "OUTLOOK_PERMANENTLY_DELETE_MESSAGE" => {
                json!({"success": true, "message": "Message permanently deleted."})
            }
            "OUTLOOK_SET_MESSAGE_READ_STATUS" | "OUTLOOK_UPDATE_MESSAGE" => v,
            "OUTLOOK_ADD_MESSAGE_CATEGORIES" | "OUTLOOK_REMOVE_MESSAGE_CATEGORIES" => v,
            "OUTLOOK_BATCH_MOVE_MESSAGES" => parse_batch(&v, true),
            "OUTLOOK_BATCH_UPDATE_MESSAGES" => parse_batch(&v, false),
            "OUTLOOK_CREATE_MAIL_FOLDER" => {
                json!({"created": true, "folder": v, "folder_id": v["id"]})
            }
            "OUTLOOK_RENAME_MAIL_FOLDER" => v,
            "OUTLOOK_DELETE_MAIL_FOLDER" => {
                json!({"success": true, "message": "Mail folder deleted."})
            }
            "OUTLOOK_MOVE_MAIL_FOLDER" | "OUTLOOK_COPY_MAIL_FOLDER" => {
                json!({"success": true, "new_folder_id": v["id"], "folder": v})
            }
            "OUTLOOK_CREATE_FOCUSED_INBOX_OVERRIDE" | "OUTLOOK_UPDATE_FOCUSED_INBOX_OVERRIDE" => v,
            "OUTLOOK_DELETE_FOCUSED_INBOX_OVERRIDE" => {
                json!({"success": true, "message": "Focused Inbox override deleted."})
            }
            "OUTLOOK_DELETE_MESSAGE_ATTACHMENT" => {
                json!({"success": true, "message": "Attachment deleted."})
            }
            _ => v,
        })
    }
}

/// Graph page collection -> `{items, count, has_more, next_link}`.
fn page(v: &Value, key: &str) -> Value {
    json!({key: v["value"], "count": v["value"].as_array().map(|a| a.len()).unwrap_or(0),
        "has_more": v["@odata.nextLink"].is_string(), "next_link": v["@odata.nextLink"]})
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encodes_graph_ids_and_validates_delta_links() {
        let o = Outlook::with_api_base("https://graph.test");
        let r = o
            .build_request("OUTLOOK_GET_MESSAGE", &json!({"message_id": " AAMk AAA=/x="}))
            .unwrap();
        assert_eq!(r.url, "https://graph.test/v1.0/me/messages/AAMk%20AAA%3D%2Fx%3D");
        assert!(o
            .build_request("OUTLOOK_GET_MESSAGE", &json!({"message_id": "   "}))
            .is_err());
        let good = "https://graph.test/v1.0/me/mailFolders/inbox/messages/delta?$deltatoken=x";
        assert_eq!(
            o.build_request(
                "OUTLOOK_LIST_MESSAGES_DELTA",
                &json!({"folder_id": "inbox", "delta_link": good})
            )
            .unwrap()
            .url,
            good
        );
        for bad in [
            "http://graph.test/v1.0/me/mailFolders/inbox/messages/delta",
            "https://evil.test/v1.0/me/mailFolders/inbox/messages/delta",
            "https://graph.test/v1.0/me/messages/delta",
            "https://graph.test/beta/me/mailFolders/inbox/childFolders/delta",
            "not a url",
        ] {
            assert!(o
                .build_request(
                    "OUTLOOK_LIST_MESSAGES_DELTA",
                    &json!({"folder_id": "inbox", "delta_link": bad})
                )
                .is_err(),
                "{bad} must be rejected"
            );
        }
        assert!(o
            .build_request("OUTLOOK_PERMANENTLY_DELETE_MESSAGE", &json!({"message_id": "m", "confirm": false}))
            .is_err());
        assert!(o
            .build_request("OUTLOOK_UPDATE_MESSAGE", &json!({"message_id": "m"}))
            .is_err());
    }
}
