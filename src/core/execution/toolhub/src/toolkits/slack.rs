//! Slack toolkit (Web API `https://slack.com/api`). Action set modelled on
//! Activepieces `piece-slack` (MIT).
//! Auth: bot/user token (xoxb-/xoxp-) as Bearer, or Slack OAuth v2.
//!
//! The Slack Web API is a single flat endpoint family (`POST/GET /api/<method>`);
//! there are no URL path parameters, so every ID is passed as a query parameter
//! or a JSON body field and validated with [`slack_id`] (the same guard
//! [`super::path_segment`] applies to path segments) before it leaves the tool.
use anyhow::{anyhow, bail, Result};
use serde_json::{json, Value};

use super::common::*;
use super::ToolkitAdapter;
use crate::http::{HttpRequest, HttpResponse};
use crate::model::*;

pub const SLUG: &str = "slack";

/// Slack IDs (channels, users, timestamps) are restricted to a safe charset.
fn slack_id<'a>(args: &'a Value, key: &str) -> Result<&'a str> {
    let v = req_str(args, key)?;
    if v.len() > 64
        || !v
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'.' || b == b'_' || b == b'-')
    {
        bail!("invalid argument `{key}`");
    }
    Ok(v)
}

fn flag(args: &Value, key: &str, default: bool) -> bool {
    args.get(key).and_then(Value::as_bool).unwrap_or(default)
}

fn obj_list(args: &Value, key: &str) -> Vec<Value> {
    args.get(key)
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default()
}

/// `textToSectionBlocks` from the Activepieces lib: mrkdwn sections, split at
/// 3000 characters on line/word boundaries (Slack's per-block limit).
const SECTION_MAX: usize = 3000;

fn section(text: &str) -> Value {
    json!({"type": "section", "text": {"type": "mrkdwn", "text": text}})
}

fn section_blocks(text: &str) -> Vec<Value> {
    if text.chars().count() <= SECTION_MAX {
        return vec![section(text)];
    }
    let mut blocks = Vec::new();
    let mut rest = text.to_string();
    while !rest.is_empty() {
        if rest.chars().count() <= SECTION_MAX {
            blocks.push(section(&rest));
            break;
        }
        let head: String = rest.chars().take(SECTION_MAX).collect();
        let idx = head.rfind('\n').filter(|i| *i > 0).or_else(|| head.rfind(' '));
        let idx = idx.unwrap_or(SECTION_MAX);
        blocks.push(section(&rest.chars().take(idx).collect::<String>()));
        rest = rest.chars().skip(idx).collect::<String>();
        rest = rest.trim_start().to_string();
    }
    blocks
}

/// Text blocks first, then caller-supplied Block Kit blocks (as upstream).
fn message_blocks(args: &Value) -> Result<Vec<Value>> {
    let mut b = match opt_str(args, "text") {
        Some(t) => section_blocks(t),
        None => Vec::new(),
    };
    b.extend(obj_list(args, "blocks"));
    if b.is_empty() {
        bail!("either `text` or `blocks` must be provided");
    }
    Ok(b)
}

fn channel_brief(c: &Value) -> Value {
    json!({"id": c["id"], "name": c["name"], "is_private": c["is_private"],
           "is_archived": c["is_archived"], "is_member": c["is_member"], "num_members": c["num_members"]})
}

fn user_brief(u: &Value) -> Value {
    json!({"id": u["id"], "name": u["name"], "real_name": u["real_name"], "is_bot": u["is_bot"],
           "is_admin": u["is_admin"], "deleted": u["deleted"], "tz": u["tz"],
           "display_name": u["profile"]["display_name"]})
}

fn msg_brief(m: &Value) -> Value {
    json!({"ts": m["ts"], "user": m["user"], "text": m["text"], "thread_ts": m["thread_ts"],
           "reply_count": m["reply_count"]})
}

pub struct Slack {
    spec: ToolkitSpec,
    api_base: String,
}

impl Default for Slack {
    fn default() -> Self {
        Self::with_api_base("https://slack.com/api")
    }
}

