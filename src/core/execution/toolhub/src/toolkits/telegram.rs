//! Telegram Bot toolkit (Bot API). Action set modelled on Activepieces
//! `telegram-bot` (MIT), files under `packages/pieces/community/telegram-bot/src`.
//!
//! Endpoints: the piece builds **every** URL as
//! `https://api.telegram.org/bot<token>/<method>` (`telegramCommons.getApiUrl`,
//! `src/lib/common/index.ts`) and sends a JSON body via
//! `httpClient.sendRequest({ method: HttpMethod.POST, url, body })`. Telegram
//! takes no query parameters, and no tool argument ends up in the path — the
//! method name is a constant and `chat_id`/`message_id` travel in the JSON body
//! — so no value needs URL encoding here.
//!
//! Auth (from `src/index.ts`): `PieceAuth.SecretText` — the bot token @BotFather
//! issued. The piece sends that token as a URL path segment and *no* auth
//! header at all, so this adapter takes the token through its base URL
//! ([`Telegram::with_api_base`], e.g. `https://api.telegram.org/bot<token>`)
//! instead of leaking it into a tool argument. The hub still injects the stored
//! credential as `X-Telegram-Bot-Api-Token-Key`, the header a self-hosted
//! `telegram-bot-api` server accepts, so one credential covers both hosts.
//! `builtin()` has no per-account wiring (same situation as Zendesk/Salesforce):
//! a caller that knows the token registers `Telegram::with_api_base(..)`.
//!
//! Scopes: the Bot API issues none — the token grants the full rights of the
//! bot — so every tool carries an empty scope list.
//!
//! Answers come back in Telegram's `{ok, result}` envelope, which the
//! Activepieces actions return verbatim (`return response.body`); the port does
//! the same and adds `count` for the list-shaped `getChatAdministrators`.
//! File uploads (`sendPhoto`/`sendVideo`/`sendDocument`/`sendAudio`/
//! `sendMediaGroup`), the webhook subscribe/unsubscribe helpers and the
//! approval waitpoint flow are not ported.
use anyhow::{bail, Result};
use serde_json::{json, Map, Value};

use super::common::*;
use super::ToolkitAdapter;
use crate::http::{HttpRequest, HttpResponse};
use crate::model::*;

pub const SLUG: &str = "telegram";

/// Base used by [`Telegram::default`]: the real host with a visible placeholder
/// token — deliberately not a working credential.
const DEFAULT_API_BASE: &str = "https://api.telegram.org/bot<bot-token>";

/// Chat actions of `telegram_send_chat_action` (`sendChatAction`).
const CHAT_ACTIONS: &[&str] = &[
    "typing",
    "upload_photo",
    "record_video",
    "upload_video",
    "record_voice",
    "upload_voice",
    "upload_document",
    "choose_sticker",
    "find_location",
    "record_video_note",
    "upload_video_note",
];

/// Text formats of `format` / `explanation_format` (`resolveParseMode` in
/// `src/lib/common/index.ts` maps `None` to "no parse_mode at all").
const FORMATS: &[&str] = &["None", "MarkdownV2", "HTML"];

// ---------------------------------------------------------------- schema helpers

/// Checkbox with the piece's default value.
fn flag(desc: &str, default: bool) -> Value {
    json!({"type": "boolean", "description": desc, "default": default})
}

/// Checkbox without a schema default, so an unset value stays unset.
fn toggle(desc: &str) -> Value {
    json!({"type": "boolean", "description": desc})
}

fn choice(desc: &str, options: &[&str], default: &str) -> Value {
    json!({"type": "string", "description": desc, "enum": options, "default": default})
}

/// Optional whole number; the piece sends the value through, we only document
/// the range Telegram documents.
fn opt_int(desc: &str, min: u64, max: u64) -> Value {
    json!({"type": "integer", "description": desc, "minimum": min, "maximum": max})
}

fn opt_num(desc: &str, min: f64, max: f64) -> Value {
    json!({"type": "number", "description": desc, "minimum": min, "maximum": max})
}

/// `Property.DateTime` input; the piece converts it to Unix seconds.
fn datetime(desc: &str) -> Value {
    s(desc)
}

fn string_list(desc: &str) -> Value {
    json!({"type": "array", "items": {"type": "string"}, "description": desc})
}

/// `reply_markup` of the piece (`Property.Json`) has to stay a JSON object.
fn markup() -> Value {
    json!({"type": "object", "description":
        "Reply Markup: raw Telegram interface options, e.g. {\"inline_keyboard\":[[{\"text\":\"Open\",\"url\":\"https://example.com\"}]]}. Malformed markup is rejected by Telegram."})
}

const CHAT_ID: &str =
    "Chat ID: numeric id or @username of a public channel. Ask @getmyid_bot for a private chat id; the bot must be able to reach the chat.";
const THREAD_ID: &str =
    "Message Thread Id: target topic of a forum supergroup; empty posts to the main chat.";
const SILENT: &str = "Disable Notification: deliver without a sound.";
const PROTECT: &str = "Protect Content: stop recipients from forwarding or saving the message.";
const NO_PREVIEW: &str = "Disable Web Page Preview: do not show a preview card for links.";
const REPLY_TO: &str = "Reply To Message Id: send as a reply to this message in the same chat.";

fn chat_id_prop() -> Value {
    s(CHAT_ID)
}
fn thread_prop() -> Value {
    s(THREAD_ID)
}

// ---------------------------------------------------------------- body helpers

fn put_str(m: &mut Map<String, Value>, args: &Value, key: &str, field: &str) {
    if let Some(v) = opt_str(args, key) {
        m.insert(field.into(), json!(v));
    }
}

/// Numeric field; the piece passes `Property.Number` values through as JSON
/// numbers, so integers stay integers and floats stay floats.
fn put_num(m: &mut Map<String, Value>, args: &Value, key: &str, field: &str) -> Result<()> {
    match args.get(key) {
        None | Some(Value::Null) => Ok(()),
        Some(v) if v.is_number() => {
            m.insert(field.into(), v.clone());
            Ok(())
        }
        Some(_) => bail!("`{key}` must be a number"),
    }
}

