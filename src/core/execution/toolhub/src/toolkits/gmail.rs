//! Gmail toolkit (Gmail API v1). Action set modelled on Activepieces `piece-gmail` (MIT).
//!
//! Ported from `src/lib/actions/*.ts`; actions needing two chained calls
//! (reply/forward: fetch original + send; get-or-create label: list + create)
//! and file uploads are not ported.
use anyhow::{bail, Result};
use base64::Engine as _;
use serde_json::{json, Value};

use super::common::*;
use super::{path_segment, ToolkitAdapter};
use crate::http::{HttpRequest, HttpResponse};
use crate::model::*;

pub const SLUG: &str = "gmail";
const READ: &str = "https://www.googleapis.com/auth/gmail.readonly";
const SEND: &str = "https://www.googleapis.com/auth/gmail.send";
const MODIFY: &str = "https://www.googleapis.com/auth/gmail.modify";
const COMPOSE: &str = "https://www.googleapis.com/auth/gmail.compose";
const EMAIL: &str = "https://www.googleapis.com/auth/email";
/// Gmail's batch modify ceiling (modify-labels-action.ts).
const BATCH_MODIFY_LIMIT: u64 = 1000;

pub struct Gmail {
    spec: ToolkitSpec,
    api_base: String,
}

impl Default for Gmail {
    fn default() -> Self {
        Self::with_api_base("https://gmail.googleapis.com")
    }
}

fn body_type_prop() -> Value {
    json!({"type": "string", "enum": ["plain_text", "html"], "default": "plain_text",
        "description": "Wie der Body interpretiert wird / how the body text is interpreted"})
}

fn flag(desc: &str) -> Value {
    json!({"type": "boolean", "description": desc})
}

/// Shared MIME-level props for send/draft tools (Activepieces `receiver/cc/bcc/...`).
fn mail_props() -> Vec<(&'static str, Value)> {
    vec![
        ("to", str_list("Empfänger / recipients")),
        ("cc", str_list("CC")),
        ("bcc", str_list("BCC")),
        ("subject", s("Betreff / subject")),
        ("body", s("Text")),
        ("body_type", body_type_prop()),
        ("body_html", s("Optional HTML body (overrides body_type)")),
        ("from", s("Optionale Absendeadresse (Send-as) / optional send-as From address")),
        ("sender_name", s("Optionaler Anzeigename / optional From display name")),
        ("reply_to", str_list("Optionale Reply-To-Adressen / optional Reply-To addresses")),
        ("in_reply_to", s("Optional Message-ID, auf die geantwortet wird (In-Reply-To/References)")),
        ("thread_id", s("Antwort innerhalb dieses Threads / reply within this thread")),
    ]
}

