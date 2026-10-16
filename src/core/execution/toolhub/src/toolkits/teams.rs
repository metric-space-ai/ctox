//! Microsoft Teams toolkit (Microsoft Graph v1.0).
//! Action set modelled on Activepieces `piece-microsoft-teams` (MIT).
//!
//! The Activepieces piece drives the Graph SDK; every action maps onto one plain
//! Graph REST call here. Three TS behaviours are deliberately not ported:
//!
//! - `paginateGraphList` / `withGraphRetry` — pagination and throttling retries
//!   belong to the hub's HTTP layer, not to `build_request`.
//! - `request_approval_*` used an Activepieces *waitpoint* resume URL; here the
//!   caller supplies `confirmation_url` and the tool only posts the adaptive card.
//! - Mentions in `send-channel-message` were resolved with a member lookup first;
//!   the resolved Graph `mentions` array is accepted as an argument instead.
use anyhow::{bail, Result};
use serde_json::{json, Value};

use super::common::*;
use super::{path_segment, ToolkitAdapter};
use crate::http::{HttpRequest, HttpResponse};
use crate::model::*;

pub const SLUG: &str = "microsoft_teams";

const USER_READ: &str = "User.Read";
const USER_READ_BASIC: &str = "User.ReadBasic.All";
const CHANNEL_CREATE: &str = "Channel.Create";
const CHANNEL_READ: &str = "Channel.ReadBasic.All";
const CHANNEL_MSG_SEND: &str = "ChannelMessage.Send";
const CHANNEL_MSG_READ: &str = "ChannelMessage.Read.All";
const TEAM_READ: &str = "Team.ReadBasic.All";
const TEAM_MEMBER_READ: &str = "TeamMember.Read.All";
const CHAT_RW: &str = "Chat.ReadWrite";
const PRESENCE_READ: &str = "Presence.Read.All";
const TRANSCRIPT_READ: &str = "OnlineMeetingTranscript.Read.All";
const RECORDING_READ: &str = "OnlineMeetingRecording.Read.All";

pub struct Teams {
    spec: ToolkitSpec,
    api_base: String,
}

impl Default for Teams {
    fn default() -> Self {
        Self::with_api_base("https://graph.microsoft.com")
    }
}

/// Graph identifiers for channels, chats and messages (`19:general@thread.skype`,
/// base64url-ish message ids) are wider than plain GUIDs, so they get their own
/// guard: everything outside `A-Z a-z 0-9 - _ . ~ = + $ ! : @` is rejected, which
/// rules out `/`, `?`, `#` and whitespace and therefore any escape from the path
/// segment. Team and user ids are plain GUIDs and use `path_segment`.
fn graph_id<'a>(args: &'a Value, key: &str) -> Result<&'a str> {
    let v = req_str(args, key)?;
    if v.len() > 200
        || v == "."
        || v == ".."
        || !v
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.' | b'~' | b'=' | b'+' | b'$' | b'!' | b':' | b'@'))
    {
        bail!("invalid argument `{key}`");
    }
    Ok(v)
}

fn enum_prop(desc: &str, options: &[&str], default: &str) -> Value {
    json!({"type": "string", "description": desc, "enum": options, "default": default})
}

fn obj_list(desc: &str) -> Value {
    json!({"type": "array", "description": desc, "items": {"type": "object"}})
}

impl Teams {
    pub fn with_api_base(api_base: &str) -> Self {
        let api_base = api_base.trim_end_matches('/').to_string();

        let t = |slug, title, desc: (&str, &str), props: Props<'_>, req: &[&str], scopes: &[&str], m| {
            tool(SLUG, slug, title, desc, props, req, scopes, m)
        };