/// Checkbox: `ctx.propsValue.x ?? <default>` in the piece.
fn put_flag(m: &mut Map<String, Value>, args: &Value, key: &str, field: &str, default: bool) {
    m.insert(
        field.into(),
        json!(args.get(key).and_then(Value::as_bool).unwrap_or(default)),
    );
}

/// `telegramCommons.resolveParseMode`: `None`/unset means "send no parse_mode".
fn put_parse_mode(m: &mut Map<String, Value>, args: &Value, key: &str, field: &str) -> Result<()> {
    let Some(v) = opt_str(args, key) else { return Ok(()) };
    if !FORMATS.contains(&v) {
        bail!("`{key}` must be one of None, MarkdownV2, HTML");
    }
    if v != "None" {
        m.insert(field.into(), json!(v));
    }
    Ok(())
}

/// `Property.Json` reply markup, passed through untouched when given.
fn put_markup(m: &mut Map<String, Value>, args: &Value, field: &str) -> Result<()> {
    match args.get("reply_markup") {
        None | Some(Value::Null) => Ok(()),
        Some(v) if v.is_object() => {
            m.insert(field.into(), v.clone());
            Ok(())
        }
        Some(_) => bail!("`reply_markup` must be a JSON object"),
    }
}

/// `Property.DateTime` -> Unix seconds (`Math.floor(new Date(x).getTime() / 1000)`).
fn put_unix_time(m: &mut Map<String, Value>, args: &Value, key: &str, field: &str) -> Result<()> {
    let Some(v) = opt_str(args, key) else { return Ok(()) };
    m.insert(field.into(), json!(unix_seconds(v)?));
    Ok(())
}

/// 64-bit number helper for `unix_seconds`.
fn digits(s: &str) -> Result<i64> {
    if s.is_empty() || !s.bytes().all(|b| b.is_ascii_digit()) {
        bail!("expected digits, got `{s}`");
    }
    s.parse::<i64>().map_err(|e| anyhow::anyhow!("bad number `{s}`: {e}"))
}

/// ISO-8601 date/time -> Unix seconds, without a date-time crate.
fn unix_seconds(input: &str) -> Result<u64> {
    let s = input.trim();
    let (date, rest) = match s.split_once(['T', 't', ' ']) {
        Some((d, r)) => (d, r.trim_start()),
        None => (s, ""),
    };
    let mut dp = date.split('-');
    let (y, mo, d) = (digits(dp.next().unwrap_or(""))?, digits(dp.next().unwrap_or(""))?, digits(dp.next().unwrap_or(""))?);
    if dp.next().is_some() || !(1..=12).contains(&mo) || !(1..=31).contains(&d) {
        bail!("`{input}` is not an ISO-8601 date/time");
    }
    let (clock, offset) = split_zone(rest);
    let mut tp = clock.split(':');
    let h = digits(tp.next().unwrap_or("0").split('.').next().unwrap_or("0"))?;
    let mi = digits(tp.next().unwrap_or("0"))?;
    let sec = digits(tp.next().unwrap_or("0").split('.').next().unwrap_or("0"))?;
    if tp.next().is_some() || h > 23 || mi > 59 || sec > 60 {
        bail!("`{input}` is not an ISO-8601 date/time");
    }
    let secs = days_from_civil(y, mo, d) * 86_400 + h * 3_600 + mi * 60 + sec - offset;
    u64::try_from(secs).map_err(|_| anyhow::anyhow!("`{input}` is before 1970"))
}

/// Split `HH:MM:SS(.fff)?` from `Z` / `+HH:MM` / `-HH:MM`; the returned offset
/// is subtracted (local time is ahead of UTC for a positive offset).
fn split_zone(rest: &str) -> (&str, i64) {
    let rest = rest.trim();
    let clock = rest.strip_suffix(['Z', 'z']).map_or(rest, str::trim_end);
    if clock.is_empty() {
        return (clock, 0);
    }
    let bytes = clock.as_bytes();
    if let Some(i) = clock.find(['+', '-']) {
        if matches!(bytes[i], b'+' | b'-') {
            let sign = if bytes[i] == b'-' { -1 } else { 1 };
            let parts: Vec<&str> = clock[i + 1..].split(':').collect();
            if parts.len() > 2 {
                return (clock, 0);
            }
            let h = digits(parts.first().copied().unwrap_or("")).unwrap_or(0);
            let m = digits(parts.get(1).copied().unwrap_or("0")).unwrap_or(0);
            return (&clock[..i], sign * (h * 3_600 + m * 60));
        }
    }
    (clock, 0)
}