impl Slack {
    pub fn with_api_base(api_base: &str) -> Self {
        let t = |slug, title, desc, props: Props<'_>, req: &[&str], scopes: &[&str], m| {
            tool(SLUG, slug, title, desc, props, req, scopes, m)
        };
        let channel = || s("Kanal-ID / channel ID (C…, G…, D…)");
        let spec = ToolkitSpec {
            slug: SLUG.into(),
            name: "Slack".into(),
            description: LocalizedText::new(
                "Kanäle, Verlauf, Nachrichten, Mitglieder und Suche in Slack.",
                "Channels, history, messages, members and search in Slack.",
            ),
            auth_schemes: vec![
                AuthScheme::Bearer,
                AuthScheme::OAuth2(OAuth2Scheme {
                    authorize_url: "https://slack.com/oauth/v2/authorize".into(),
                    token_url: "https://slack.com/api/oauth.v2.access".into(),
                    default_scopes: [
                        "channels:read",
                        "channels:history",
                        "channels:write",
                        "groups:read",
                        "groups:history",
                        "groups:write",
                        "chat:write",
                        "im:read",
                        "im:history",
                        "im:write",
                        "mpim:read",
                        "mpim:history",
                        "mpim:write",
                        "users:read",
                        "users:read.email",
                        "users.profile:read",
                        "reactions:read",
                        "reactions:write",
                        "usergroups:read",
                        "files:read",
                    ]
                    .iter()
                    .map(|s| s.to_string())
                    .collect(),
                    pkce: false,
                    scope_separator: ",".into(),
                    refreshable: false,
                    extra_authorize_params: Vec::new(),
                }),
            ],
            tools: vec![
                // ---------------- conversations: read ----------------
                t(
                    "SLACK_LIST_CHANNELS",
                    ("Kanäle auflisten", "List channels"),
                    (
                        "Listet öffentliche/private Kanäle des Workspace.",
                        "Lists public/private channels of the workspace.",
                    ),
                    &[
                        ("types", s("public_channel,private_channel,mpim,im")),
                        ("limit", int("1-1000", 1, 1000, 200)),
                        ("cursor", s("Pagination cursor")),
                        ("exclude_archived", json!({"type": "boolean", "default": true})),
                    ],
                    &[],
                    &["channels:read", "groups:read"],
                    false,
                ),
                t(
                    "SLACK_GET_CHANNEL_INFO",
                    ("Kanal-Info lesen", "Get channel info"),
                    (
                        "Liefert Details eines Kanals (Topic, Purpose, Privat, Archiviert).",
                        "Returns details of one channel (topic, purpose, private, archived).",
                    ),
                    &[
                        ("channel", channel()),
                        ("include_num_members", json!({"type": "boolean", "default": false})),
                    ],
                    &["channel"],
                    &["channels:read", "groups:read"],
                    false,
                ),
                t(
                    "SLACK_GET_HISTORY",
                    ("Verlauf lesen", "Read history"),
                    (
                        "Liest Nachrichten eines Kanals.",
                        "Reads messages of a channel.",
                    ),
                    &[
                        ("channel", channel()),
                        ("limit", int("1-1000", 1, 1000, 50)),
                        ("oldest", s("Unix ts")),
                        ("latest", s("Unix ts")),
                        ("cursor", s("Pagination cursor")),
                    ],
                    &["channel"],
                    &["channels:history", "groups:history"],
                    false,
                ),
                t(
                    "SLACK_GET_MESSAGE",
                    ("Nachricht nach Zeitstempel lesen", "Get message by timestamp"),
                    (
                        "Holt genau eine Nachricht anhand ihres Unix-Timestamps.",
                        "Fetches a single message by its Unix timestamp.",
                    ),
                    &[
                        ("channel", channel()),
                        ("ts", s("Nachrichten-Zeitstempel / message ts")),
                    ],
                    &["channel", "ts"],
                    &["channels:history", "groups:history"],
                    false,
                ),
                t(
                    "SLACK_GET_THREAD",
                    ("Thread lesen", "Read thread"),
                    ("Liest Antworten eines Threads.", "Reads thread replies."),
                    &[
                        ("channel", channel()),
                        ("thread_ts", s("Parent message ts")),
                        ("limit", int("1-1000", 1, 1000, 100)),
                        ("cursor", s("Pagination cursor")),
                    ],
                    &["channel", "thread_ts"],
                    &["channels:history", "groups:history"],
                    false,
                ),
                t(
                    "SLACK_LIST_CHANNEL_MEMBERS",
                    ("Kanalmitglieder auflisten", "List channel members"),
                    (
                        "Listet die User-IDs der Mitglieder eines Kanals.",
                        "Lists the user IDs of the members of a channel.",
                    ),
                    &[
                        ("channel", channel()),
                        ("limit", int("1-1000", 1, 1000, 1000)),
                        ("cursor", s("Pagination cursor")),
                    ],
                    &["channel"],
                    &["channels:read", "groups:read"],
                    false,
                ),
                t(
                    "SLACK_LIST_USER_CONVERSATIONS",
                    ("Unterhaltungen eines Nutzers", "List user conversations"),
                    (
                        "Listet Kanäle, in denen ein Nutzer Mitglied ist.",
                        "Lists the channels a user is a member of.",
                    ),
                    &[
                        ("user", s("User-ID (U…); leer = eigener / user ID, empty = self")),
                        ("types", s("public_channel,private_channel,mpim,im")),
                        ("limit", int("1-1000", 1, 1000, 200)),
                        ("cursor", s("Pagination cursor")),
                        ("exclude_archived", json!({"type": "boolean", "default": true})),
                    ],
                    &[],
                    &["channels:read", "im:read", "mpim:read", "users:read"],
                    false,
                ),
                t(
                    "SLACK_LIST_SCHEDULED_MESSAGES",
                    ("Geplante Nachrichten auflisten", "List scheduled messages"),
                    (
                        "Listet geplante Nachrichten eines Kanals.",
                        "Lists scheduled messages of a channel.",
                    ),
                    &[
                        ("channel", s("Kanal-ID; optional / channel ID, optional")),
                        ("oldest", s("Unix ts, geplant am oder davor / planned at or before")),
                        ("latest", s("Unix ts, geplant am oder danach / planned at or after")),
                        ("limit", int("1-100", 1, 100, 100)),
                    ],
                    &[],
                    &["chat:write"],
                    false,
                ),
                // ---------------- users: read ----------------
                t(
                    "SLACK_LIST_USERS",
                    ("Nutzer auflisten", "List users"),
                    (
                        "Listet die Mitglieder des Workspace (Bots/Deaktivierte filterbar).",
                        "Lists workspace members (filter bots/deactivated).",
                    ),
                    &[
                        ("limit", int("1-1000", 1, 1000, 200)),
                        ("cursor", s("Pagination cursor")),
                        ("include_bots", json!({"type": "boolean", "default": false})),
                        ("include_disabled", json!({"type": "boolean", "default": false})),
                    ],
                    &[],
                    &["users:read"],
                    false,
                ),
                t(
                    "SLACK_GET_USER",
                    ("Nutzerprofil lesen", "Get user profile"),
                    (
                        "Liefert das Profil eines Nutzers per User-ID.",
                        "Returns a user profile by user ID.",
                    ),
                    &[("user", s("User-ID (U… oder W…)/ user ID"))],
                    &["user"],
                    &["users:read"],
                    false,
                ),
                t(
                    "SLACK_FIND_USER_BY_EMAIL",
                    ("Nutzer per E-Mail finden", "Find user by email"),
                    (
                        "Sucht ein Slack-Mitglied per E-Mail.",
                        "Looks up a member by email.",
                    ),
                    &[("email", s("E-Mail / email"))],
                    &["email"],
                    &["users:read.email"],
                    false,
                ),
                t(
                    "SLACK_LIST_USER_GROUPS",
                    ("Benutzergruppen auflisten", "List user groups"),
                    (
                        "Listet die Benutzergruppen (@handle) des Workspace.",
                        "Lists the workspace user groups (@handle).",
                    ),
                    &[(
                        "include_users",
                        json!({"type": "boolean", "default": false}),
                    )],
                    &[],
                    &["usergroups:read"],
                    false,
                ),
                t(
                    "SLACK_LIST_USER_GROUP_USERS",
                    ("Mitglieder einer Benutzergruppe", "List user group members"),
                    (
                        "Listet die Mitglieder einer Benutzergruppe.",
                        "Lists the members of a user group.",
                    ),
                    &[
                        ("usergroup", s("ID der Benutzergruppe / user group ID (S…)")),
                        ("include_disabled", json!({"type": "boolean", "default": false})),
                    ],
                    &["usergroup"],
                    &["usergroups:read"],
                    false,
                ),
                // ---------------- search / files / permalinks ----------------
                t(
                    "SLACK_SEARCH_MESSAGES",
                    ("Nachrichten suchen", "Search messages"),
                    (
                        "Volltextsuche (benötigt User-Token mit search:read).",
                        "Full-text search (needs user token with search:read).",
                    ),
                    &[
                        ("query", s("Slack search query")),
                        ("count", int("1-100", 1, 100, 100)),
                        ("page", int("Seite / page", 1, 100, 1)),
                    ],
                    &["query"],
                    &["search:read"],
                    false,
                ),
                t(
                    "SLACK_SEARCH_ALL",
                    ("Alles suchen", "Search all"),
                    (
                        "Sucht Nachrichten und Dateien (User-Token mit search:read).",
                        "Searches messages and files (user token with search:read).",
                    ),
                    &[
                        ("query", s("Slack search query")),
                        ("count", int("1-100", 1, 100, 100)),
                    ],
                    &["query"],
                    &["search:read"],
                    false,
                ),
                t(
                    "SLACK_GET_MESSAGE_PERMALINK",
                    ("Permalink holen", "Get message permalink"),
                    (
                        "Erzeugt einen permanenten Link auf eine Nachricht.",
                        "Builds a permanent link to a message.",
                    ),
                    &[
                        ("channel", channel()),
                        ("message_ts", s("Nachrichten-Zeitstempel / message ts")),
                    ],
                    &["channel", "message_ts"],
                    &["channels:history", "groups:history"],
                    false,
                ),
                t(
                    "SLACK_LIST_FILES",
                    ("Dateien auflisten", "List files"),
                    (
                        "Listet Dateien eines Kanals oder Nutzers.",
                        "Lists files of a channel or user.",
                    ),
                    &[
                        ("channel", s("Kanal-ID; optional / channel ID, optional")),
                        ("user", s("User-ID; optional / user ID, optional")),
                        ("count", int("1-1000", 1, 1000, 100)),
                        ("page", int("Seite / page", 1, 1000, 1)),
                    ],
                    &[],
                    &["files:read"],
                    false,
                ),
                // ---------------- write ----------------
                t(
                    "SLACK_SEND_MESSAGE",
                    ("Nachricht senden", "Send message"),
                    (
                        "Postet eine Nachricht (optional im Thread); braucht --confirm.",
                        "Posts a message (optionally in a thread); requires --confirm.",
                    ),
                    &[
                        ("channel", channel()),
                        ("text", s("Nachrichtentext (mrkdwn)/ message text (mrkdwn)")),
                        ("thread_ts", s("Antwort im Thread / reply in thread")),
                        ("blocks", json!({"type": "array", "items": {"type": "object"},
                            "description": "Optionale Block-Kit-Blöcke / optional Block Kit blocks"})),
                    ],
                    &["channel"],
                    &["chat:write"],
                    true,
                ),
                t(
                    "SLACK_SEND_EPHEMERAL_MESSAGE",
                    ("Ephemäre Nachricht senden", "Send ephemeral message"),
                    (
                        "Sendet eine nur für einen Nutzer sichtbare Nachricht.",
                        "Sends a message visible only to one user.",
                    ),
                    &[
                        ("channel", channel()),
                        ("user", s("Empfänger-User-ID / recipient user ID")),
                        ("text", s("Nachrichtentext / message text")),
                        ("blocks", json!({"type": "array", "items": {"type": "object"},
                            "description": "Optionale Block-Kit-Blöcke / optional Block Kit blocks"})),
                    ],
                    &["channel", "user"],
                    &["chat:write"],
                    true,
                ),
                t(
                    "SLACK_SCHEDULE_MESSAGE",
                    ("Nachricht planen", "Schedule message"),
                    (
                        "Plant eine Nachricht für einen späteren Zeitpunkt.",
                        "Schedules a message for a later point in time.",
                    ),
                    &[
                        ("channel", channel()),
                        ("post_at", int("Unix-Zeitstempel (Sekunden) in der Zukunft / Unix timestamp (seconds) in the future", 1, 4_102_444_800, 0)),
                        ("text", s("Nachrichtentext / message text")),
                        ("thread_ts", s("Im Thread planen / schedule in thread")),
                        ("blocks", json!({"type": "array", "items": {"type": "object"},
                            "description": "Optionale Block-Kit-Blöcke / optional Block Kit blocks"})),
                    ],
                    &["channel", "post_at"],
                    &["chat:write"],
                    true,
                ),
                t(
                    "SLACK_UPDATE_MESSAGE",
                    ("Nachricht ändern", "Update message"),
                    ("Ändert eine eigene Nachricht.", "Edits an own message."),
                    &[
                        ("channel", channel()),
                        ("ts", s("Nachrichten-Zeitstempel / message ts")),
                        ("text", s("Neuer Text / new text")),
                    ],
                    &["channel", "ts", "text"],
                    &["chat:write"],
                    true,
                ),
                t(
                    "SLACK_DELETE_MESSAGE",
                    ("Nachricht löschen", "Delete message"),
                    (
                        "Löscht eine eigene Nachricht (User-Token).",
                        "Deletes an own message (user token).",
                    ),
                    &[
                        ("channel", channel()),
                        ("ts", s("Nachrichten-Zeitstempel / message ts")),
                    ],
                    &["channel", "ts"],
                    &["chat:write"],
                    true,
                ),
                t(
                    "SLACK_ADD_REACTION",
                    ("Reaktion hinzufügen", "Add reaction"),
                    ("Fügt ein Emoji hinzu.", "Adds an emoji reaction."),
                    &[
                        ("channel", channel()),
                        ("ts", s("Nachrichten-Zeitstempel / message ts")),
                        ("name", s("Emoji-Name ohne Doppelpunkte / emoji name without colons")),
                    ],
                    &["channel", "ts", "name"],
                    &["reactions:write"],
                    true,
                ),
                t(
                    "SLACK_REMOVE_REACTION",
                    ("Reaktion entfernen", "Remove reaction"),
                    ("Entfernt ein Emoji von einer Nachricht.", "Removes an emoji from a message."),
                    &[
                        ("channel", channel()),
                        ("ts", s("Nachrichten-Zeitstempel / message ts")),
                        ("name", s("Emoji-Name ohne Doppelpunkte / emoji name without colons")),
                    ],
                    &["channel", "ts", "name"],
                    &["reactions:write"],
                    true,
                ),
                t(
                    "SLACK_JOIN_CHANNEL",
                    ("Kanal beitreten", "Join channel"),
                    ("Lässt den Bot einem öffentlichen Kanal beitreten.", "Makes the bot join a public channel."),
                    &[("channel", channel())],
                    &["channel"],
                    &["channels:join"],
                    true,
                ),
                t(
                    "SLACK_LEAVE_CHANNEL",
                    ("Kanal verlassen", "Leave channel"),
                    (
                        "Lässt den Bot einen Kanal verlassen.",
                        "Makes the bot leave a channel.",
                    ),
                    &[("channel", channel())],
                    &["channel"],
                    &["channels:read", "groups:read"],
                    true,
                ),
                t(
                    "SLACK_CREATE_CHANNEL",
                    ("Kanal erstellen", "Create channel"),
                    (
                        "Erstellt einen öffentlichen oder privaten Kanal.",
                        "Creates a public or private channel.",
                    ),
                    &[
                        ("name", s("Kanalname (klein, Bindestriche)/ channel name (lowercase, dashes)")),
                        ("is_private", json!({"type": "boolean", "default": false})),
                    ],
                    &["name"],
                    &["channels:manage", "groups:write"],
                    true,
                ),
                t(
                    "SLACK_ARCHIVE_CHANNEL",
                    ("Kanal archivieren", "Archive channel"),
                    ("Archiviert einen Kanal.", "Archives a channel."),
                    &[("channel", channel())],
                    &["channel"],
                    &["channels:manage", "groups:write"],
                    true,
                ),
                t(
                    "SLACK_UNARCHIVE_CHANNEL",
                    ("Kanal wiederherstellen", "Unarchive channel"),
                    (
                        "Hebt die Archivierung eines Kanals auf (User-Token).",
                        "Restores an archived channel (user token).",
                    ),
                    &[("channel", channel())],
                    &["channel"],
                    &["channels:manage", "groups:write"],
                    true,
                ),
                t(
                    "SLACK_RENAME_CHANNEL",
                    ("Kanal umbenennen", "Rename channel"),
                    ("Benennt einen Kanal um.", "Renames a channel."),
                    &[
                        ("channel", channel()),
                        ("name", s("Neuer Kanalname / new channel name")),
                    ],
                    &["channel", "name"],
                    &["channels:manage", "groups:write"],
                    true,
                ),
                t(
                    "SLACK_SET_CHANNEL_TOPIC",
                    ("Kanal-Thema setzen", "Set channel topic"),
                    ("Setzt das Thema eines Kanals.", "Sets the topic of a channel."),
                    &[
                        ("channel", channel()),
                        ("topic", s("Neues Thema / new topic")),
                    ],
                    &["channel", "topic"],
                    &["channels:manage", "groups:write"],
                    true,
                ),
                t(
                    "SLACK_SET_CHANNEL_PURPOSE",
                    ("Kanal-Zweck setzen", "Set channel purpose"),
                    ("Setzt den Zweck eines Kanals.", "Sets the purpose of a channel."),
                    &[
                        ("channel", channel()),
                        ("purpose", s("Neuer Zweck / new purpose")),
                    ],
                    &["channel", "purpose"],
                    &["channels:manage", "groups:write"],
                    true,
                ),
                t(
                    "SLACK_INVITE_USER",
                    ("Nutzer in Kanal einladen", "Invite user to channel"),
                    (
                        "Lädt einen Nutzer in einen Kanal ein.",
                        "Invites a user into a channel.",
                    ),
                    &[
                        ("channel", channel()),
                        ("user", s("User-ID / user ID (U…)")),
                    ],
                    &["channel", "user"],
                    &["channels:write.invites", "groups:write.invites"],
                    true,
                ),
                t(
                    "SLACK_REMOVE_USER_FROM_CHANNEL",
                    ("Nutzer aus Kanal entfernen", "Remove user from channel"),
                    (
                        "Entfernt ein Mitglied aus einem Kanal.",
                        "Removes a member from a channel.",
                    ),
                    &[
                        ("channel", channel()),
                        ("user", s("User-ID / user ID (U…)")),
                    ],
                    &["channel", "user"],
                    &["channels:manage", "groups:write"],
                    true,
                ),
                t(
                    "SLACK_MARK_CONVERSATION_READ",
                    ("Unterhaltung als gelesen markieren", "Mark conversation read"),
                    (
                        "Markiert die Unterhaltung bis zu einer Nachricht als gelesen.",
                        "Marks the conversation read up to a message.",
                    ),
                    &[
                        ("channel", channel()),
                        ("ts", s("Nachrichten-Zeitstempel / message ts")),
                    ],
                    &["channel", "ts"],
                    &["channels:read", "groups:read"],
                    true,
                ),
            ],
        };
        Self {
            spec,
            api_base: api_base.trim_end_matches('/').to_string(),
        }
    }

    fn get(&self, method: &str, q: String) -> HttpRequest {
        HttpRequest::new("GET", format!("{}/{method}{q}", self.api_base))
    }
    fn post(&self, method: &str, body: Value) -> HttpRequest {
        HttpRequest::new("POST", format!("{}/{method}", self.api_base))
            .json(&body)
            .header("content-type", "application/json; charset=utf-8")
    }
    /// `{channel}` shorthand used by most conversation tools.
    fn channel_req<'a>(&self, args: &'a Value) -> Result<&'a str> {
        slack_id(args, "channel")
    }
}