impl Gmail {
    pub fn with_api_base(api_base: &str) -> Self {
        let t = |slug, title, desc, props: Props<'_>, req: &[&str], scopes: &[&str], m| {
            tool(SLUG, slug, title, desc, props, req, scopes, m)
        };
        let spec = ToolkitSpec {
            slug: SLUG.into(),
            name: "Gmail".into(),
            description: LocalizedText::new(
                "E-Mails in Gmail suchen, lesen, entwerfen und senden.",
                "Search, read, draft and send Gmail messages.",
            ),
            auth_schemes: vec![
                AuthScheme::OAuth2(OAuth2Scheme {
                    authorize_url: "https://accounts.google.com/o/oauth2/auth".into(),
                    token_url: "https://oauth2.googleapis.com/token".into(),
                    default_scopes: vec![
                        SEND.into(),
                        READ.into(),
                        COMPOSE.into(),
                        MODIFY.into(),
                        EMAIL.into(),
                    ],
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
                // --- read / list / search ----------------------------------
                t(
                    "GMAIL_SEARCH_MESSAGES",
                    ("E-Mails suchen", "Search emails"),
                    (
                        "Sucht Nachrichten mit Gmail-Suchsyntax.",
                        "Searches messages using Gmail search syntax.",
                    ),
                    &[
                        ("query", s("e.g. `from:x is:unread`")),
                        ("label_ids", str_list("Label IDs")),
                        ("max_results", int("1-100", 1, 100, 20)),
                        ("page_token", s("Next page token")),
                    ],
                    &[],
                    &[READ],
                    false,
                ),
                t(
                    "GMAIL_SEARCH_MAIL",
                    ("E-Mails finden", "Find email"),
                    (
                        "Durchsucht das Postfach mit Filtern (Absender, Betreff, Anhang, Datum); ohne Filter die neuesten E-Mails.",
                        "Searches the mailbox with filters (sender, subject, attachment, dates); without filters the latest emails.",
                    ),
                    &[
                        ("from", s("Nur E-Mails von diesem Absender / only emails from this sender")),
                        ("to", s("Nur E-Mails an diesen Empfänger / only emails to this recipient")),
                        ("subject", s("Nur E-Mails mit diesem Betrefftext / only emails with this subject text")),
                        ("content", s("Nur E-Mails mit diesem Text im Body / only emails with this body text")),
                        ("has_attachment", flag("Nur E-Mails mit Anhang / only emails with an attachment")),
                        ("attachment_name", s("Nur E-Mails mit diesem Anhangdateinamen / attachment file name")),
                        ("label", s("Nur E-Mails mit diesem Label (Name oder ID) / label name or ID")),
                        ("category", s("Postfach-Tab: primary, social, promotions, updates, forums, …")),
                        ("after_date", s("Nur E-Mails nach diesem Datum (ISO 8601) / received after this date")),
                        ("before_date", s("Nur E-Mails vor diesem Datum (ISO 8601) / received before this date")),
                        ("include_spam_trash", flag("Spam und Papierkorb einbeziehen / include Spam and Trash")),
                        ("max_results", int("Neueste zuerst, 1-500 / newest first, 1-500", 1, 500, 10)),
                    ],
                    &[],
                    &[READ],
                    false,
                ),
                t(
                    "GMAIL_GET_MESSAGE",
                    ("E-Mail lesen", "Read email"),
                    (
                        "Liest eine Nachricht mit Kopfzeilen und Text.",
                        "Reads a message with headers and text.",
                    ),
                    &[("message_id", s("Message ID"))],
                    &["message_id"],
                    &[READ],
                    false,
                ),
                t(
                    "GMAIL_GET_THREAD",
                    ("Konversation lesen", "Read thread"),
                    (
                        "Liest alle Nachrichten eines Threads.",
                        "Reads all messages of a thread.",
                    ),
                    &[("thread_id", s("Thread ID"))],
                    &["thread_id"],
                    &[READ],
                    false,
                ),
                t(
                    "GMAIL_LIST_LABELS",
                    ("Labels auflisten", "List labels"),
                    ("Listet die Labels des Postfachs.", "Lists mailbox labels."),
                    &[],
                    &[],
                    &[READ],
                    false,
                ),
                t(
                    "GMAIL_LIST_DRAFTS",
                    ("Entwürfe auflisten", "List drafts"),
                    (
                        "Listet Entwürfe auf, optional gefiltert; Auflösung für Get/Update/Send/Delete Draft.",
                        "Lists drafts, optionally filtered; resolver for Get/Update/Send/Delete Draft.",
                    ),
                    &[
                        ("query", s("Optionale Gmail-Suchabfrage, z.B. `subject:invoice`")),
                        ("max_results", int("1-500", 1, 500, 20)),
                    ],
                    &[],
                    &[READ],
                    false,
                ),
                t(
                    "GMAIL_LIST_THREADS",
                    ("Konversationen auflisten", "List threads"),
                    (
                        "Listet Konversationen auf; Auflösung für Thread-IDs.",
                        "Lists conversation threads; resolver for thread IDs.",
                    ),
                    &[
                        ("query", s("z.B. `from:boss@example.com is:unread`")),
                        ("label_ids", str_list("Label-IDs (z.B. INBOX, UNREAD)")),
                        ("include_spam_trash", flag("Spam & Papierkorb einbeziehen / include Spam & Trash")),
                        ("max_results", int("1-500", 1, 500, 20)),
                    ],
                    &[],
                    &[READ],
                    false,
                ),
                t(
                    "GMAIL_LIST_HISTORY",
                    ("Postfach-Verlauf", "List history"),
                    (
                        "Änderungen im Postfach seit einer History-ID (aus Get Profile).",
                        "Mailbox changes since a history ID (from Get Profile).",
                    ),
                    &[
                        ("start_history_id", s("Änderungen nach dieser History-ID / changes after this history ID")),
                        ("history_types", str_list("messageAdded, messageDeleted, labelAdded, labelRemoved")),
                        ("max_results", int("1-500", 1, 500, 100)),
                    ],
                    &["start_history_id"],
                    &[READ],
                    false,
                ),
                t(
                    "GMAIL_GET_DRAFT",
                    ("Entwurf lesen", "Get draft"),
                    (
                        "Liest einen Entwurf per ID.",
                        "Reads a single draft by ID.",
                    ),
                    &[("draft_id", s("Draft ID (from List Drafts)"))],
                    &["draft_id"],
                    &[READ],
                    false,
                ),
                t(
                    "GMAIL_GET_LABEL",
                    ("Label lesen", "Get label"),
                    (
                        "Liest ein Label per ID inkl. Nachrichten- und Thread-Zähler.",
                        "Reads a label by ID incl. message and thread counts.",
                    ),
                    &[("label_id", s("Label ID (from List Labels)"))],
                    &["label_id"],
                    &[READ],
                    false,
                ),
                t(
                    "GMAIL_GET_ATTACHMENT",
                    ("Anhang laden", "Get attachment"),
                    (
                        "Lädt einen Anhang per Nachrichten- und Anhang-ID (Base64).",
                        "Downloads an attachment by message and attachment ID (base64).",
                    ),
                    &[
                        ("message_id", s("Message ID")),
                        ("attachment_id", s("Attachment ID (payload parts[].body.attachmentId)")),
                        ("file_name", s("Optionaler Dateiname / optional file name")),
                    ],
                    &["message_id", "attachment_id"],
                    &[READ],
                    false,
                ),
                t(
                    "GMAIL_GET_PROFILE",
                    ("Profil abrufen", "Get profile"),
                    (
                        "Gibt das verbundene Postfachprofil zurück (E-Mail, Zähler, History-ID).",
                        "Returns the connected mailbox profile (email, counts, history ID).",
                    ),
                    &[],
                    &[],
                    &[READ],
                    false,
                ),
                // --- write / send ------------------------------------------
                t(
                    "GMAIL_ARCHIVE_MESSAGE",
                    ("E-Mail archivieren", "Archive email"),
                    (
                        "Archiviert eine Nachricht (INBOX-Label entfernen).",
                        "Archives a message (removes the INBOX label).",
                    ),
                    &[("message_id", s("Message ID"))],
                    &["message_id"],
                    &[MODIFY],
                    true,
                ),
                t(
                    "GMAIL_TRASH_MESSAGE",
                    ("E-Mail in den Papierkorb", "Trash email"),
                    (
                        "Verschiebt eine Nachricht in den Papierkorb (wiederherstellbar).",
                        "Moves a message to Trash (recoverable).",
                    ),
                    &[("message_id", s("Message ID"))],
                    &["message_id"],
                    &[MODIFY],
                    true,
                ),
                t(
                    "GMAIL_UNTRASH_MESSAGE",
                    ("E-Mail wiederherstellen", "Untrash email"),
                    (
                        "Holt eine Nachricht aus dem Papierkorb zurück.",
                        "Restores a message from Trash.",
                    ),
                    &[("message_id", s("Message ID"))],
                    &["message_id"],
                    &[MODIFY],
                    true,
                ),
                t(
                    "GMAIL_ADD_LABEL_TO_EMAIL",
                    ("Label an E-Mail anhängen", "Add label to email"),
                    (
                        "Fügt einer Nachricht ein Label hinzu.",
                        "Applies a label to a message.",
                    ),
                    &[
                        ("message_id", s("Message ID")),
                        ("label_id", s("Label ID (from List Labels)")),
                    ],
                    &["message_id", "label_id"],
                    &[MODIFY],
                    true,
                ),
                t(
                    "GMAIL_REMOVE_LABEL_FROM_EMAIL",
                    ("Label von E-Mail entfernen", "Remove label from email"),
                    (
                        "Entfernt ein Label von einer Nachricht.",
                        "Removes a label from a message.",
                    ),
                    &[
                        ("message_id", s("Message ID")),
                        ("label_id", s("Label ID (from List Labels)")),
                    ],
                    &["message_id", "label_id"],
                    &[MODIFY],
                    true,
                ),
                t(
                    "GMAIL_MODIFY_LABELS",
                    ("Labels ändern", "Modify labels"),
                    (
                        "Labels hinzufügen/entfernen (Archivieren = INBOX entfernen).",
                        "Add/remove labels (archive = remove INBOX).",
                    ),
                    &[
                        ("message_id", s("Message ID")),
                        ("add_label_ids", str_list("Add")),
                        ("remove_label_ids", str_list("Remove")),
                    ],
                    &["message_id"],
                    &[MODIFY],
                    true,
                ),
                t(
                    "GMAIL_BATCH_MODIFY_LABELS",
                    ("Labels mehrerer E-Mails ändern", "Batch modify labels"),
                    (
                        "Labels für bis zu 1000 Nachrichten in einem Aufruf ändern.",
                        "Add/remove labels across up to 1000 messages in one call.",
                    ),
                    &[
                        ("message_ids", str_list("Message IDs (max 1000)")),
                        ("add_label_ids", str_list("Add")),
                        ("remove_label_ids", str_list("Remove")),
                    ],
                    &["message_ids"],
                    &[MODIFY],
                    true,
                ),
                t(
                    "GMAIL_MODIFY_THREAD_LABELS",
                    ("Labels einer Konversation ändern", "Modify thread labels"),
                    (
                        "Labels für alle Nachrichten eines Threads ändern.",
                        "Add/remove labels across every message in a thread.",
                    ),
                    &[
                        ("thread_id", s("Thread ID")),
                        ("add_label_ids", str_list("Add")),
                        ("remove_label_ids", str_list("Remove")),
                    ],
                    &["thread_id"],
                    &[MODIFY],
                    true,
                ),
                t(
                    "GMAIL_CREATE_LABEL",
                    ("Label anlegen", "Create label"),
                    (
                        "Legt ein neues Label im Postfach an.",
                        "Creates a new mailbox label.",
                    ),
                    &[("name", s("Label name shown in Gmail"))],
                    &["name"],
                    &[MODIFY],
                    true,
                ),
                t(
                    "GMAIL_UPDATE_LABEL",
                    ("Label aktualisieren", "Update label"),
                    (
                        "Benennt ein Label um oder ändert Farbe/Sichtbarkeit; nicht gesetzte Felder bleiben unverändert.",
                        "Renames a label or changes color/visibility; unset fields stay unchanged.",
                    ),
                    &[
                        ("label_id", s("Label ID of a user label (from List Labels)")),
                        ("name", s("Neuer Name (leer = unverändert) / new name (empty = keep)")),
                        ("label_list_visibility", json!({"type": "string", "enum": ["labelShow", "labelShowIfUnread", "labelHide"],
                            "description": "Label-Listen-Sichtbarkeit / label list visibility"})),
                        ("message_list_visibility", json!({"type": "string", "enum": ["show", "hide"],
                            "description": "Nachrichten-Listen-Sichtbarkeit / message list visibility"})),
                        ("background_color", s("Hintergrundfarbe (hex, leer = unverändert) / background color hex")),
                        ("text_color", s("Textfarbe (hex, leer = unverändert) / text color hex")),
                    ],
                    &["label_id"],
                    &[MODIFY],
                    true,
                ),
                t(
                    "GMAIL_CREATE_DRAFT",
                    ("Entwurf anlegen", "Create draft"),
                    (
                        "Legt einen Entwurf an (wird nicht gesendet).",
                        "Creates a draft (not sent).",
                    ),
                    &mail_props(),
                    &["to", "subject"],
                    &[COMPOSE],
                    true,
                ),
                t(
                    "GMAIL_UPDATE_DRAFT",
                    ("Entwurf aktualisieren", "Update draft"),
                    (
                        "Ersetzt den Inhalt eines vorhandenen Entwurfs.",
                        "Replaces the content of an existing draft.",
                    ),
                    &[mail_props(), vec![("draft_id", s("Draft ID (from List Drafts)"))]].concat(),
                    &["draft_id", "subject", "body"],
                    &[COMPOSE],
                    true,
                ),
                t(
                    "GMAIL_SEND_DRAFT",
                    ("Entwurf senden", "Send draft"),
                    (
                        "Sendet einen vorhandenen Entwurf.",
                        "Sends an existing draft by ID.",
                    ),
                    &[("draft_id", s("Draft ID (from List Drafts)"))],
                    &["draft_id"],
                    &[SEND],
                    true,
                ),
                t(
                    "GMAIL_DELETE_DRAFT",
                    ("Entwurf löschen", "Delete draft"),
                    (
                        "Löscht einen Entwurf endgültig (nicht wiederherstellbar).",
                        "Permanently deletes a draft (not recoverable).",
                    ),
                    &[("draft_id", s("Draft ID (from List Drafts)"))],
                    &["draft_id"],
                    &[COMPOSE],
                    true,
                ),
                t(
                    "GMAIL_SEND_EMAIL",
                    ("E-Mail senden", "Send email"),
                    (
                        "Sendet eine E-Mail; braucht --confirm.",
                        "Sends an email; requires --confirm.",
                    ),
                    &mail_props(),
                    &["to", "subject"],
                    &[SEND],
                    true,
                ),
                t(
                    "GMAIL_STOP_WATCH",
                    ("Push-Beobachtung beenden", "Stop watch"),
                    (
                        "Beendet Push-Benachrichtigungen (Pub/Sub) auf dem Postfach.",
                        "Stops push (Pub/Sub) notifications on the mailbox.",
                    ),
                    &[],
                    &[],
                    &[MODIFY],
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
        format!("{}/gmail/v1/users/me{path}", self.api_base)
    }

    /// `after:`/`before:` Gmail dates are `YYYY/MM/DD` (search-email-action.ts).
    fn gmail_date(v: &str, key: &str) -> Result<String> {
        let b = v.as_bytes();
        let digits = |r: std::ops::Range<usize>| b[r].iter().all(u8::is_ascii_digit);
        let ok = v.len() >= 10
            && digits(0..4)
            && b[4] == b'-'
            && digits(5..7)
            && b[7] == b'-'
            && digits(8..10);
        if !ok {
            bail!("invalid argument `{key}` (expected ISO date)");
        }
        Ok(v[..10].replace('-', "/"))
    }

    fn trimmed(args: &Value, key: &str) -> Option<String> {
        opt_str(args, key)
            .map(str::trim)
            .filter(|v| !v.is_empty())
            .map(String::from)
    }

    pub fn raw_mime(args: &Value) -> Result<String> {
        let b64 = |x: &str| base64::engine::general_purpose::STANDARD.encode(x.as_bytes());
        if str_vec(args, "to").is_empty() {
            bail!("missing argument `to`");
        }
        let mut h = String::new();
        for (name, key) in [("To", "to"), ("Cc", "cc"), ("Bcc", "bcc")] {
            let v = str_vec(args, key);
            if !v.is_empty() {
                let joined = v.join(", ");
                header_safe(key, &joined)?;
                h.push_str(&format!("{name}: {joined}\r\n"));
            }
        }
        let reply_to = str_vec(args, "reply_to");
        if !reply_to.is_empty() {
            let joined = reply_to.join(", ");
            header_safe("reply_to", &joined)?;
            h.push_str(&format!("Reply-To: {joined}\r\n"));
        }
        if let Some(from) = Self::trimmed(args, "from") {
            let from = header_safe("from", &from)?;
            match Self::trimmed(args, "sender_name") {
                Some(n) => h.push_str(&format!(
                    "From: {} <{from}>\r\n",
                    header_safe("sender_name", &n)?
                )),
                None => h.push_str(&format!("From: {from}\r\n")),
            }
        }
        let subject = header_safe("subject", req_str(args, "subject")?)?;
        h.push_str(&format!(
            "Subject: =?UTF-8?B?{}?=\r\n",
            b64(subject)
        ));
        if let Some(irt) = Self::trimmed(args, "in_reply_to") {
            header_safe("in_reply_to", &irt)?;
            h.push_str(&format!("In-Reply-To: {irt}\r\nReferences: {irt}\r\n"));
        }
        h.push_str("MIME-Version: 1.0\r\n");
        let text = opt_str(args, "body").unwrap_or("");
        if let Some(html) = opt_str(args, "body_html") {
            let b = "ctox-toolhub-alt";
            h.push_str(&format!(
                "Content-Type: multipart/alternative; boundary=\"{b}\"\r\n\r\n"
            ));
            h.push_str(&format!("--{b}\r\nContent-Type: text/plain; charset=UTF-8\r\nContent-Transfer-Encoding: base64\r\n\r\n{}\r\n", b64(text)));
            h.push_str(&format!("--{b}\r\nContent-Type: text/html; charset=UTF-8\r\nContent-Transfer-Encoding: base64\r\n\r\n{}\r\n--{b}--\r\n", b64(html)));
        } else if Self::trimmed(args, "body_type").as_deref() == Some("html") {
            h.push_str(&format!("Content-Type: text/html; charset=UTF-8\r\nContent-Transfer-Encoding: base64\r\n\r\n{}\r\n", b64(text)));
        } else {
            h.push_str(&format!("Content-Type: text/plain; charset=UTF-8\r\nContent-Transfer-Encoding: base64\r\n\r\n{}\r\n", b64(text)));
        }
        Ok(base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(h.as_bytes()))
    }

    fn message_body(args: &Value) -> Result<Value> {
        let mut m = json!({"raw": Self::raw_mime(args)?});
        if let Some(t) = opt_str(args, "thread_id") {
            m["threadId"] = json!(t);
        }
        Ok(m)
    }

    /// `addLabelIds`/`removeLabelIds` pair; at least one side must be set.
    fn label_change(args: &Value) -> Result<(Vec<String>, Vec<String>)> {
        let (add, remove) = (
            str_vec(args, "add_label_ids"),
            str_vec(args, "remove_label_ids"),
        );
        if add.is_empty() && remove.is_empty() {
            bail!("missing argument `add_label_ids` or `remove_label_ids`");
        }
        Ok((add, remove))
    }
}

fn repeated(q: &mut String, key: &str, values: &[String]) {
    for v in values {
        let enc: String = url::form_urlencoded::byte_serialize(v.as_bytes()).collect();
        q.push_str(&format!("&{key}={enc}"));
    }
}

fn header(msg: &Value, name: &str) -> Value {
    msg["payload"]["headers"]
        .as_array()
        .and_then(|hs| {
            hs.iter().find(|h| {
                h["name"]
                    .as_str()
                    .is_some_and(|n| n.eq_ignore_ascii_case(name))
            })
        })
        .map(|h| h["value"].clone())
        .unwrap_or(Value::Null)
}

fn text_body(part: &Value) -> Option<String> {
    fn find(p: &Value, mime: &str) -> Option<String> {
        if p["mimeType"].as_str() == Some(mime) {
            if let Some(d) = p["body"]["data"].as_str() {
                let bytes = base64::engine::general_purpose::URL_SAFE_NO_PAD
                    .decode(d.trim_end_matches('='))
                    .ok()?;
                return Some(String::from_utf8_lossy(&bytes).into_owned());
            }
        }
        p["parts"].as_array()?.iter().find_map(|c| find(c, mime))
    }
    find(part, "text/plain").or_else(|| find(part, "text/html"))
}

fn summarize(m: &Value) -> Value {
    json!({"id": m["id"], "thread_id": m["threadId"], "label_ids": m["labelIds"], "snippet": m["snippet"],
        "from": header(m, "From"), "to": header(m, "To"), "subject": header(m, "Subject"),
        "date": header(m, "Date"), "body": text_body(&m["payload"])})
}

impl ToolkitAdapter for Gmail {
    fn spec(&self) -> &ToolkitSpec {
        &self.spec
    }

    fn build_request(&self, tool: &str, args: &Value) -> Result<HttpRequest> {
        Ok(match tool {
            // --- read / list / search ----------------------------------
            "GMAIL_SEARCH_MESSAGES" => {
                let mut q = query(&[
                    ("q", Self::trimmed(args, "query")),
                    (
                        "maxResults",
                        Some(opt_u64(args, "max_results", 20, 1, 100).to_string()),
                    ),
                    ("pageToken", opt_str(args, "page_token").map(String::from)),
                ]);
                repeated(&mut q, "labelIds", &str_vec(args, "label_ids"));
                HttpRequest::new("GET", self.url(&format!("/messages{q}")))
            }
            "GMAIL_SEARCH_MAIL" => {
                let mut parts: Vec<String> = Vec::new();
                for (key, prefix) in [("from", "from"), ("to", "to"), ("subject", "subject")] {
                    if let Some(v) = Self::trimmed(args, key) {
                        parts.push(format!("{prefix}:({v})"));
                    }
                }
                if let Some(c) = Self::trimmed(args, "content") {
                    parts.push(format!("\"{c}\""));
                }
                if args.get("has_attachment").and_then(Value::as_bool) == Some(true) {
                    parts.push("has:attachment".into());
                }
                if let Some(a) = Self::trimmed(args, "attachment_name") {
                    parts.push(format!("filename:({a})"));
                }
                if let Some(l) = Self::trimmed(args, "label") {
                    parts.push(format!("label:{l}"));
                }
                if let Some(c) = Self::trimmed(args, "category") {
                    parts.push(format!("category:{c}"));
                }
                for (key, word) in [("after_date", "after"), ("before_date", "before")] {
                    if let Some(d) = Self::trimmed(args, key) {
                        parts.push(format!("{word}:{}", Self::gmail_date(&d, key)?));
                    }
                }
                let qtext = parts.join(" ");
                let q = query(&[
                    ("q", (!qtext.is_empty()).then_some(qtext)),
                    ("maxResults", Some(opt_u64(args, "max_results", 10, 1, 500).to_string())),
                    (
                        "includeSpamTrash",
                        (args.get("include_spam_trash").and_then(Value::as_bool) == Some(true))
                            .then(|| "true".into()),
                    ),
                ]);
                HttpRequest::new("GET", self.url(&format!("/messages{q}")))
            }
            "GMAIL_GET_MESSAGE" => HttpRequest::new(
                "GET",
                self.url(&format!(
                    "/messages/{}?format=full",
                    path_segment(args, "message_id")?
                )),
            ),
            "GMAIL_GET_THREAD" => HttpRequest::new(
                "GET",
                self.url(&format!(
                    "/threads/{}?format=full",
                    path_segment(args, "thread_id")?
                )),
            ),
            "GMAIL_LIST_LABELS" => HttpRequest::new("GET", self.url("/labels")),
            "GMAIL_LIST_DRAFTS" => {
                let q = query(&[
                    ("q", Self::trimmed(args, "query")),
                    ("maxResults", Some(opt_u64(args, "max_results", 20, 1, 500).to_string())),
                ]);
                HttpRequest::new("GET", self.url(&format!("/drafts{q}")))
            }
            "GMAIL_LIST_THREADS" => {
                let mut q = query(&[
                    ("q", Self::trimmed(args, "query")),
                    (
                        "includeSpamTrash",
                        (args.get("include_spam_trash").and_then(Value::as_bool) == Some(true))
                            .then(|| "true".into()),
                    ),
                    ("maxResults", Some(opt_u64(args, "max_results", 20, 1, 500).to_string())),
                ]);
                repeated(&mut q, "labelIds", &str_vec(args, "label_ids"));
                HttpRequest::new("GET", self.url(&format!("/threads{q}")))
            }
            "GMAIL_LIST_HISTORY" => {
                let mut q = query(&[
                    (
                        "startHistoryId",
                        Some(req_str(args, "start_history_id")?.to_string()),
                    ),
                    ("maxResults", Some(opt_u64(args, "max_results", 100, 1, 500).to_string())),
                ]);
                repeated(&mut q, "historyTypes", &str_vec(args, "history_types"));
                HttpRequest::new("GET", self.url(&format!("/history{q}")))
            }
            "GMAIL_GET_DRAFT" => HttpRequest::new(
                "GET",
                self.url(&format!(
                    "/drafts/{}?format=full",
                    path_segment(args, "draft_id")?
                )),
            ),
            "GMAIL_GET_LABEL" => HttpRequest::new(
                "GET",
                self.url(&format!("/labels/{}", path_segment(args, "label_id")?)),
            ),
            "GMAIL_GET_ATTACHMENT" => {
                let mid = path_segment(args, "message_id")?;
                let aid = path_segment(args, "attachment_id")?;
                HttpRequest::new("GET", self.url(&format!("/messages/{mid}/attachments/{aid}")))
            }
            "GMAIL_GET_PROFILE" => HttpRequest::new("GET", self.url("/profile")),
            // --- write / send ------------------------------------------
            "GMAIL_ARCHIVE_MESSAGE" => HttpRequest::new(
                "POST",
                self.url(&format!(
                    "/messages/{}/modify",
                    path_segment(args, "message_id")?
                )),
            )
            .json(&json!({"removeLabelIds": ["INBOX"]})),
            "GMAIL_TRASH_MESSAGE" => HttpRequest::new(
                "POST",
                self.url(&format!(
                    "/messages/{}/trash",
                    path_segment(args, "message_id")?
                )),
            ),
            "GMAIL_UNTRASH_MESSAGE" => HttpRequest::new(
                "POST",
                self.url(&format!(
                    "/messages/{}/untrash",
                    path_segment(args, "message_id")?
                )),
            ),
            "GMAIL_ADD_LABEL_TO_EMAIL" | "GMAIL_REMOVE_LABEL_FROM_EMAIL" => {
                let body = if tool == "GMAIL_ADD_LABEL_TO_EMAIL" {
                    json!({"addLabelIds": [req_str(args, "label_id")?]})
                } else {
                    json!({"removeLabelIds": [req_str(args, "label_id")?]})
                };
                HttpRequest::new(
                    "POST",
                    self.url(&format!(
                        "/messages/{}/modify",
                        path_segment(args, "message_id")?
                    )),
                )
                .json(&body)
            }
            "GMAIL_MODIFY_LABELS" => {
                let (add, remove) = Self::label_change(args)?;
                HttpRequest::new(
                    "POST",
                    self.url(&format!(
                        "/messages/{}/modify",
                        path_segment(args, "message_id")?
                    )),
                )
                .json(&json!({"addLabelIds": add, "removeLabelIds": remove}))
            }
            "GMAIL_BATCH_MODIFY_LABELS" => {
                let ids = str_vec(args, "message_ids");
                if ids.is_empty() {
                    bail!("missing argument `message_ids`");
                }
                if ids.len() as u64 > BATCH_MODIFY_LIMIT {
                    bail!("at most {BATCH_MODIFY_LIMIT} message IDs per call");
                }
                let (add, remove) = Self::label_change(args)?;
                let mut body = json!({"ids": ids});
                if !add.is_empty() {
                    body["addLabelIds"] = json!(add);
                }
                if !remove.is_empty() {
                    body["removeLabelIds"] = json!(remove);
                }
                HttpRequest::new("POST", self.url("/messages/batchModify")).json(&body)
            }
            "GMAIL_MODIFY_THREAD_LABELS" => {
                let (add, remove) = Self::label_change(args)?;
                HttpRequest::new(
                    "POST",
                    self.url(&format!(
                        "/threads/{}/modify",
                        path_segment(args, "thread_id")?
                    )),
                )
                .json(&json!({"addLabelIds": add, "removeLabelIds": remove}))
            }
            "GMAIL_CREATE_LABEL" => HttpRequest::new("POST", self.url("/labels"))
                .json(&json!({"name": req_str(args, "name")?})),
            "GMAIL_UPDATE_LABEL" => {
                let mut body = json!({});
                for (key, json_key) in [
                    ("name", "name"),
                    ("label_list_visibility", "labelListVisibility"),
                    ("message_list_visibility", "messageListVisibility"),
                ] {
                    if let Some(v) = Self::trimmed(args, key) {
                        body[json_key] = json!(v);
                    }
                }
                let bg = Self::trimmed(args, "background_color");
                let tc = Self::trimmed(args, "text_color");
                if bg.is_some() || tc.is_some() {
                    body["color"] = json!({
                        "backgroundColor": bg,
                        "textColor": tc,
                    });
                }
                HttpRequest::new(
                    "PATCH",
                    self.url(&format!("/labels/{}", path_segment(args, "label_id")?)),
                )
                .json(&body)
            }
            "GMAIL_CREATE_DRAFT" => HttpRequest::new("POST", self.url("/drafts"))
                .json(&json!({"message": Self::message_body(args)?})),
            "GMAIL_UPDATE_DRAFT" => HttpRequest::new(
                "PATCH",
                self.url(&format!(
                    "/drafts/{}",
                    path_segment(args, "draft_id")?
                )),
            )
            .json(&json!({"message": Self::message_body(args)?})),
            "GMAIL_SEND_DRAFT" => HttpRequest::new("POST", self.url("/drafts/send"))
                .json(&json!({"id": req_str(args, "draft_id")?})),
            "GMAIL_DELETE_DRAFT" => HttpRequest::new(
                "DELETE",
                self.url(&format!("/drafts/{}", path_segment(args, "draft_id")?)),
            ),
            "GMAIL_SEND_EMAIL" => HttpRequest::new("POST", self.url("/messages/send"))
                .json(&Self::message_body(args)?),
            "GMAIL_STOP_WATCH" => HttpRequest::new("POST", self.url("/stop")),
            other => bail!("unknown Gmail tool `{other}`"),
        })
    }

    fn parse_response(&self, tool: &str, response: &HttpResponse) -> Result<Value> {
        let v = response.json()?;
        Ok(match tool {
            "GMAIL_GET_MESSAGE" => summarize(&v),
            "GMAIL_GET_THREAD" => json!({"id": v["id"], "messages":
                v["messages"].as_array().map(|m| m.iter().map(summarize).collect::<Vec<_>>())}),
            "GMAIL_SEARCH_MESSAGES" => {
                json!({"messages": v["messages"], "next_page_token": v["nextPageToken"],
                "result_size_estimate": v["resultSizeEstimate"]})
            }
            "GMAIL_SEARCH_MAIL" => {
                let n = v["messages"].as_array().map_or(0, |a| a.len());
                json!({"found": n > 0, "results": {"messages": v["messages"], "count": n}})
            }
            "GMAIL_LIST_LABELS" => json!(v["labels"].as_array().map(|l| l
                .iter()
                .map(|x| json!({"id": x["id"], "name": x["name"], "type": x["type"]}))
                .collect::<Vec<_>>())),
            "GMAIL_LIST_DRAFTS" | "GMAIL_LIST_THREADS" => {
                let (key, out) = if tool == "GMAIL_LIST_DRAFTS" {
                    ("drafts", "drafts")
                } else {
                    ("threads", "threads")
                };
                let n = v[key].as_array().map_or(0, |a| a.len());
                json!({out: v[key], "count": n, "result_size_estimate": v["resultSizeEstimate"],
                    "next_page_token": v["nextPageToken"]})
            }
            "GMAIL_GET_ATTACHMENT" => json!({"data": v["data"], "size": v["size"]}),
            "GMAIL_BATCH_MODIFY_LABELS" | "GMAIL_STOP_WATCH" => json!({"success": true}),
            "GMAIL_DELETE_DRAFT" => {
                json!({"success": true, "deleted": response.status != 404})
            }
            "GMAIL_SEND_DRAFT" => {
                json!({"id": v["id"], "thread_id": v["threadId"], "label_ids": v["labelIds"]})
            }
            "GMAIL_CREATE_DRAFT" | "GMAIL_UPDATE_DRAFT" => {
                json!({"draft_id": v["id"], "message_id": v["message"]["id"]})
            }
            "GMAIL_SEND_EMAIL" => {
                json!({"id": v["id"], "thread_id": v["threadId"], "label_ids": v["labelIds"]})
            }
            _ => v,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn requests_and_header_injection() {
        let g = Gmail::with_api_base("https://g.test");
        let r = g
            .build_request(
                "GMAIL_SEARCH_MESSAGES",
                &json!({"query": "is:unread from:a@b", "label_ids": ["INBOX"], "max_results": 999}),
            )
            .unwrap();
        assert_eq!(r.url, "https://g.test/gmail/v1/users/me/messages?q=is%3Aunread+from%3Aa%40b&maxResults=100&labelIds=INBOX");
        assert!(g
            .build_request("GMAIL_GET_MESSAGE", &json!({"message_id": "../x"}))
            .is_err());
        assert!(g
            .build_request(
                "GMAIL_SEND_EMAIL",
                &json!({"to": "a@b", "subject": "x\r\nBcc: evil@x"})
            )
            .is_err());
        let msg = json!({"id": "1", "payload": {"headers": [{"name": "Subject", "value": "S"}],
            "mimeType": "multipart/alternative", "parts": [{"mimeType": "text/plain", "body": {"data": "aGk"}}]}});
        assert_eq!(summarize(&msg)["body"], "hi");
    }

    #[test]
    fn search_mail_query_building() {
        let g = Gmail::with_api_base("https://g.test");
        let r = g
            .build_request(
                "GMAIL_SEARCH_MAIL",
                &json!({
                    "from": "boss@example.com",
                    "subject": "Invoice",
                    "content": "overdue",
                    "has_attachment": true,
                    "attachment_name": "inv.pdf",
                    "category": "updates",
                    "after_date": "2026-01-31T10:00:00Z",
                    "before_date": "2026-02-28T00:00:00Z",
                    "include_spam_trash": true,
                    "max_results": 500,
                }),
            )
            .unwrap();
        assert_eq!(
            r.url,
            "https://g.test/gmail/v1/users/me/messages?q=from%3A%28boss%40example.com%29+subject%3A%28Invoice%29+%22overdue%22+has%3Aattachment+filename%3A%28inv.pdf%29+category%3Aupdates+after%3A2026%2F01%2F31+before%3A2026%2F02%2F28&maxResults=500&includeSpamTrash=true"
        );
        assert!(g
            .build_request("GMAIL_SEARCH_MAIL", &json!({"after_date": "not-a-date"}))
            .is_err());
    }

    #[test]
    fn draft_and_label_writes() {
        let g = Gmail::with_api_base("https://g.test");
        let r = g
            .build_request(
                "GMAIL_UPDATE_DRAFT",
                &json!({"draft_id": "r99", "to": ["a@b"], "subject": "S", "body": "B", "body_type": "html"}),
            )
            .unwrap();
        assert_eq!(r.method, "PATCH");
        assert!(r.url.ends_with("/drafts/r99"));
        let body: Value = serde_json::from_slice(r.body.as_deref().unwrap()).unwrap();
        let raw = base64::engine::general_purpose::URL_SAFE_NO_PAD
            .decode(body["message"]["raw"].as_str().unwrap())
            .unwrap();
        assert!(String::from_utf8_lossy(&raw).contains("text/html"));

        let r = g
            .build_request(
                "GMAIL_BATCH_MODIFY_LABELS",
                &json!({"message_ids": ["m1", "m2"], "remove_label_ids": ["INBOX"]}),
            )
            .unwrap();
        let body: Value = serde_json::from_slice(&r.body.unwrap()).unwrap();
        assert_eq!(body, json!({"ids": ["m1", "m2"], "removeLabelIds": ["INBOX"]}));
        // At least one of add/remove label ids is required.
        assert!(g
            .build_request("GMAIL_BATCH_MODIFY_LABELS", &json!({"message_ids": ["m1"]}))
            .is_err());

        let r = g
            .build_request(
                "GMAIL_UPDATE_LABEL",
                &json!({"label_id": "Label_5", "name": "Follow Up", "background_color": "#fb4c2f"}),
            )
            .unwrap();
        assert_eq!(r.method, "PATCH");
        let body: Value = serde_json::from_slice(&r.body.unwrap()).unwrap();
        assert_eq!(
            body,
            json!({"name": "Follow Up", "color": {"backgroundColor": "#fb4c2f", "textColor": null}})
        );
    }
}