/// Days since 1970-01-01 (Howard Hinnant's `days_from_civil`).
fn days_from_civil(y: i64, m: i64, d: i64) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = if y >= 0 { y } else { y - 399 } / 400;
    let yoe = y - era * 400;
    let mp = (m + 9) % 12;
    let doy = (153 * mp + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

/// `https://api.telegram.org/file/bot<token>/<file_path>` — the download URL
/// `telegram-get-file.ts` / `get-file.action.ts` build from the same base.
fn file_base(api_base: &str) -> String {
    let Some(scheme_end) = api_base.find("://") else { return api_base.into() };
    let host_and_path = &api_base[scheme_end + 3..];
    match host_and_path.find('/') {
        Some(i) => format!(
            "{}{}/file{}",
            &api_base[..scheme_end + 3],
            &host_and_path[..i],
            &host_and_path[i..]
        ),
        None => format!("{api_base}/file"),
    }
}

// ---------------------------------------------------------------- adapter

pub struct Telegram {
    spec: ToolkitSpec,
    api_base: String,
}

impl Default for Telegram {
    fn default() -> Self {
        Self::with_api_base(DEFAULT_API_BASE)
    }
}

#[allow(clippy::too_many_lines)]
impl Telegram {
    /// `api_base` is `telegramCommons.getApiUrl`'s prefix including the token,
    /// e.g. `https://api.telegram.org/bot123456:AAH…`.
    pub fn with_api_base(api_base: &str) -> Self {
        let t = |slug, title, desc, props: Props<'_>, req: &[&str], mutating: bool| {
            tool(SLUG, slug, title, desc, props, req, &[], mutating)
        };
        let spec = ToolkitSpec {
            slug: SLUG.into(),
            name: "Telegram Bot".into(),
            description: LocalizedText::new(
                "Nachrichten, Chats und Mitglieder eines Telegram-Bots lesen und schreiben.",
                "Read and write messages, chats and members of a Telegram bot.",
            ),
            auth_schemes: vec![AuthScheme::ApiKey {
                header_name: "x-telegram-bot-api-token-key".into(),
                prefix: String::new(),
            }],
            tools: vec![
                t(
                    "TELEGRAM_GET_BOT_INFO",
                    ("Bot-Info lesen", "Get bot info"),
                    (
                        "Liest Identität und Fähigkeiten des verbundenen Bots.",
                        "Reads the identity and capabilities of the connected bot.",
                    ),
                    &[],
                    &[],
                    false,
                ),
                t(
                    "TELEGRAM_GET_CHAT",
                    ("Chat lesen", "Get chat"),
                    (
                        "Liest Titel, Typ und Mitgliederzahl eines Chats.",
                        "Reads the title, type and member count of a chat.",
                    ),
                    &[("chat_id", chat_id_prop())],
                    &["chat_id"],
                    false,
                ),
                t(
                    "TELEGRAM_GET_CHAT_MEMBER",
                    ("Chat-Mitglied lesen", "Get chat member"),
                    (
                        "Liest Status und Rechte eines Nutzers in einem Chat.",
                        "Reads the status and rights of one user in a chat.",
                    ),
                    &[
                        ("chat_id", chat_id_prop()),
                        ("user_id", s("User ID des Nutzers, z. B. aus einem empfangenen Update")),
                    ],
                    &["chat_id", "user_id"],
                    false,
                ),
                t(
                    "TELEGRAM_GET_CHAT_MEMBER_COUNT",
                    ("Mitgliederzahl lesen", "Get chat member count"),
                    (
                        "Liest die Anzahl der Mitglieder eines Chats.",
                        "Reads the number of members of a chat.",
                    ),
                    &[("chat_id", chat_id_prop())],
                    &["chat_id"],
                    false,
                ),
                t(
                    "TELEGRAM_GET_CHAT_ADMINISTRATORS",
                    ("Administratoren auflisten", "Get chat administrators"),
                    (
                        "Listet die Administratoren eines Chats mit ihren Rechten.",
                        "Lists the administrators of a chat with their rights.",
                    ),
                    &[("chat_id", chat_id_prop())],
                    &["chat_id"],
                    false,
                ),
                t(
                    "TELEGRAM_GET_FILE",
                    ("Datei-Info lesen", "Get file info"),
                    (
                        "Löst eine file_id auf Metadaten und Download-URL auf.",
                        "Resolves a file_id to its metadata and download URL.",
                    ),
                    &[(
                        "file_id",
                        s("File ID der Datei, z. B. document.file_id einer empfangenen Nachricht"),
                    )],
                    &["file_id"],
                    false,
                ),
                t(
                    "TELEGRAM_SEND_MESSAGE",
                    ("Nachricht senden", "Send message"),
                    (
                        "Sendet eine Textnachricht an einen Chat; braucht --confirm.",
                        "Sends a text message to a chat; requires --confirm.",
                    ),
                    &[
                        ("chat_id", chat_id_prop()),
                        ("message", s("Zu sendender Text")),
                        ("format", choice("Wie Telegram den Text formatiert", FORMATS, "None")),
                        ("message_thread_id", thread_prop()),
                        ("disable_web_page_preview", flag(NO_PREVIEW, false)),
                        ("disable_notification", flag(SILENT, false)),
                        ("protect_content", flag(PROTECT, false)),
                        ("reply_to_message_id", opt_int(REPLY_TO, 1, u64::MAX)),
                        ("reply_markup", markup()),
                    ],
                    &["chat_id", "message"],
                    true,
                ),
                t(
                    "TELEGRAM_EDIT_MESSAGE_TEXT",
                    ("Nachricht ändern", "Edit message text"),
                    (
                        "Ersetzt den Text einer bereits gesendeten Nachricht; braucht --confirm.",
                        "Replaces the text of a message the bot already sent; requires --confirm.",
                    ),
                    &[
                        ("chat_id", s("Chat ID oder @channelusername der Nachricht; zusammen mit Message ID, leer bei Inline-Nachrichten")),
                        ("message_id", s("ID der zu ändernden Nachricht")),
                        ("inline_message_id", s("ID der Inline-Nachricht; alternativ zu Chat ID + Message ID")),
                        ("text", s("Neuer Text (1–4096 Zeichen)")),
                        ("format", choice("Wie Telegram den Text formatiert", FORMATS, "None")),
                        ("disable_web_page_preview", toggle(NO_PREVIEW)),
                        ("reply_markup", markup()),
                    ],
                    &["text"],
                    true,
                ),
                t(
                    "TELEGRAM_DELETE_MESSAGE",
                    ("Nachricht löschen", "Delete message"),
                    (
                        "Löscht eine Nachricht dauerhaft; braucht --confirm.",
                        "Permanently removes a message; requires --confirm.",
                    ),
                    &[
                        ("chat_id", chat_id_prop()),
                        ("message_id", s("ID der zu löschenden Nachricht")),
                    ],
                    &["chat_id", "message_id"],
                    true,
                ),
                t(
                    "TELEGRAM_FORWARD_MESSAGE",
                    ("Nachricht weiterleiten", "Forward message"),
                    (
                        "Leitet eine Nachricht in einen anderen Chat weiter; braucht --confirm.",
                        "Forwards a message into another chat; requires --confirm.",
                    ),
                    &[
                        ("chat_id", s("Ziel-Chat-ID oder @channelusername")),
                        ("from_chat_id", s("Quell-Chat-ID der ursprünglichen Nachricht")),
                        ("message_id", s("ID der weiterzuleitenden Nachricht")),
                        ("message_thread_id", thread_prop()),
                        ("disable_notification", flag(SILENT, false)),
                        ("protect_content", flag(PROTECT, false)),
                    ],
                    &["chat_id", "from_chat_id", "message_id"],
                    true,
                ),
                t(
                    "TELEGRAM_PIN_MESSAGE",
                    ("Nachricht anheften", "Pin message"),
                    (
                        "Haftet eine Nachricht im Chat an; braucht --confirm.",
                        "Pins a message in the chat; requires --confirm.",
                    ),
                    &[
                        ("chat_id", chat_id_prop()),
                        ("message_id", s("ID der anzuheftenden Nachricht")),
                        ("disable_notification", flag(SILENT, false)),
                    ],
                    &["chat_id", "message_id"],
                    true,
                ),
                t(
                    "TELEGRAM_UNPIN_MESSAGE",
                    ("Anheftung aufheben", "Unpin message"),
                    (
                        "Hebt die Anheftung einer Nachricht auf; braucht --confirm.",
                        "Removes the pin from a message; requires --confirm.",
                    ),
                    &[
                        ("chat_id", chat_id_prop()),
                        ("message_id", s("ID der Nachricht; leer = zuletzt angeheftete Nachricht")),
                    ],
                    &["chat_id"],
                    true,
                ),
                t(
                    "TELEGRAM_SEND_CHAT_ACTION",
                    ("Chat-Aktion senden", "Send chat action"),
                    (
                        "Zeigt für bis zu 5 Sekunden einen Status wie „typing“ im Chat.",
                        "Shows a status such as \"typing\" in the chat for up to 5 seconds.",
                    ),
                    &[
                        ("chat_id", chat_id_prop()),
                        ("action", choice("Art des Status", CHAT_ACTIONS, "typing")),
                        ("message_thread_id", thread_prop()),
                    ],
                    &["chat_id", "action"],
                    true,
                ),
                t(
                    "TELEGRAM_SEND_LOCATION",
                    ("Standort senden", "Send location"),
                    (
                        "Sendet einen geoqrten Standort an einen Chat; braucht --confirm.",
                        "Sends a geographic location to a chat; requires --confirm.",
                    ),
                    &[
                        ("chat_id", chat_id_prop()),
                        ("latitude", opt_num("Breitengrad des Standorts.", -90.0, 90.0)),
                        ("longitude", opt_num("Längengrad des Standorts.", -180.0, 180.0)),
                        ("horizontal_accuracy", opt_num("Unsicherheitsradius in Metern (0–1500).", 0.0, 1500.0)),
                        ("live_period", opt_int("Sekunden, die der Standort live aktualisiert wird (60–86400).", 60, 86_400)),
                        ("heading", opt_int("Richtung in Grad für Live-Standorte (1–360).", 1, 360)),
                        ("proximity_alert_radius", opt_int("Distanz für Nähe-Benachrichtigungen in Metern (1–100000).", 1, 100_000)),
                        ("message_thread_id", thread_prop()),
                        ("disable_notification", flag(SILENT, false)),
                        ("protect_content", flag(PROTECT, false)),
                        ("reply_to_message_id", opt_int(REPLY_TO, 1, u64::MAX)),
                        ("reply_markup", markup()),
                    ],
                    &["chat_id", "latitude", "longitude"],
                    true,
                ),
                t(
                    "TELEGRAM_SEND_POLL",
                    ("Umfrage senden", "Send poll"),
                    (
                        "Sendet eine native Telegram-Umfrage (regular oder quiz); braucht --confirm.",
                        "Posts a native Telegram poll (regular or quiz); requires --confirm.",
                    ),
                    &[
                        ("chat_id", chat_id_prop()),
                        ("question", s("Frage der Umfrage, 1–300 Zeichen")),
                        ("options", string_list("Antwortoptionen (2–10 Stück, je 1–100 Zeichen)")),
                        ("is_anonymous", flag("Anonyme Umfrage.", true)),
                        ("type", choice("Umfragetyp", &["regular", "quiz"], "regular")),
                        ("allows_multiple_answers", flag("Mehrfachauswahl (nur bei regular).", false)),
                        ("correct_option_id", opt_int("Index der richtigen Antwort (0-basiert, nur bei quiz).", 0, 9)),
                        ("explanation", s("Erklärung, die bei falscher Antwort erscheint (0–200 Zeichen)")),
                        ("explanation_format", choice("Format der Erklärung", FORMATS, "None")),
                        ("open_period", opt_int("Sekunden, die die Umfrage läuft (5–600).", 5, 600)),
                        ("close_date", datetime("ISO-8601-Zeitpunkt, wann die Umfrage automatisch schließt; nicht zusammen mit Open Period.")),
                        ("is_closed", flag("Umfrage sofort schließen.", false)),
                        ("message_thread_id", thread_prop()),
                        ("disable_notification", flag(SILENT, false)),
                        ("protect_content", flag(PROTECT, false)),
                        ("reply_to_message_id", opt_int(REPLY_TO, 1, u64::MAX)),
                        ("reply_markup", markup()),
                    ],
                    &["chat_id", "question", "options"],
                    true,
                ),
                t(
                    "TELEGRAM_ANSWER_CALLBACK_QUERY",
                    ("Callback beantworten", "Answer callback query"),
                    (
                        "Beantwortet eine Callback-Query eines Inline-Knopfdrucks; braucht --confirm.",
                        "Answers the callback query of an inline keyboard tap; requires --confirm.",
                    ),
                    &[
                        ("callback_query_id", s("ID der Callback-Query, z. B. callback_query.id aus einem empfangenen Update")),
                        ("text", s("Text, der dem Nutzer angezeigt wird; leer zeigt nichts")),
                        ("show_alert", flag("Alert-Dialog statt Benachrichtigung zeigen.", false)),
                        ("url", s("URL, die der Nutzer öffnet")),
                        ("cache_time", opt_int("Sekunden, die das Ergebnis clientseitig gecacht werden darf.", 0, u64::MAX)),
                    ],
                    &["callback_query_id"],
                    true,
                ),
                t(
                    "TELEGRAM_CREATE_INVITE_LINK",
                    ("Einladungslink erstellen", "Create invite link"),
                    (
                        "Erstellt einen neuen Einladungslink für einen Chat; braucht --confirm.",
                        "Creates a new invite link for a chat; requires --confirm.",
                    ),
                    &[
                        ("chat_id", chat_id_prop()),
                        ("name", s("Name des Einladungslinks (max. 32 Zeichen)")),
                        ("expire_date", datetime("ISO-8601-Zeitpunkt, wann der Link abläuft")),
                        ("member_limit", opt_int("Maximale Mitgliederzahl über diesen Link (1–99999).", 1, 99_999)),
                        ("creates_join_request", flag("Beitritt muss von Admins bestätigt werden.", false)),
                    ],
                    &["chat_id"],
                    true,
                ),
            ],
        };
        Self {
            spec,
            api_base: api_base.trim_end_matches('/').to_string(),
        }
    }

    fn url(&self, method: &str) -> String {
        format!("{}/{}", self.api_base, method)
    }
}