impl ToolkitAdapter for Slack {
    fn spec(&self) -> &ToolkitSpec {
        &self.spec
    }

    fn build_request(&self, tool: &str, a: &Value) -> Result<HttpRequest> {
        let o = |k: &str| opt_str(a, k).map(String::from);
        Ok(match tool {
            "SLACK_LIST_CHANNELS" => self.get(
                "conversations.list",
                query(&[
                    (
                        "types",
                        Some(o("types").unwrap_or_else(|| "public_channel,private_channel".into())),
                    ),
                    ("limit", Some(opt_u64(a, "limit", 200, 1, 1000).to_string())),
                    ("cursor", o("cursor")),
                    ("exclude_archived", Some(flag(a, "exclude_archived", true).to_string())),
                ]),
            ),
            "SLACK_GET_CHANNEL_INFO" => self.get(
                "conversations.info",
                query(&[
                    ("channel", Some(self.channel_req(a)?.into())),
                    ("include_num_members", Some(flag(a, "include_num_members", false).to_string())),
                ]),
            ),
            "SLACK_GET_HISTORY" => self.get(
                "conversations.history",
                query(&[
                    ("channel", Some(self.channel_req(a)?.into())),
                    ("limit", Some(opt_u64(a, "limit", 50, 1, 1000).to_string())),
                    ("oldest", o("oldest")),
                    ("latest", o("latest")),
                    ("cursor", o("cursor")),
                ]),
            ),
            "SLACK_GET_MESSAGE" => self.get(
                "conversations.history",
                query(&[
                    ("channel", Some(self.channel_req(a)?.into())),
                    ("oldest", Some(slack_id(a, "ts")?.into())),
                    ("limit", Some("1".into())),
                    ("inclusive", Some("true".into())),
                ]),
            ),
            "SLACK_GET_THREAD" => self.get(
                "conversations.replies",
                query(&[
                    ("channel", Some(self.channel_req(a)?.into())),
                    ("ts", Some(slack_id(a, "thread_ts")?.into())),
                    ("limit", Some(opt_u64(a, "limit", 100, 1, 1000).to_string())),
                    ("cursor", o("cursor")),
                ]),
            ),
            "SLACK_LIST_CHANNEL_MEMBERS" => self.get(
                "conversations.members",
                query(&[
                    ("channel", Some(self.channel_req(a)?.into())),
                    ("limit", Some(opt_u64(a, "limit", 1000, 1, 1000).to_string())),
                    ("cursor", o("cursor")),
                ]),
            ),
            "SLACK_LIST_USER_CONVERSATIONS" => self.get(
                "users.conversations",
                query(&[
                    ("user", o("user")),
                    (
                        "types",
                        Some(o("types").unwrap_or_else(|| "public_channel,private_channel".into())),
                    ),
                    ("limit", Some(opt_u64(a, "limit", 200, 1, 1000).to_string())),
                    ("cursor", o("cursor")),
                    ("exclude_archived", Some(flag(a, "exclude_archived", true).to_string())),
                ]),
            ),
            "SLACK_LIST_SCHEDULED_MESSAGES" => self.get(
                "chat.scheduledMessages.list",
                query(&[
                    ("channel", o("channel")),
                    ("oldest", o("oldest")),
                    ("latest", o("latest")),
                    ("limit", Some(opt_u64(a, "limit", 100, 1, 100).to_string())),
                ]),
            ),
            "SLACK_LIST_USERS" => self.get(
                "users.list",
                query(&[
                    ("limit", Some(opt_u64(a, "limit", 200, 1, 1000).to_string())),
                    ("cursor", o("cursor")),
                ]),
            ),
            "SLACK_GET_USER" => self.get(
                "users.info",
                query(&[("user", Some(slack_id(a, "user")?.into()))]),
            ),
            "SLACK_FIND_USER_BY_EMAIL" => self.get(
                "users.lookupByEmail",
                query(&[("email", Some(req_str(a, "email")?.into()))]),
            ),
            "SLACK_LIST_USER_GROUPS" => self.get(
                "usergroups.list",
                query(&[(
                    "include_users",
                    Some(flag(a, "include_users", false).to_string()),
                )]),
            ),
            "SLACK_LIST_USER_GROUP_USERS" => self.get(
                "usergroups.users.list",
                query(&[
                    ("usergroup", Some(slack_id(a, "usergroup")?.into())),
                    ("include_disabled", Some(flag(a, "include_disabled", false).to_string())),
                ]),
            ),
            "SLACK_SEARCH_MESSAGES" => self.get(
                "search.messages",
                query(&[
                    ("query", Some(req_str(a, "query")?.into())),
                    ("count", Some(opt_u64(a, "count", 100, 1, 100).to_string())),
                    ("page", Some(opt_u64(a, "page", 1, 1, 100).to_string())),
                ]),
            ),
            "SLACK_SEARCH_ALL" => self.get(
                "search.all",
                query(&[
                    ("query", Some(req_str(a, "query")?.into())),
                    ("count", Some(opt_u64(a, "count", 100, 1, 100).to_string())),
                ]),
            ),
            "SLACK_GET_MESSAGE_PERMALINK" => self.get(
                "chat.getPermalink",
                query(&[
                    ("channel", Some(self.channel_req(a)?.into())),
                    ("message_ts", Some(slack_id(a, "message_ts")?.into())),
                ]),
            ),
            "SLACK_LIST_FILES" => self.get(
                "files.list",
                query(&[
                    ("channel", o("channel")),
                    ("user", o("user")),
                    ("count", Some(opt_u64(a, "count", 100, 1, 1000).to_string())),
                    ("page", Some(opt_u64(a, "page", 1, 1, 1000).to_string())),
                ]),
            ),
            "SLACK_SEND_MESSAGE" => {
                let blocks = message_blocks(a)?;
                let text = req_str(a, "text").ok();
                let mut b = json!({"channel": self.channel_req(a)?, "blocks": blocks});
                if let Some(t) = text {
                    b["text"] = json!(t);
                }
                if opt_str(a, "thread_ts").is_some() {
                    b["thread_ts"] = json!(slack_id(a, "thread_ts")?);
                }
                self.post("chat.postMessage", b)
            }
            "SLACK_SEND_EPHEMERAL_MESSAGE" => {
                let blocks = message_blocks(a)?;
                let mut b = json!({
                    "channel": self.channel_req(a)?,
                    "user": slack_id(a, "user")?,
                    "blocks": blocks,
                });
                if let Some(t) = opt_str(a, "text") {
                    b["text"] = json!(t);
                }
                self.post("chat.postEphemeral", b)
            }
            "SLACK_SCHEDULE_MESSAGE" => {
                let blocks = message_blocks(a)?;
                let mut b = json!({
                    "channel": self.channel_req(a)?,
                    "post_at": opt_u64(a, "post_at", 0, 1, 4_102_444_800),
                    "blocks": blocks,
                });
                if let Some(t) = opt_str(a, "text") {
                    b["text"] = json!(t);
                }
                if opt_str(a, "thread_ts").is_some() {
                    b["thread_ts"] = json!(slack_id(a, "thread_ts")?);
                }
                self.post("chat.scheduleMessage", b)
            }
            "SLACK_UPDATE_MESSAGE" => self.post(
                "chat.update",
                json!({"channel": self.channel_req(a)?, "ts": slack_id(a, "ts")?,
                       "text": req_str(a, "text")?}),
            ),
            "SLACK_DELETE_MESSAGE" => self.post(
                "chat.delete",
                json!({"channel": self.channel_req(a)?, "ts": slack_id(a, "ts")?}),
            ),
            "SLACK_ADD_REACTION" | "SLACK_REMOVE_REACTION" => {
                let method = if tool == "SLACK_ADD_REACTION" {
                    "reactions.add"
                } else {
                    "reactions.remove"
                };
                self.post(
                    method,
                    json!({"channel": self.channel_req(a)?, "timestamp": slack_id(a, "ts")?,
                           "name": req_str(a, "name")?.trim_matches(':')}),
                )
            }
            "SLACK_JOIN_CHANNEL" => {
                self.post("conversations.join", json!({"channel": self.channel_req(a)?}))
            }
            "SLACK_LEAVE_CHANNEL" => {
                self.post("conversations.leave", json!({"channel": self.channel_req(a)?}))
            }
            "SLACK_CREATE_CHANNEL" => self.post(
                "conversations.create",
                json!({"name": req_str(a, "name")?, "is_private": flag(a, "is_private", false)}),
            ),
            "SLACK_ARCHIVE_CHANNEL" => {
                self.post("conversations.archive", json!({"channel": self.channel_req(a)?}))
            }
            "SLACK_UNARCHIVE_CHANNEL" => {
                self.post("conversations.unarchive", json!({"channel": self.channel_req(a)?}))
            }
            "SLACK_RENAME_CHANNEL" => self.post(
                "conversations.rename",
                json!({"channel": self.channel_req(a)?, "name": req_str(a, "name")?}),
            ),
            "SLACK_SET_CHANNEL_TOPIC" => self.post(
                "conversations.setTopic",
                json!({"channel": self.channel_req(a)?, "topic": req_str(a, "topic")?}),
            ),
            "SLACK_SET_CHANNEL_PURPOSE" => self.post(
                "conversations.setPurpose",
                json!({"channel": self.channel_req(a)?, "purpose": req_str(a, "purpose")?}),
            ),
            "SLACK_INVITE_USER" => self.post(
                "conversations.invite",
                json!({"channel": self.channel_req(a)?, "users": slack_id(a, "user")?}),
            ),
            "SLACK_REMOVE_USER_FROM_CHANNEL" => self.post(
                "conversations.kick",
                json!({"channel": self.channel_req(a)?, "user": slack_id(a, "user")?}),
            ),
            "SLACK_MARK_CONVERSATION_READ" => self.post(
                "conversations.mark",
                json!({"channel": self.channel_req(a)?, "ts": slack_id(a, "ts")?}),
            ),
            other => bail!("unknown Slack tool `{other}`"),
        })
    }