        let spec = ToolkitSpec {
            slug: SLUG.into(),
            name: "Microsoft Teams".into(),
            description: LocalizedText::new(
                "Teams, Kanäle, Chats, Nachrichten und Besprechungsaufzeichnungen über Microsoft Graph lesen und schreiben.",
                "Read and write Teams, channels, chats, messages and meeting recordings via Microsoft Graph.",
            ),
            auth_schemes: vec![
                AuthScheme::OAuth2(OAuth2Scheme {
                    // The TS piece templates both URLs with the connection's
                    // `cloud` and `tenantId` props; the catalog entry below is
                    // the commercial multi-tenant default (`tenantId=common`).
                    // GCC High uses login.microsoftonline.us / graph.microsoft.us.
                    authorize_url: "https://login.microsoftonline.com/common/oauth2/v2.0/authorize".into(),
                    token_url: "https://login.microsoftonline.com/common/oauth2/v2.0/token".into(),
                    default_scopes: [
                        "openid",
                        "email",
                        "profile",
                        "offline_access",
                        USER_READ,
                        CHANNEL_CREATE,
                        CHANNEL_READ,
                        CHANNEL_MSG_SEND,
                        TEAM_READ,
                        CHAT_RW,
                        CHANNEL_MSG_READ,
                        TEAM_MEMBER_READ,
                        USER_READ_BASIC,
                        PRESENCE_READ,
                        TRANSCRIPT_READ,
                        RECORDING_READ,
                    ]
                    .iter()
                    .map(|s| (*s).to_string())
                    .collect(),
                    pkce: false,
                    scope_separator: " ".into(),
                    refreshable: true,
                    extra_authorize_params: vec![],
                }),
                AuthScheme::Bearer,
            ],
            tools: vec![
                t(
                    "TEAMS_GET_ME",
                    ("Profil abrufen", "Get current user"),
                    (
                        "Liest das angemeldete Benutzerprofil (`/me`).",
                        "Reads the signed-in user profile (`/me`).",
                    ),
                    &[],
                    &[],
                    &[USER_READ, USER_READ_BASIC],
                    false,
                ),
                t(
                    "TEAMS_LIST_MY_TEAMS",
                    ("Meine Teams auflisten", "List my teams"),
                    (
                        "Listet die Teams, in denen der Benutzer Mitglied ist.",
                        "Lists the teams the signed-in user is a member of.",
                    ),
                    &[],
                    &[],
                    &[TEAM_READ],
                    false,
                ),
                t(
                    "TEAMS_LIST_CHANNELS",
                    ("Kanäle auflisten", "List team channels"),
                    (
                        "Listet die Standardkanäle eines Teams.",
                        "Lists the standard channels of a team.",
                    ),
                    &[("team_id", s("Team ID"))],
                    &["team_id"],
                    &[CHANNEL_READ],
                    false,
                ),
                t(
                    "TEAMS_LIST_ALL_CHANNELS",
                    ("Alle Kanäle auflisten", "List all channels"),
                    (
                        "Listet alle Kanäle eines Teams inkl. privater Kanäle.",
                        "Lists all channels of a team, including private ones.",
                    ),
                    &[("team_id", s("Team ID"))],
                    &["team_id"],
                    &[CHANNEL_READ],
                    false,
                ),
                t(
                    "TEAMS_LIST_TEAM_MEMBERS",
                    ("Teammitglieder auflisten", "List team members"),
                    (
                        "Listet die Mitglieder eines Teams.",
                        "Lists the members of a team.",
                    ),
                    &[("team_id", s("Team ID"))],
                    &["team_id"],
                    &[TEAM_MEMBER_READ],
                    false,
                ),
                t(
                    "TEAMS_FIND_CHANNEL",
                    ("Kanal suchen", "Find channel"),
                    (
                        "Findet Kanäle eines Teams nach exaktem Anzeigenamen.",
                        "Finds channels of a team by exact display name.",
                    ),
                    &[
                        ("team_id", s("Team ID")),
                        ("channel_name", s("Exakter Kanalname / exact channel name")),
                    ],
                    &["team_id", "channel_name"],
                    &[CHANNEL_READ],
                    false,
                ),
                t(
                    "TEAMS_FIND_TEAM_MEMBER",
                    ("Teammitglied suchen", "Find team member"),
                    (
                        "Findet ein Teammitglied nach E-Mail oder Anzeigename.",
                        "Finds a team member by email or display name.",
                    ),
                    &[
                        ("team_id", s("Team ID")),
                        ("search_by", enum_prop("email | name", &["email", "name"], "email")),
                        ("search_value", s("E-Mail oder Name / email or name")),
                    ],
                    &["team_id", "search_by", "search_value"],
                    &[TEAM_MEMBER_READ],
                    false,
                ),
                t(
                    "TEAMS_LIST_CHANNEL_MESSAGES",
                    ("Kanalnachrichten auflisten", "List channel messages"),
                    (
                        "Listet die Nachrichten eines Kanals.",
                        "Lists the messages of a channel.",
                    ),
                    &[
                        ("team_id", s("Team ID")),
                        ("channel_id", s("Channel ID")),
                        ("top", int("1-50", 1, 50, 20)),
                    ],
                    &["team_id", "channel_id"],
                    &[CHANNEL_MSG_READ],
                    false,
                ),
                t(
                    "TEAMS_GET_CHANNEL_MESSAGE",
                    ("Kanalnachricht lesen", "Get channel message"),
                    (
                        "Liest eine Kanalnachricht oder eine einzelne Antwort darauf.",
                        "Reads a channel message or one of its replies.",
                    ),
                    &[
                        ("team_id", s("Team ID")),
                        ("channel_id", s("Channel ID")),
                        ("message_id", s("Message ID")),
                        ("reply_id", s("Optional: ID der Antwort / optional reply ID")),
                    ],
                    &["team_id", "channel_id", "message_id"],
                    &[CHANNEL_MSG_READ],
                    false,
                ),
                t(
                    "TEAMS_LIST_CHAT_MESSAGES",
                    ("Chatnachrichten auflisten", "List chat messages"),
                    (
                        "Listet die Nachrichten eines Chats.",
                        "Lists the messages of a chat.",
                    ),
                    &[
                        ("chat_id", s("Chat ID")),
                        ("top", int("1-50", 1, 50, 20)),
                    ],
                    &["chat_id"],
                    &[CHAT_RW],
                    false,
                ),
                t(
                    "TEAMS_GET_CHAT_MESSAGE",
                    ("Chatnachricht lesen", "Get chat message"),
                    (
                        "Liest eine einzelne Nachricht eines Chats.",
                        "Reads a single message of a chat.",
                    ),
                    &[
                        ("chat_id", s("Chat ID")),
                        ("message_id", s("Message ID")),
                    ],
                    &["chat_id", "message_id"],
                    &[CHAT_RW],
                    false,
                ),
                t(
                    "TEAMS_LIST_CHATS",
                    ("Chats auflisten", "List chats"),
                    (
                        "Listet die Chats des Benutzers inkl. Mitgliedern.",
                        "Lists the signed-in user's chats including members.",
                    ),
                    &[("top", int("1-100 (Default 50)", 1, 100, 50))],
                    &[],
                    &[CHAT_RW],
                    false,
                ),
                t(
                    "TEAMS_FIND_MEETING",
                    ("Besprechung finden", "Find online meeting"),
                    (
                        "Löst eine Besprechungs-URL oder Join-ID zur Meeting-ID auf.",
                        "Resolves a join web URL or join meeting ID to a meeting ID.",
                    ),
                    &[
                        ("identifier_type", enum_prop("joinWebUrl | joinMeetingId", &["joinWebUrl", "joinMeetingId"], "joinWebUrl")),
                        ("identifier_value", s("Join-Web-URL oder Join-ID / join web URL or join ID")),
                    ],
                    &["identifier_type", "identifier_value"],
                    &[USER_READ],
                    false,
                ),
                t(
                    "TEAMS_LIST_MEETING_RECORDINGS",
                    ("Besprechungsaufzeichnungen auflisten", "List meeting recordings"),
                    (
                        "Listet alle Aufzeichnungen einer Online-Besprechung.",
                        "Lists all recordings of an online meeting.",
                    ),
                    &[
                        ("meeting_id", s("Meeting ID (TEAMS_FIND_MEETING)")),
                        ("top", int("1-50", 1, 50, 20)),
                    ],
                    &["meeting_id"],
                    &[RECORDING_READ],
                    false,
                ),
                t(
                    "TEAMS_GET_MEETING_RECORDING",
                    ("Aufzeichnung abrufen", "Get meeting recording"),
                    (
                        "Liest die Metadaten einer einzelnen Aufzeichnung.",
                        "Reads the metadata of a single recording.",
                    ),
                    &[
                        ("meeting_id", s("Meeting ID (TEAMS_FIND_MEETING)")),
                        ("recording_id", s("Recording ID")),
                    ],
                    &["meeting_id", "recording_id"],
                    &[RECORDING_READ],
                    false,
                ),
                t(
                    "TEAMS_LIST_MEETING_TRANSCRIPTS",
                    ("Besprechungstranskripte auflisten", "List meeting transcripts"),
                    (
                        "Listet alle Transkripte einer Online-Besprechung.",
                        "Lists all transcripts of an online meeting.",
                    ),
                    &[
                        ("meeting_id", s("Meeting ID (TEAMS_FIND_MEETING)")),
                        ("top", int("1-50", 1, 50, 20)),
                    ],
                    &["meeting_id"],
                    &[TRANSCRIPT_READ],
                    false,
                ),
                t(
                    "TEAMS_GET_MEETING_TRANSCRIPT",
                    ("Transkriptinhalt abrufen", "Get meeting transcript content"),
                    (
                        "Lädt den VTT-Inhalt eines Transkripts herunter.",
                        "Downloads the VTT content of a transcript.",
                    ),
                    &[
                        ("meeting_id", s("Meeting ID (TEAMS_FIND_MEETING)")),
                        ("transcript_id", s("Transcript ID")),
                    ],
                    &["meeting_id", "transcript_id"],
                    &[TRANSCRIPT_READ],
                    false,
                ),
                t(
                    "TEAMS_CREATE_CHANNEL",
                    ("Kanal erstellen", "Create channel"),
                    (
                        "Legt einen Standardkanal in einem Team an.",
                        "Creates a standard channel in a team.",
                    ),
                    &[
                        ("team_id", s("Team ID")),
                        ("channel_display_name", s("Kanalname / channel name")),
                        ("channel_description", s("Optionale Beschreibung / optional description")),
                    ],
                    &["team_id", "channel_display_name"],
                    &[CHANNEL_CREATE],
                    true,
                ),
                t(
                    "TEAMS_CREATE_PRIVATE_CHANNEL",
                    ("Privaten Kanal erstellen", "Create private channel"),
                    (
                        "Legt einen privaten (mitgliederbeschränkten) Kanal an.",
                        "Creates a private (membership-restricted) channel.",
                    ),
                    &[
                        ("team_id", s("Team ID")),
                        ("channel_display_name", s("Kanalname / channel name")),
                        ("channel_description", s("Optionale Beschreibung / optional description")),
                    ],
                    &["team_id", "channel_display_name"],
                    &[CHANNEL_CREATE],
                    true,
                ),
                t(
                    "TEAMS_SEND_CHANNEL_MESSAGE",
                    ("Kanalnachricht senden", "Send channel message"),
                    (
                        "Postet eine Nachricht in einen Kanal.",
                        "Posts a message to a channel.",
                    ),
                    &[
                        ("team_id", s("Team ID")),
                        ("channel_id", s("Channel ID")),
                        ("content", s("Nachrichtentext / message body")),
                        (
                            "content_type",
                            enum_prop("text | html (Activepieces postet immer html)", &["text", "html"], "html"),
                        ),
                        ("mentions", obj_list("Optionale Graph-`mentions` (z. B. aus TEAMS_LIST_TEAM_MEMBERS)")),
                    ],
                    &["team_id", "channel_id", "content"],
                    &[CHANNEL_MSG_SEND],
                    true,
                ),
                t(
                    "TEAMS_REPLY_TO_CHANNEL_MESSAGE",
                    ("Auf Kanalnachricht antworten", "Reply to channel message"),
                    (
                        "Postet eine Antwort auf eine bestehende Kanalnachricht.",
                        "Posts a reply under an existing channel message.",
                    ),
                    &[
                        ("team_id", s("Team ID")),
                        ("channel_id", s("Channel ID")),
                        ("message_id", s("ID der Elternnachricht / parent message ID")),
                        ("content", s("Antworttext / reply body")),
                        ("content_type", enum_prop("text | html", &["text", "html"], "text")),
                    ],
                    &["team_id", "channel_id", "message_id", "content"],
                    &[CHANNEL_MSG_SEND],
                    true,
                ),
                t(
                    "TEAMS_CREATE_CHAT",
                    ("Chat erstellen", "Create chat"),
                    (
                        "Startet einen neuen Chat (1:1 bei einer Person, sonst Gruppenchat).",
                        "Starts a new chat (one-on-one for one member, group chat otherwise).",
                    ),
                    &[
                        ("member_ids", str_list("IDs der weiteren Teilnehmer / other participant user IDs")),
                    ],
                    &["member_ids"],
                    &[CHAT_RW],
                    true,
                ),
                t(
                    "TEAMS_SEND_CHAT_MESSAGE",
                    ("Chatnachricht senden", "Send chat message"),
                    (
                        "Sendet eine Nachricht in einen bestehenden Chat.",
                        "Sends a message into an existing chat.",
                    ),
                    &[
                        ("chat_id", s("Chat ID")),
                        ("content", s("Nachrichtentext / message body")),
                        ("content_type", enum_prop("text | html", &["text", "html"], "text")),
                    ],
                    &["chat_id", "content"],
                    &[CHAT_RW],
                    true,
                ),
                t(
                    "TEAMS_DELETE_CHAT_MESSAGE",
                    ("Chatnachricht löschen", "Delete chat message"),
                    (
                        "Löscht eine eigene Chatnachricht (softDelete).",
                        "Soft-deletes a chat message you sent.",
                    ),
                    &[
                        ("user_id", s("Eigene Benutzer-ID (TEAMS_GET_ME) / own user ID")),
                        ("chat_id", s("Chat ID")),
                        ("message_id", s("Message ID")),
                    ],
                    &["user_id", "chat_id", "message_id"],
                    &[CHAT_RW],
                    true,
                ),
                t(
                    "TEAMS_REQUEST_APPROVAL_IN_CHANNEL",
                    ("Freigabe im Kanal anfragen", "Request approval in channel"),
                    (
                        "Postet eine Freigabe-Karte in einen Kanal (die Karte verlinkt auf `confirmation_url`).",
                        "Posts an approval card into a channel (the card links to `confirmation_url`).",
                    ),
                    &[
                        ("team_id", s("Team ID")),
                        ("channel_id", s("Channel ID")),
                        ("message", s("Fragetext / request text")),
                        ("confirmation_url", s("URL, die die Karte als Button öffnet")),
                        ("attachment_id", s("Optionale Attachment-ID / optional attachment ID")),
                    ],
                    &["team_id", "channel_id", "message", "confirmation_url"],
                    &[CHANNEL_MSG_SEND],
                    true,
                ),
                t(
                    "TEAMS_REQUEST_APPROVAL_DIRECT_MESSAGE",
                    ("Freigabe per Chat anfragen", "Request approval via direct message"),
                    (
                        "Postet eine Freigabe-Karte in einen Chat (die Karte verlinkt auf `confirmation_url`).",
                        "Posts an approval card into a chat (the card links to `confirmation_url`).",
                    ),
                    &[
                        ("chat_id", s("Chat ID")),
                        ("message", s("Fragetext / request text")),
                        ("confirmation_url", s("URL, die die Karte als Button öffnet")),
                        ("attachment_id", s("Optionale Attachment-ID / optional attachment ID")),
                    ],
                    &["chat_id", "message", "confirmation_url"],
                    &[CHAT_RW],
                    true,
                ),
            ],
        };
        Self { spec, api_base }
    }

    fn url(&self, path: &str) -> String {
        format!("{}/v1.0{path}", self.api_base)
    }

    /// Body of a channel/chat message (`chatMessage-post-replies` shape).
    fn message_body(args: &Value) -> Result<Value> {
        let content_type = opt_str(args, "content_type").unwrap_or("text");
        if !matches!(content_type, "text" | "html") {
            bail!("invalid argument `content_type`");
        }
        Ok(json!({"body": {"content": req_str(args, "content")?, "contentType": content_type}}))
    }

    /// Adaptive card used by the two approval actions of the TS piece; the
    /// waitpoint resume URL is supplied by the caller as `confirmation_url`.
    fn approval_body(args: &Value) -> Result<Value> {
        let message = req_str(args, "message")?;
        let url = opt_str(args, "confirmation_url")
            .ok_or_else(|| anyhow::anyhow!("missing argument `confirmation_url`"))?;
        let id = opt_str(args, "attachment_id").unwrap_or("ctox-approval");
        let card = json!({
            "$schema": "http://adaptivecards.io/schemas/adaptive-card.json",
            "type": "AdaptiveCard",
            "version": "1.4",
            "body": [{"type": "TextBlock", "text": "Please choose an action:"}],
            "actions": [{"type": "Action.OpenUrl", "title": "Review & Respond", "url": url}],
        });
        Ok(json!({
            "body": {
                "contentType": "html",
                "content": format!("{message}<attachment id=\"{id}\"></attachment>"),
            },
            "attachments": [{
                "id": id,
                "contentType": "application/vnd.microsoft.card.adaptive",
                "contentUrl": Value::Null,
                "content": card.to_string(),
            }],
        }))
    }
}