/// Bot API method of `tool`, exactly as passed to `telegramCommons.getApiUrl`
/// in the Activepieces source. Never taken from tool arguments.
fn method_of(tool: &str) -> Result<&'static str> {
    Ok(match tool {
        "TELEGRAM_GET_BOT_INFO" => "getMe",
        "TELEGRAM_GET_CHAT" => "getChat",
        "TELEGRAM_GET_CHAT_MEMBER" => "getChatMember",
        "TELEGRAM_GET_CHAT_MEMBER_COUNT" => "getChatMemberCount",
        "TELEGRAM_GET_CHAT_ADMINISTRATORS" => "getChatAdministrators",
        "TELEGRAM_GET_FILE" => "getFile",
        "TELEGRAM_SEND_MESSAGE" => "sendMessage",
        "TELEGRAM_EDIT_MESSAGE_TEXT" => "editMessageText",
        "TELEGRAM_DELETE_MESSAGE" => "deleteMessage",
        "TELEGRAM_FORWARD_MESSAGE" => "forwardMessage",
        "TELEGRAM_PIN_MESSAGE" => "pinChatMessage",
        "TELEGRAM_UNPIN_MESSAGE" => "unpinChatMessage",
        "TELEGRAM_SEND_CHAT_ACTION" => "sendChatAction",
        "TELEGRAM_SEND_LOCATION" => "sendLocation",
        "TELEGRAM_SEND_POLL" => "sendPoll",
        "TELEGRAM_ANSWER_CALLBACK_QUERY" => "answerCallbackQuery",
        "TELEGRAM_CREATE_INVITE_LINK" => "createChatInviteLink",
        other => bail!("unknown Telegram tool `{other}`"),
    })
}