    fn parse_response(&self, tool: &str, response: &HttpResponse) -> Result<Value> {
        let v = response.json()?;
        // Slack answers errors with HTTP 200 + ok:false.
        if v.get("ok").and_then(Value::as_bool) != Some(true) {
            let e = v
                .get("error")
                .and_then(Value::as_str)
                .unwrap_or("unknown_error");
            return Err(anyhow!("slack error: {e}"));
        }
        let arr = |k: &str| {
            v[k].as_array()
                .map(|a| a.iter().map(channel_brief).collect::<Vec<_>>())
                .unwrap_or_default()
        };
        let cursor = || v["response_metadata"]["next_cursor"].clone();
        let group_brief = |g: &Value| {
            json!({"id": g["id"], "handle": g["handle"], "name": g["name"],
                   "description": g["description"], "user_count": g["user_count"]})
        };
        Ok(match tool {
            "SLACK_LIST_CHANNELS" | "SLACK_LIST_USER_CONVERSATIONS" => {
                json!({"channels": arr("channels"), "next_cursor": cursor()})
            }
            "SLACK_GET_CHANNEL_INFO" => json!({"channel": channel_brief(&v["channel"])}),
            "SLACK_GET_HISTORY" | "SLACK_GET_THREAD" => json!({
                "messages": v["messages"].as_array().map(|m| m.iter().map(msg_brief).collect::<Vec<_>>()),
                "has_more": v["has_more"], "next_cursor": cursor()}),
            "SLACK_GET_MESSAGE" => {
                let m = v["messages"]
                    .as_array()
                    .and_then(|a| a.first())
                    .ok_or_else(|| anyhow!("no message found for the provided timestamp"))?;
                msg_brief(m)
            }
            "SLACK_LIST_CHANNEL_MEMBERS" => {
                json!({"members": v["members"], "count": v["members"].as_array().map(Vec::len).unwrap_or(0),
                       "next_cursor": cursor()})
            }
            "SLACK_LIST_SCHEDULED_MESSAGES" => json!({
                "scheduled_messages": v["scheduled_messages"].as_array().map(|a| a.iter()
                    .map(|m| json!({"id": m["id"], "channel_id": m["channel_id"], "post_at": m["post_at"],
                                    "text": m["text"]})).collect::<Vec<_>>()),
                "next_cursor": cursor()}),
            "SLACK_LIST_USERS" => json!({
                "users": v["members"].as_array().map(|m| m.iter().map(user_brief).collect::<Vec<_>>()),
                "next_cursor": cursor()}),
            "SLACK_GET_USER" | "SLACK_FIND_USER_BY_EMAIL" => user_brief(&v["user"]),
            "SLACK_LIST_USER_GROUPS" => {
                json!({"usergroups": v["usergroups"].as_array().map(|g| g.iter().map(group_brief).collect::<Vec<_>>())})
            }
            "SLACK_LIST_USER_GROUP_USERS" => json!({
                "users": v["users"], "count": v["users"].as_array().map(Vec::len).unwrap_or(0)}),
            "SLACK_SEARCH_MESSAGES" => json!({
                "total": v["messages"]["total"],
                "matches": v["messages"]["matches"].as_array().map(|m| m.iter().map(|x| json!({
                    "ts": x["ts"], "text": x["text"], "user": x["username"],
                    "channel": x["channel"]["name"], "permalink": x["permalink"]})).collect::<Vec<_>>())}),
            "SLACK_SEARCH_ALL" => json!({
                "messages": v["messages"]["matches"], "message_count": v["messages"]["total"],
                "files": v["files"]["matches"], "file_count": v["files"]["total"]}),
            "SLACK_GET_MESSAGE_PERMALINK" => {
                json!({"channel": v["channel"], "permalink": v["permalink"]})
            }
            "SLACK_LIST_FILES" => json!({
                "files": v["files"].as_array().map(|f| f.iter().map(|x| json!({
                    "id": x["id"], "name": x["name"], "title": x["title"],
                    "mimetype": x["mimetype"], "permalink": x["permalink"]})).collect::<Vec<_>>()),
                "paging": v["paging"]}),
            "SLACK_SEND_MESSAGE" | "SLACK_UPDATE_MESSAGE" => {
                json!({"channel": v["channel"], "ts": v["ts"], "text": v["message"]["text"]})
            }
            "SLACK_SEND_EPHEMERAL_MESSAGE" => json!({"message_ts": v["message_ts"]}),
            "SLACK_SCHEDULE_MESSAGE" => {
                json!({"channel": v["channel"], "scheduled_message_id": v["scheduled_message_id"],
                       "post_at": v["post_at"]})
            }
            "SLACK_CREATE_CHANNEL" => json!({"channel": channel_brief(&v["channel"])}),
            "SLACK_RENAME_CHANNEL" => json!({"channel": channel_brief(&v["channel"])}),
            _ => json!({"ok": true}),
        })
    }
}
#[cfg(test)]
mod tests {
    use super::*;