/// `?$filter=...` with the OData parameter name kept literal (only the value is
/// percent-encoded; `+` for spaces and `%27` for quotes are valid in a query).
fn odata(pairs: &[(&str, Option<String>)]) -> String {
    let mut q = String::new();
    for (k, v) in pairs {
        let Some(v) = v else { continue };
        let enc: String = url::form_urlencoded::byte_serialize(v.as_bytes()).collect();
        if !q.is_empty() {
            q.push('&');
        }
        q.push_str(k);
        q.push('=');
        q.push_str(&enc);
    }
    if q.is_empty() {
        String::new()
    } else {
        format!("?{q}")
    }
}

/// OData `$filter=displayName eq 'x'` for the *search* actions of the TS piece.
fn equality_filter(field: &str, value: &str) -> String {
    format!("{field} eq '{}'", value.replace('\'', "''"))
}

fn collection(v: &Value, key: &str) -> Value {
    match v.get(key).and_then(Value::as_array) {
        Some(a) => Value::Array(a.clone()),
        None => v.clone(),
    }
}

impl ToolkitAdapter for Teams {
    fn spec(&self) -> &ToolkitSpec {
        &self.spec
    }

    fn build_request(&self, tool: &str, args: &Value) -> Result<HttpRequest> {
        Ok(match tool {
            "TEAMS_GET_ME" => HttpRequest::new("GET", self.url("/me")),
            "TEAMS_LIST_MY_TEAMS" => HttpRequest::new("GET", self.url("/me/joinedTeams")),
            "TEAMS_LIST_CHATS" => {
                let top = opt_u64(args, "top", 50, 1, 100);
                HttpRequest::new(
                    "GET",
                    self.url(&format!(
                        "/chats?$expand=members&$top={top}"
                    )),
                )
            }
            "TEAMS_LIST_CHANNELS" => HttpRequest::new(
                "GET",
                self.url(&format!("/teams/{}/channels", path_segment(args, "team_id")?)),
            ),
            "TEAMS_LIST_ALL_CHANNELS" => HttpRequest::new(
                "GET",
                self.url(&format!("/teams/{}/allChannels", path_segment(args, "team_id")?)),
            ),
            "TEAMS_LIST_TEAM_MEMBERS" => HttpRequest::new(
                "GET",
                self.url(&format!("/teams/{}/members", path_segment(args, "team_id")?)),
            ),
            "TEAMS_FIND_CHANNEL" => {
                let filter = equality_filter(
                    "displayName",
                    req_str(args, "channel_name")?,
                );
                HttpRequest::new(
                    "GET",
                    self.url(&format!(
                        "/teams/{}/allChannels{}",
                        path_segment(args, "team_id")?,
                        odata(&[("$filter", Some(filter))])
                    )),
                )
            }
            "TEAMS_FIND_TEAM_MEMBER" => {
                let by = req_str(args, "search_by")?;
                let field = match by {
                    "email" => "microsoft.graph.aadUserConversationMember/email",
                    "name" => "microsoft.graph.aadUserConversationMember/displayName",
                    other => bail!("invalid argument `search_by`: {other}"),
                };
                let filter = equality_filter(field, req_str(args, "search_value")?);
                HttpRequest::new(
                    "GET",
                    self.url(&format!(
                        "/teams/{}/members{}",
                        path_segment(args, "team_id")?,
                        odata(&[("$filter", Some(filter))])
                    )),
                )
            }
            "TEAMS_LIST_CHANNEL_MESSAGES" => {
                let top = opt_u64(args, "top", 20, 1, 50);
                HttpRequest::new(
                    "GET",
                    self.url(&format!(
                        "/teams/{}/channels/{}/messages?$top={top}",
                        path_segment(args, "team_id")?,
                        graph_id(args, "channel_id")?
                    )),
                )
            }
            "TEAMS_GET_CHANNEL_MESSAGE" => {
                let base = format!(
                    "/teams/{}/channels/{}/messages/{}",
                    path_segment(args, "team_id")?,
                    graph_id(args, "channel_id")?,
                    graph_id(args, "message_id")?
                );
                let path = match opt_str(args, "reply_id") {
                    Some(_) => format!("{base}/replies/{}", graph_id(args, "reply_id")?),
                    None => base,
                };
                HttpRequest::new("GET", self.url(&path))
            }
            "TEAMS_LIST_CHAT_MESSAGES" => {
                let top = opt_u64(args, "top", 20, 1, 50);
                HttpRequest::new(
                    "GET",
                    self.url(&format!("/chats/{}/messages?$top={top}", graph_id(args, "chat_id")?)),
                )
            }
            "TEAMS_GET_CHAT_MESSAGE" => HttpRequest::new(
                "GET",
                self.url(&format!(
                    "/chats/{}/messages/{}",
                    graph_id(args, "chat_id")?,
                    graph_id(args, "message_id")?
                )),
            ),
            "TEAMS_FIND_MEETING" => {
                let value = req_str(args, "identifier_value")?;
                let filter = match req_str(args, "identifier_type")? {
                    "joinWebUrl" => equality_filter("JoinWebUrl", value),
                    "joinMeetingId" => equality_filter("joinMeetingIdSettings/joinMeetingId", value),
                    other => bail!("invalid argument `identifier_type`: {other}"),
                };
                HttpRequest::new(
                    "GET",
                    self.url(&format!(
                        "/me/onlineMeetings{}",
                        odata(&[("$filter", Some(filter))])
                    )),
                )
            }
            "TEAMS_LIST_MEETING_RECORDINGS" => {
                let top = opt_u64(args, "top", 20, 1, 50);
                HttpRequest::new(
                    "GET",
                    self.url(&format!(
                        "/me/onlineMeetings/{}/recordings?$top={top}",
                        graph_id(args, "meeting_id")?
                    )),
                )
            }
            "TEAMS_GET_MEETING_RECORDING" => HttpRequest::new(
                "GET",
                self.url(&format!(
                    "/me/onlineMeetings/{}/recordings/{}",
                    graph_id(args, "meeting_id")?,
                    graph_id(args, "recording_id")?
                )),
            ),
            "TEAMS_LIST_MEETING_TRANSCRIPTS" => {
                let top = opt_u64(args, "top", 20, 1, 50);
                HttpRequest::new(
                    "GET",
                    self.url(&format!(
                        "/me/onlineMeetings/{}/transcripts?$top={top}",
                        graph_id(args, "meeting_id")?
                    )),
                )
            }
            "TEAMS_GET_MEETING_TRANSCRIPT" => HttpRequest::new(
                "GET",
                self.url(&format!(
                    "/me/onlineMeetings/{}/transcripts/{}/content",
                    graph_id(args, "meeting_id")?,
                    graph_id(args, "transcript_id")?
                )),
            )
            .header("accept", "text/vtt"),
            "TEAMS_CREATE_CHANNEL" | "TEAMS_CREATE_PRIVATE_CHANNEL" => {
                let mut channel = json!({
                    "displayName": req_str(args, "channel_display_name")?,
                });
                if let Some(d) = opt_str(args, "channel_description") {
                    channel["description"] = json!(d);
                }
                if tool == "TEAMS_CREATE_PRIVATE_CHANNEL" {
                    channel["membershipType"] = json!("private");
                }
                HttpRequest::new(
                    "POST",
                    self.url(&format!("/teams/{}/channels", path_segment(args, "team_id")?)),
                )
                .json(&channel)
            }
            "TEAMS_SEND_CHANNEL_MESSAGE" => {
                let mut msg = Self::message_body(args)?;
                if let Some(m) = args.get("mentions").filter(|v| !v.is_null()) {
                    if !m.is_array() {
                        bail!("invalid argument `mentions`");
                    }
                    msg["mentions"] = m.clone();
                }
                HttpRequest::new(
                    "POST",
                    self.url(&format!(
                        "/teams/{}/channels/{}/messages",
                        path_segment(args, "team_id")?,
                        graph_id(args, "channel_id")?
                    )),
                )
                .json(&msg)
            }
            "TEAMS_REPLY_TO_CHANNEL_MESSAGE" => HttpRequest::new(
                "POST",
                self.url(&format!(
                    "/teams/{}/channels/{}/messages/{}/replies",
                    path_segment(args, "team_id")?,
                    graph_id(args, "channel_id")?,
                    graph_id(args, "message_id")?
                )),
            )
            .json(&Self::message_body(args)?),
            "TEAMS_CREATE_CHAT" => {
                let others = str_vec(args, "member_ids");
                if others.is_empty() {
                    bail!("missing argument `member_ids`");
                }
                let member = |id: &str| {
                    json!({
                        "@odata.type": "#microsoft.graph.aadUserConversationMember",
                        "roles": ["owner"],
                        "user@odata.bind": format!("{}/v1.0/users('{id}')", self.api_base),
                    })
                };
                HttpRequest::new("POST", self.url("/chats")).json(&json!({
                    "chatType": if others.len() == 1 { "oneOnOne" } else { "group" },
                    "members": others.iter().map(|m| member(m)).collect::<Vec<_>>(),
                }))
            }
            "TEAMS_SEND_CHAT_MESSAGE" => HttpRequest::new(
                "POST",
                self.url(&format!("/chats/{}/messages", graph_id(args, "chat_id")?)),
            )
            .json(&Self::message_body(args)?),
            "TEAMS_DELETE_CHAT_MESSAGE" => HttpRequest::new(
                "POST",
                self.url(&format!(
                    "/users/{}/chats/{}/messages/{}/softDelete",
                    path_segment(args, "user_id")?,
                    graph_id(args, "chat_id")?,
                    graph_id(args, "message_id")?
                )),
            )
            .json(&json!({})),
            "TEAMS_REQUEST_APPROVAL_IN_CHANNEL" => HttpRequest::new(
                "POST",
                self.url(&format!(
                    "/teams/{}/channels/{}/messages",
                    path_segment(args, "team_id")?,
                    graph_id(args, "channel_id")?
                )),
            )
            .json(&Self::approval_body(args)?),
            "TEAMS_REQUEST_APPROVAL_DIRECT_MESSAGE" => HttpRequest::new(
                "POST",
                self.url(&format!("/chats/{}/messages", graph_id(args, "chat_id")?)),
            )
            .json(&Self::approval_body(args)?),
            other => bail!("unknown Microsoft Teams tool `{other}`"),
        })
    }