impl Telegram {
    #[allow(clippy::too_many_lines)]
    fn body(&self, tool: &str, args: &Value) -> Result<Option<Value>> {
        let mut m = Map::new();
        match tool {
            // AP: lib/action/telegram-get-bot-info.ts — getMe, no body
            "TELEGRAM_GET_BOT_INFO" => {}
            // AP: lib/action/telegram-get-chat.ts — getChat {chat_id}
            "TELEGRAM_GET_CHAT" => {
                m.insert("chat_id".into(), json!(req_str(args, "chat_id")?));
            }
            // AP: lib/action/telegram-get-chat-member.ts — getChatMember {chat_id, user_id}
            "TELEGRAM_GET_CHAT_MEMBER" => {
                m.insert("chat_id".into(), json!(req_str(args, "chat_id")?));
                m.insert("user_id".into(), json!(req_str(args, "user_id")?));
            }
            // AP: lib/action/telegram-get-chat-member-count.ts — getChatMemberCount {chat_id}
            "TELEGRAM_GET_CHAT_MEMBER_COUNT" => {
                m.insert("chat_id".into(), json!(req_str(args, "chat_id")?));
            }
            // AP: lib/action/telegram-get-chat-administrators.ts — getChatAdministrators {chat_id}
            "TELEGRAM_GET_CHAT_ADMINISTRATORS" => {
                m.insert("chat_id".into(), json!(req_str(args, "chat_id")?));
            }
            // AP: lib/action/telegram-get-file.ts — getFile {file_id}
            "TELEGRAM_GET_FILE" => {
                m.insert("file_id".into(), json!(req_str(args, "file_id")?));
            }
            // AP: lib/action/telegram-send-message.ts — sendMessage
            "TELEGRAM_SEND_MESSAGE" => {
                m.insert("chat_id".into(), json!(req_str(args, "chat_id")?));
                m.insert("text".into(), json!(req_str(args, "message")?));
                put_parse_mode(&mut m, args, "format", "parse_mode")?;
                put_str(&mut m, args, "message_thread_id", "message_thread_id");
                put_flag(&mut m, args, "disable_web_page_preview", "disable_web_page_preview", false);
                put_flag(&mut m, args, "disable_notification", "disable_notification", false);
                put_flag(&mut m, args, "protect_content", "protect_content", false);
                put_num(&mut m, args, "reply_to_message_id", "reply_to_message_id")?;
                put_markup(&mut m, args, "reply_markup")?;
            }
            // AP: lib/action/telegram-edit-message-text.ts — editMessageText
            "TELEGRAM_EDIT_MESSAGE_TEXT" => {
                let chat = opt_str(args, "chat_id");
                let message = opt_str(args, "message_id");
                let inline = opt_str(args, "inline_message_id");
                if chat.is_none() != message.is_none() {
                    bail!("`chat_id` and `message_id` belong together");
                }
                if (chat.is_none() || message.is_none()) && inline.is_none() {
                    bail!("either `chat_id` + `message_id`, or `inline_message_id` must be given");
                }
                if inline.is_some() && chat.is_some() {
                    bail!("use either `inline_message_id` or `chat_id` + `message_id`, not both");
                }
                if let Some(v) = chat {
                    m.insert("chat_id".into(), json!(v));
                }
                if let Some(v) = message {
                    m.insert("message_id".into(), json!(v));
                }
                if let Some(v) = inline {
                    m.insert("inline_message_id".into(), json!(v));
                }
                m.insert("text".into(), json!(req_str(args, "text")?));
                put_parse_mode(&mut m, args, "format", "parse_mode")?;
                put_flag(&mut m, args, "disable_web_page_preview", "disable_web_page_preview", false);
                put_markup(&mut m, args, "reply_markup")?;
            }
            // AP: lib/action/telegram-delete-message.ts — deleteMessage {chat_id, message_id}
            "TELEGRAM_DELETE_MESSAGE" => {
                m.insert("chat_id".into(), json!(req_str(args, "chat_id")?));
                m.insert("message_id".into(), json!(req_str(args, "message_id")?));
            }
            // AP: lib/action/telegram-forward-message.ts — forwardMessage
            "TELEGRAM_FORWARD_MESSAGE" => {
                m.insert("chat_id".into(), json!(req_str(args, "chat_id")?));
                m.insert("from_chat_id".into(), json!(req_str(args, "from_chat_id")?));
                m.insert("message_id".into(), json!(req_str(args, "message_id")?));
                put_str(&mut m, args, "message_thread_id", "message_thread_id");
                put_flag(&mut m, args, "disable_notification", "disable_notification", false);
                put_flag(&mut m, args, "protect_content", "protect_content", false);
            }
            // AP: lib/action/telegram-pin-message.ts — pinChatMessage
            "TELEGRAM_PIN_MESSAGE" => {
                m.insert("chat_id".into(), json!(req_str(args, "chat_id")?));
                m.insert("message_id".into(), json!(req_str(args, "message_id")?));
                put_flag(&mut m, args, "disable_notification", "disable_notification", false);
            }
            // AP: lib/action/telegram-unpin-message.ts — unpinChatMessage {chat_id, message_id?}
            "TELEGRAM_UNPIN_MESSAGE" => {
                m.insert("chat_id".into(), json!(req_str(args, "chat_id")?));
                put_str(&mut m, args, "message_id", "message_id");
            }
            // AP: lib/action/telegram-send-chat-action.ts — sendChatAction
            "TELEGRAM_SEND_CHAT_ACTION" => {
                m.insert("chat_id".into(), json!(req_str(args, "chat_id")?));
                let action = req_str(args, "action")?;
                if !CHAT_ACTIONS.contains(&action) {
                    bail!("`action` must be one of {}", CHAT_ACTIONS.join(", "));
                }
                m.insert("action".into(), json!(action));
                put_str(&mut m, args, "message_thread_id", "message_thread_id");
            }
            // AP: lib/action/telegram-send-location.ts — sendLocation
            "TELEGRAM_SEND_LOCATION" => {
                m.insert("chat_id".into(), json!(req_str(args, "chat_id")?));
                for (key, field) in [
                    ("latitude", "latitude"),
                    ("longitude", "longitude"),
                    ("horizontal_accuracy", "horizontal_accuracy"),
                    ("live_period", "live_period"),
                    ("heading", "heading"),
                    ("proximity_alert_radius", "proximity_alert_radius"),
                ] {
                    put_num(&mut m, args, key, field)?;
                }
                put_str(&mut m, args, "message_thread_id", "message_thread_id");
                put_flag(&mut m, args, "disable_notification", "disable_notification", false);
                put_flag(&mut m, args, "protect_content", "protect_content", false);
                put_num(&mut m, args, "reply_to_message_id", "reply_to_message_id")?;
                put_markup(&mut m, args, "reply_markup")?;
            }
            // AP: lib/action/telegram-send-poll.ts — sendPoll (incl. its quiz validation)
            "TELEGRAM_SEND_POLL" => {
                m.insert("chat_id".into(), json!(req_str(args, "chat_id")?));
                m.insert("question".into(), json!(req_str(args, "question")?));
                let options = str_vec(args, "options");
                if options.len() < 2 || options.len() > 10 {
                    bail!("a poll requires between 2 and 10 options, got {}", options.len());
                }
                m.insert("options".into(), json!(options));
                put_flag(&mut m, args, "is_anonymous", "is_anonymous", true);
                let poll_type = opt_str(args, "type").unwrap_or("regular");
                if poll_type != "regular" && poll_type != "quiz" {
                    bail!("`type` must be regular or quiz");
                }
                m.insert("type".into(), json!(poll_type));
                let multiple = args.get("allows_multiple_answers").and_then(Value::as_bool).unwrap_or(false);
                put_flag(&mut m, args, "allows_multiple_answers", "allows_multiple_answers", false);
                let correct = args.get("correct_option_id").filter(|v| !v.is_null());
                if poll_type == "quiz" {
                    let Some(idx) = correct.and_then(Value::as_u64) else {
                        bail!("quiz polls require `correct_option_id`");
                    };
                    if idx as usize >= options.len() {
                        bail!("`correct_option_id` must be between 0 and {}", options.len() - 1);
                    }
                    if multiple {
                        bail!("quiz polls cannot allow multiple answers");
                    }
                } else if correct.is_some() {
                    bail!("`correct_option_id` is only valid for quiz polls; set `type` to quiz or clear the field");
                }
                if let Some(v) = correct {
                    m.insert("correct_option_id".into(), v.clone());
                }
                put_str(&mut m, args, "explanation", "explanation");
                put_parse_mode(&mut m, args, "explanation_format", "explanation_parse_mode")?;
                put_num(&mut m, args, "open_period", "open_period")?;
                put_unix_time(&mut m, args, "close_date", "close_date")?;
                if m.get("open_period").is_some() && m.get("close_date").is_some() {
                    bail!("`open_period` and `close_date` are mutually exclusive — set at most one");
                }
                put_flag(&mut m, args, "is_closed", "is_closed", false);
                put_str(&mut m, args, "message_thread_id", "message_thread_id");
                put_flag(&mut m, args, "disable_notification", "disable_notification", false);
                put_flag(&mut m, args, "protect_content", "protect_content", false);
                put_num(&mut m, args, "reply_to_message_id", "reply_to_message_id")?;
                put_markup(&mut m, args, "reply_markup")?;
            }
            // AP: lib/action/telegram-answer-callback-query.ts — answerCallbackQuery
            "TELEGRAM_ANSWER_CALLBACK_QUERY" => {
                m.insert(
                    "callback_query_id".into(),
                    json!(req_str(args, "callback_query_id")?),
                );
                put_str(&mut m, args, "text", "text");
                put_flag(&mut m, args, "show_alert", "show_alert", false);
                put_str(&mut m, args, "url", "url");
                put_num(&mut m, args, "cache_time", "cache_time")?;
            }
            // AP: lib/action/telegram-create-invite-link.ts — createChatInviteLink
            "TELEGRAM_CREATE_INVITE_LINK" => {
                m.insert("chat_id".into(), json!(req_str(args, "chat_id")?));
                put_str(&mut m, args, "name", "name");
                put_unix_time(&mut m, args, "expire_date", "expire_date")?;
                put_num(&mut m, args, "member_limit", "member_limit")?;
                put_flag(&mut m, args, "creates_join_request", "creates_join_request", false);
            }
            other => bail!("unknown Telegram tool `{other}`"),
        }
        Ok((!m.is_empty()).then_some(Value::Object(m)))
    }
}