    fn s() -> Slack {
        Slack::with_api_base("https://s.test/api")
    }

    #[test]
    fn requests_ids_and_errors() {
        let s = s();
        let r = s
            .build_request(
                "SLACK_GET_HISTORY",
                &json!({"channel": "C123", "limit": 5000}),
            )
            .unwrap();
        assert_eq!(
            r.url,
            "https://s.test/api/conversations.history?channel=C123&limit=1000"
        );
        assert!(s
            .build_request("SLACK_GET_HISTORY", &json!({"channel": "C1&x=y"}))
            .is_err());
        assert!(s
            .build_request("SLACK_GET_USER", &json!({"user": "../etc"}))
            .is_err());
        let err = HttpResponse {
            status: 200,
            body: br#"{"ok":false,"error":"channel_not_found"}"#.to_vec(),
            session_id: None,
        };
        assert!(s
            .parse_response("SLACK_GET_HISTORY", &err)
            .unwrap_err()
            .to_string()
            .contains("channel_not_found"));
    }

    #[test]
    fn post_message_builds_mrkdwn_blocks() {
        let p = s()
            .build_request(
                "SLACK_SEND_MESSAGE",
                &json!({"channel": "C1", "text": "Hi", "thread_ts": "1.2"}),
            )
            .unwrap();
        assert_eq!(p.method, "POST");
        assert_eq!(p.url, "https://s.test/api/chat.postMessage");
        assert_eq!(
            serde_json::from_slice::<Value>(&p.body.unwrap()).unwrap(),
            json!({"channel": "C1", "text": "Hi", "thread_ts": "1.2",
                   "blocks": [{"type": "section", "text": {"type": "mrkdwn", "text": "Hi"}}]})
        );
        // Either text or blocks must be present.
        assert!(s()
            .build_request("SLACK_SEND_MESSAGE", &json!({"channel": "C1"}))
            .is_err());
    }

    #[test]
    fn long_text_is_split_into_sections() {
        let text = "a".repeat(SECTION_MAX + 10);
        let blocks = section_blocks(&text);
        assert_eq!(blocks.len(), 2);
        let first = blocks[0]["text"]["text"].as_str().unwrap().chars().count();
        assert_eq!(first, SECTION_MAX);
        // Multi-byte safe: never panics on a char boundary.
        assert_eq!(section_blocks(&"ä".repeat(SECTION_MAX * 2)).len(), 2);
    }
}