    fn parse_response(&self, tool: &str, response: &HttpResponse) -> Result<Value> {
        // The transcript content endpoint answers with raw `text/vtt`.
        if tool == "TEAMS_GET_MEETING_TRANSCRIPT" {
            let text = match response.json() {
                Ok(Value::String(s)) => s,
                _ => String::from_utf8_lossy(&response.body).into_owned(),
            };
            return Ok(json!({"content": text}));
        }
        let v = response.json()?;
        Ok(match tool {
            "TEAMS_FIND_CHANNEL" | "TEAMS_FIND_TEAM_MEMBER" => json!({
                "found": v["value"].as_array().is_some_and(|a| !a.is_empty()),
                "result": v["value"],
            }),
            "TEAMS_LIST_MY_TEAMS" => json!({"teams": v["value"], "next_link": v["@odata.nextLink"]}),
            "TEAMS_LIST_CHANNELS" | "TEAMS_LIST_ALL_CHANNELS" => {
                json!({"channels": v["value"], "next_link": v["@odata.nextLink"]})
            }
            "TEAMS_LIST_TEAM_MEMBERS" => json!({"members": v["value"], "next_link": v["@odata.nextLink"]}),
            "TEAMS_LIST_CHATS" => {
                let chats = v["value"]
                    .as_array()
                    .map(|a| {
                        a.iter()
                            .map(|c| {
                                json!({
                                    "id": c["id"],
                                    "chat_type": c["chatType"],
                                    "topic": c["topic"],
                                    "created_at": c["createdDateTime"],
                                    "members": c["members"].as_array().map(|m| m
                                        .iter()
                                        .map(|x| json!({"id": x["user"]["id"], "display_name": x["displayName"]}))
                                        .collect::<Vec<_>>()),
                                })
                            })
                            .collect::<Vec<_>>()
                    })
                    .unwrap_or_default();
                json!({"chats": chats, "next_link": v["@odata.nextLink"]})
            }
            "TEAMS_LIST_CHANNEL_MESSAGES"
            | "TEAMS_LIST_CHAT_MESSAGES"
            | "TEAMS_LIST_MEETING_RECORDINGS"
            | "TEAMS_LIST_MEETING_TRANSCRIPTS" => {
                let key = match tool {
                    "TEAMS_LIST_CHANNEL_MESSAGES" | "TEAMS_LIST_CHAT_MESSAGES" => "messages",
                    "TEAMS_LIST_MEETING_RECORDINGS" => "recordings",
                    _ => "transcripts",
                };
                json!({key: collection(&v, "value"), "next_link": v["@odata.nextLink"]})
            }
            "TEAMS_FIND_MEETING" => json!({"meetings": v["value"]}),
            "TEAMS_GET_CHANNEL_MESSAGE" | "TEAMS_GET_CHAT_MESSAGE" => json!({
                "id": v["id"],
                "created_at": v["createdDateTime"],
                "content_type": v["body"]["contentType"],
                "content": v["body"]["content"],
                "from": v["from"]["user"]["displayName"],
                "web_url": v["webUrl"],
            }),
            // Graph answers softDelete with an empty 202; acknowledge it the
            // way the TS action does (it returned a constant `success: true`).
            "TEAMS_DELETE_CHAT_MESSAGE" => json!({"success": true, "status": response.status}),
            "TEAMS_REQUEST_APPROVAL_IN_CHANNEL" | "TEAMS_REQUEST_APPROVAL_DIRECT_MESSAGE" => {
                json!({"approved": false, "message_id": v["id"]})
            }
            _ => v,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_path_injection() {
        let t = Teams::with_api_base("https://g.test");
        for bad in ["../x", "a/b", "a?b", "", ".."] {
            assert!(
                t.build_request(
                    "TEAMS_LIST_CHANNELS",
                    &json!({"team_id": bad})
                )
                .is_err(),
                "accepted {bad}"
            );
            assert!(
                t.build_request(
                    "TEAMS_GET_CHAT_MESSAGE",
                    &json!({"chat_id": "19:abc", "message_id": bad})
                )
                .is_err(),
                "accepted {bad}"
            );
        }
        assert!(t
            .build_request(
                "TEAMS_SEND_CHAT_MESSAGE",
                &json!({"chat_id": "19:a", "content": "hi", "content_type": "markdown"})
            )
            .is_err());
        assert!(t
            .build_request("TEAMS_CREATE_CHAT", &json!({"member_ids": []}))
            .is_err());
        assert!(t
            .build_request("TEAMS_LIST_CHATS", &json!({}))
            .is_ok());
    }

    #[test]
    fn approval_card_embeds_confirmation_url() {
        let body = Teams::approval_body(&json!({
            "message": "Bitte freigeben",
            "confirmation_url": "https://ctox.test/confirm?action=approve",
        }))
        .unwrap();
        assert_eq!(
            body["body"]["content"],
            json!("Bitte freigeben<attachment id=\"ctox-approval\"></attachment>")
        );
        let card: Value =
            serde_json::from_str(body["attachments"][0]["content"].as_str().unwrap()).unwrap();
        assert_eq!(
            card["actions"][0]["url"],
            json!("https://ctox.test/confirm?action=approve")
        );
    }
}