impl ToolkitAdapter for Telegram {
    fn spec(&self) -> &ToolkitSpec {
        &self.spec
    }

    fn build_request(&self, tool: &str, args: &Value) -> Result<HttpRequest> {
        // AP: lib/common/index.ts — telegramCommons.getApiUrl(auth, method)
        let url = self.url(method_of(tool)?);
        let request = HttpRequest::new("POST", url);
        Ok(match self.body(tool, args)? {
            Some(body) => request.json(&body),
            None => request,
        })
    }

    fn parse_response(&self, tool: &str, response: &HttpResponse) -> Result<Value> {
        let _ = tool;
        let v = response.json()?;
        if v.get("ok").and_then(Value::as_bool) != Some(true) {
            let description = v["description"].as_str().unwrap_or("no description").to_string();
            match v["error_code"].as_i64() {
                Some(code) => bail!("Telegram API error {code}: {description}"),
                None => bail!("Telegram returned an unexpected answer: {description}"),
            };
        }
        let Some(result) = v.get("result").cloned() else {
            bail!("Telegram returned no result");
        };
        // AP: the Activepieces actions return the whole `{ok, result}` body.
        let mut out = v;
        if let Some(items) = result.as_array() {
            out["count"] = json!(items.len());
        }
        // AP: lib/action/telegram-get-file.ts — https://api.telegram.org/file/bot<token>/<file_path>
        if let Some(path) = result["file_path"].as_str() {
            if path
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'/' | b'_' | b'-' | b'.'))
            {
                out["file_url"] = json!(format!("{}/{path}", file_base(&self.api_base)));
            }
        }
        Ok(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn bot() -> Telegram {
        Telegram::with_api_base("https://telegram.test/bot700000:AAHtesttoken")
    }

    /// JSON body of `tool`, for assertions on fields the request test shares.
    fn body_of(tool: &str, args: Value) -> Value {
        let t = bot();
        let r = t.build_request(tool, &args).unwrap();
        serde_json::from_slice(r.body.as_deref().expect("body")).unwrap()
    }

    #[test]
    fn urls_use_the_piece_api_url() {
        let t = bot();
        let r = t.build_request("TELEGRAM_GET_BOT_INFO", &json!({})).unwrap();
        assert_eq!(r.method, "POST");
        assert_eq!(r.url, "https://telegram.test/bot700000:AAHtesttoken/getMe");
        assert!(r.body.is_none(), "getMe is sent without a body");

        let r = t
            .build_request("TELEGRAM_GET_CHAT", &json!({"chat_id": "@channelname"}))
            .unwrap();
        assert_eq!(r.url, "https://telegram.test/bot700000:AAHtesttoken/getChat");
        assert_eq!(file_base(&t.api_base), "https://telegram.test/file/bot700000:AAHtesttoken");
        assert!(t.build_request("TELEGRAM_NOPE", &json!({})).is_err());
    }

    #[test]
    fn send_message_maps_every_field() {
        let t = bot();
        let r = t
            .build_request(
                "TELEGRAM_SEND_MESSAGE",
                &json!({"chat_id": "-1001234", "message": "Hello", "format": "HTML",
                        "message_thread_id": "42", "disable_notification": true,
                        "reply_to_message_id": 7,
                        "reply_markup": {"inline_keyboard": [[{"text": "Open", "url": "https://example.com"}]]}}),
            )
            .unwrap();
        assert_eq!(r.url, "https://telegram.test/bot700000:AAHtesttoken/sendMessage");
        let b: Value = serde_json::from_slice(r.body.as_deref().unwrap()).unwrap();
        assert_eq!(b["chat_id"], "-1001234");
        assert_eq!(b["text"], "Hello");
        assert_eq!(b["parse_mode"], "HTML");
        assert_eq!(b["message_thread_id"], "42");
        assert_eq!(b["disable_notification"], true);
        assert_eq!(b["disable_web_page_preview"], false);
        assert_eq!(b["protect_content"], false);
        assert_eq!(b["reply_to_message_id"], 7);
        assert_eq!(b["reply_markup"]["inline_keyboard"][0][0]["text"], "Open");

        // `None` means "no parse_mode at all".
        let b = body_of(
            "TELEGRAM_SEND_MESSAGE",
            json!({"chat_id": "1", "message": "Hi", "format": "None"}),
        );
        assert!(b.get("parse_mode").is_none());
    }

    #[test]
    fn edit_message_target_is_checked() {
        let t = bot();
        assert!(t
            .build_request("TELEGRAM_EDIT_MESSAGE_TEXT", &json!({"text": "x"}))
            .is_err());
        assert!(t
            .build_request("TELEGRAM_EDIT_MESSAGE_TEXT", &json!({"text": "x", "chat_id": "1"}))
            .is_err());
        assert!(t
            .build_request(
                "TELEGRAM_EDIT_MESSAGE_TEXT",
                &json!({"text": "x", "inline_message_id": "i1", "chat_id": "1", "message_id": "2"})
            )
            .is_err());
        let r = t
            .build_request(
                "TELEGRAM_EDIT_MESSAGE_TEXT",
                &json!({"text": "x", "chat_id": "1", "message_id": "2"}),
            )
            .unwrap();
        let b: Value = serde_json::from_slice(r.body.as_deref().unwrap()).unwrap();
        assert_eq!(b["chat_id"], "1");
        assert_eq!(b["message_id"], "2");
        assert!(b.get("inline_message_id").is_none());
    }

    #[test]
    fn poll_validation_matches_the_piece() {
        let t = bot();
        let ok = |args: Value| t.build_request("TELEGRAM_SEND_POLL", &args);
        assert!(ok(json!({"chat_id": "1", "question": "Q?", "options": ["a"]})).is_err());
        assert!(ok(json!({"chat_id": "1", "question": "Q?", "options": ["a", "b", "c"],
                           "type": "quiz"})).is_err());
        assert!(ok(json!({"chat_id": "1", "question": "Q?", "options": ["a", "b"],
                           "type": "quiz", "correct_option_id": 2})).is_err());
        assert!(ok(json!({"chat_id": "1", "question": "Q?", "options": ["a", "b"],
                           "type": "quiz", "correct_option_id": 0, "allows_multiple_answers": true})).is_err());
        assert!(ok(json!({"chat_id": "1", "question": "Q?", "options": ["a", "b"],
                           "correct_option_id": 0})).is_err());
        assert!(ok(json!({"chat_id": "1", "question": "Q?", "options": ["a", "b"],
                           "open_period": 60, "close_date": "2026-10-14T12:00:00Z"})).is_err());
        let r = ok(json!({"chat_id": "1", "question": "Q?", "options": ["a", "b"],
                           "type": "quiz", "correct_option_id": 1,
                           "close_date": "2026-10-14T12:00:00Z",
                           "explanation_format": "MarkdownV2", "explanation": "why"})).unwrap();
        assert_eq!(r.url, "https://telegram.test/bot700000:AAHtesttoken/sendPoll");
        let b: Value = serde_json::from_slice(r.body.as_deref().unwrap()).unwrap();
        assert_eq!(b["options"], json!(["a", "b"]));
        assert_eq!(b["type"], "quiz");
        assert_eq!(b["correct_option_id"], 1);
        assert_eq!(b["is_anonymous"], true);
        assert_eq!(b["allows_multiple_answers"], false);
        assert_eq!(b["explanation_parse_mode"], "MarkdownV2");
        assert_eq!(b["close_date"], json!(1_791_979_200));
    }

    #[test]
    fn iso_dates_become_unix_seconds() {
        assert_eq!(unix_seconds("1970-01-01T00:00:00Z").unwrap(), 0);
        assert_eq!(unix_seconds("2026-10-14T12:00:00Z").unwrap(), 1_791_979_200);
        assert_eq!(unix_seconds("2026-10-14T14:00:00+02:00").unwrap(), 1_791_979_200);
        assert_eq!(unix_seconds("2026-10-14T10:00:00-02:00").unwrap(), 1_791_979_200);
        assert_eq!(unix_seconds("2026-10-14 12:00:00").unwrap(), 1_791_979_200);
        assert_eq!(unix_seconds("2026-10-14T12:00:00.250Z").unwrap(), 1_791_979_200);
        assert!(unix_seconds("14.10.2026").is_err());
        assert!(unix_seconds("2026-13-14T00:00:00Z").is_err());
        assert!(unix_seconds("2026-10-14T99:00:00Z").is_err());
    }

    #[test]
    fn provider_errors_surface_as_tool_errors() {
        let t = bot();
        let resp = HttpResponse {
            status: 400,
            body: serde_json::to_vec(&json!({
                "ok": false, "error_code": 400, "description": "Bad Request: chat not found"
            }))
            .unwrap(),
            session_id: None,
        };
        let err = t
            .parse_response("TELEGRAM_GET_CHAT", &resp)
            .unwrap_err()
            .to_string();
        assert!(err.contains("chat not found"), "{err}");
    }

    #[test]
    fn file_metadata_gains_a_download_url() {
        let t = bot();
        let resp = HttpResponse {
            status: 200,
            body: serde_json::to_vec(&json!({"ok": true, "result": {
                "file_id": "f1", "file_unique_id": "u1", "file_size": 4,
                "file_path": "documents/file_7.pdf"}}))
            .unwrap(),
            session_id: None,
        };
        let out = t.parse_response("TELEGRAM_GET_FILE", &resp).unwrap();
        assert_eq!(out["ok"], true);
        assert_eq!(out["result"]["file_id"], "f1");
        assert_eq!(
            out["file_url"],
            "https://telegram.test/file/bot700000:AAHtesttoken/documents/file_7.pdf"
        );
    }
}