//! Discord toolkit (REST API). Action set modelled on Activepieces
//! `piece-discord` (MIT): the human-facing actions of `src/lib/actions/*.ts`
//! plus the dropdown loaders of `src/lib/common/index.ts`, which the piece only
//! uses to fill pick-lists and which therefore become plain list tools
//! (PORTING.md: "Dynamic dropdowns … become plain id strings (+ a list tool)").
//!
//! Auth: the piece authenticates with a bot token in the `Authorization`
//! header, `authorization: \`Bot ${secret_text}\`` (see every `run()` and the
//! `authMapping` of the custom API call). That is modelled as an API-key scheme
//! with the `Bot ` prefix — Discord has no OAuth scopes for bot tokens, so a
//! tool's `scopes` carry the Discord permission (or privileged intent) the call
//! needs, exactly like Linear's API-key permission levels.
//!
//! API versions: the piece pins two versions — `https://discord.com/api/v9` for
//! nearly every action and `https://discord.com/api/v10` for
//! `send-message-with-bot.ts`. Both are preserved verbatim, so the configured
//! base is `https://discord.com/api` and every arm appends the version segment
//! its TypeScript source uses.
//!
//! Not ported: the triggers (`new-message`, `new-member`), the webhook action
//! (`send-message-webhook.ts` posts to a caller-supplied webhook URL instead of
//! using the bot token), the approval action (`send-approval-message.ts` pauses
//! the flow on an Activepieces waitpoint, so it is not one HTTP request), the
//! `files[]` multipart variant of `send-message-with-bot.ts` (the hub has no
//! binary upload), all `ai-*` atomics and the `custom_api_call` action.
//!
//! `find-channel.ts` and `find-guild-member.ts` filter their result client-side
//! (`channel.name === name`, `matchesSearch`), which `parse_response` cannot do
//! without the request arguments; `DISCORD_LIST_CHANNELS` and
//! `DISCORD_LIST_MEMBERS` return the raw lists and the caller matches.
//!
//! `delete-channel.ts` / `delete-guild-role.ts` map a 404 to
//! `{success: true, alreadyAbsent: true}`. The hub only calls `parse_response`
//! for 2xx responses, so an already-deleted target surfaces as a provider error
//! here instead of a success object.
use anyhow::{bail, Result};
use serde_json::{json, Value};

use super::common::*;
use super::{path_segment, ToolkitAdapter};
use crate::http::{HttpRequest, HttpResponse};
use crate::model::*;

pub const SLUG: &str = "discord";

/// Discord bot tokens carry no scopes; these are the permissions/intents the
/// Discord developer portal documents for the respective call.
const VIEW_CHANNEL: &str = "View Channels";
const SEND_MESSAGES: &str = "Send Messages";
const MANAGE_CHANNELS: &str = "Manage Channels";
const MANAGE_ROLES: &str = "Manage Roles";
const BAN_MEMBERS: &str = "Ban Members";
const KICK_MEMBERS: &str = "Kick Members";
/// Privileged intent needed for `GET /guilds/{guild.id}/members`.
const MEMBERS_INTENT: &str = "Server Members Intent";

/// `MEMBERS_PAGE_SIZE` from `find-guild-member.ts`.
const MEMBERS_PAGE_SIZE: u64 = 1000;
/// `MAX_COLOR_VALUE` from `create-guild-role.ts`.
const MAX_COLOR_VALUE: u64 = 0x00ff_ffff;

pub struct Discord {
    spec: ToolkitSpec,
    api_base: String,
}

impl Default for Discord {
    fn default() -> Self {
        Self::with_api_base("https://discord.com/api")
    }
}

impl Discord {
    pub fn with_api_base(api_base: &str) -> Self {
        let t = |slug, title, desc, props: Props<'_>, req: &[&str], scopes: &[&str], m| {
            tool(SLUG, slug, title, desc, props, req, scopes, m)
        };
        let spec = ToolkitSpec {
            slug: SLUG.into(),
            name: "Discord".into(),
            description: LocalizedText::new(
                "Discord: Server, Kanäle, Rollen und Mitglieder lesen, Nachrichten senden, Kanäle und Rollen verwalten.",
                "Discord: read servers, channels, roles and members, send messages, manage channels and roles.",
            ),
            // `authorization: Bot <bot token>` — see the module docs.
            auth_schemes: vec![AuthScheme::ApiKey {
                header_name: "authorization".into(),
                prefix: "Bot ".into(),
            }],
            tools: vec![
                t(
                    "DISCORD_LIST_GUILDS",
                    ("Server auflisten", "List servers"),
                    (
                        "Listet alle Server, in denen der Bot Mitglied ist.",
                        "Lists every server the bot has been added to.",
                    ),
                    &[],
                    &[],
                    &[],
                    false,
                ),
                t(
                    "DISCORD_LIST_CHANNELS",
                    ("Kanäle auflisten", "List channels"),
                    (
                        "Listet alle Kanäle eines Servers mit Typ und Kategorie.",
                        "Lists every channel of a server with type and category.",
                    ),
                    &[("guild_id", s("Server-ID, z. B. aus DISCORD_LIST_GUILDS"))],
                    &["guild_id"],
                    &[VIEW_CHANNEL],
                    false,
                ),
                t(
                    "DISCORD_LIST_ROLES",
                    ("Rollen auflisten", "List roles"),
                    (
                        "Listet die Rollen eines Servers mit Farbe und Position.",
                        "Lists the roles of a server with color and position.",
                    ),
                    &[("guild_id", s("Server-ID, z. B. aus DISCORD_LIST_GUILDS"))],
                    &["guild_id"],
                    &[VIEW_CHANNEL],
                    false,
                ),
                t(
                    "DISCORD_LIST_MEMBERS",
                    ("Mitglieder auflisten", "List members"),
                    (
                        "Listet die Mitglieder eines Servers seitenweise; braucht das Server-Members-Intent.",
                        "Lists the members of a server page by page; needs the Server Members intent.",
                    ),
                    &[
                        ("guild_id", s("Server-ID, z. B. aus DISCORD_LIST_GUILDS")),
                        ("limit", int("1-1000", 1, MEMBERS_PAGE_SIZE, MEMBERS_PAGE_SIZE)),
                        ("after", s("User-ID der letzten Zeile der vorherigen Seite")),
                    ],
                    &["guild_id"],
                    &[MEMBERS_INTENT],
                    false,
                ),
                t(
                    "DISCORD_SEND_MESSAGE",
                    ("Nachricht senden", "Send message"),
                    (
                        "Postet eine Nachricht als Bot in einen Kanal oder Thread; braucht --confirm.",
                        "Posts a message as the bot to a channel or thread; requires --confirm.",
                    ),
                    &[
                        ("channel_id", s("Kanal- oder Thread-ID, z. B. aus DISCORD_LIST_CHANNELS")),
                        ("message", s("Text der Nachricht, höchstens 2000 Zeichen")),
                    ],
                    &["channel_id", "message"],
                    &[SEND_MESSAGES],
                    true,
                ),
                t(
                    "DISCORD_CREATE_CHANNEL",
                    ("Textkanal anlegen", "Create channel"),
                    (
                        "Legt einen Textkanal in einem Server an; braucht --confirm.",
                        "Creates a text channel in a server; requires --confirm.",
                    ),
                    &[
                        ("guild_id", s("Server-ID, z. B. aus DISCORD_LIST_GUILDS")),
                        ("name", s("Kanalname; Discord nutzt Kleinbuchstaben und Bindestriche")),
                        ("topic", s("Thema des Kanals, optional")),
                    ],
                    &["guild_id", "name"],
                    &[MANAGE_CHANNELS],
                    true,
                ),
                t(
                    "DISCORD_RENAME_CHANNEL",
                    ("Kanal umbenennen", "Rename channel"),
                    (
                        "Gibt einem Kanal einen neuen Namen; braucht --confirm.",
                        "Gives a channel a new name; requires --confirm.",
                    ),
                    &[
                        ("channel_id", s("Kanal-ID, z. B. aus DISCORD_LIST_CHANNELS")),
                        ("name", s("Neuer Kanalname; Discord nutzt Kleinbuchstaben und Bindestriche")),
                    ],
                    &["channel_id", "name"],
                    &[MANAGE_CHANNELS],
                    true,
                ),
                t(
                    "DISCORD_DELETE_CHANNEL",
                    ("Kanal löschen", "Delete channel"),
                    (
                        "Löscht einen Kanal mit allen Nachrichten dauerhaft; braucht --confirm.",
                        "Permanently deletes a channel and all of its messages; requires --confirm.",
                    ),
                    &[("channel_id", s("Kanal-ID, z. B. aus DISCORD_LIST_CHANNELS"))],
                    &["channel_id"],
                    &[MANAGE_CHANNELS],
                    true,
                ),
                t(
                    "DISCORD_CREATE_ROLE",
                    ("Rolle anlegen", "Create role"),
                    (
                        "Legt eine Rolle in einem Server an; braucht --confirm.",
                        "Creates a role in a server; requires --confirm.",
                    ),
                    &[
                        ("guild_id", s("Server-ID, z. B. aus DISCORD_LIST_GUILDS")),
                        ("role_name", s("Name der Rolle, z. B. Moderators")),
                        ("role_color", s("Hex-Farbe wie #5865F2 oder eine Dezimalzahl; optional")),
                        ("display_separated", json!({"type": "boolean", "description": "Mitglieder mit dieser Rolle in einer eigenen Sidebar-Gruppe zeigen", "default": false})),
                        ("role_mentionable", json!({"type": "boolean", "description": "@Erwähnung dieser Rolle erlauben", "default": false})),
                        ("creation_reason", s("Grund für das Server-Audit-Log, optional")),
                    ],
                    &["guild_id", "role_name"],
                    &[MANAGE_ROLES],
                    true,
                ),
                t(
                    "DISCORD_DELETE_ROLE",
                    ("Rolle löschen", "Delete role"),
                    (
                        "Löscht eine Rolle aus einem Server; braucht --confirm.",
                        "Deletes a role from a server; requires --confirm.",
                    ),
                    &[
                        ("guild_id", s("Server-ID, z. B. aus DISCORD_LIST_GUILDS")),
                        ("role_id", s("Rollen-ID, z. B. aus DISCORD_LIST_ROLES")),
                        ("deletion_reason", s("Grund für das Server-Audit-Log, optional")),
                    ],
                    &["guild_id", "role_id"],
                    &[MANAGE_ROLES],
                    true,
                ),
                t(
                    "DISCORD_ADD_ROLE_TO_MEMBER",
                    ("Rolle an Mitglied geben", "Add role to member"),
                    (
                        "Gibt einem Servermitglied eine Rolle; braucht --confirm.",
                        "Gives a server member a role; requires --confirm.",
                    ),
                    &[
                        ("guild_id", s("Server-ID, z. B. aus DISCORD_LIST_GUILDS")),
                        ("user_id", s("User-ID, z. B. aus DISCORD_LIST_MEMBERS")),
                        ("role_id", s("Rollen-ID, z. B. aus DISCORD_LIST_ROLES")),
                    ],
                    &["guild_id", "user_id", "role_id"],
                    &[MANAGE_ROLES],
                    true,
                ),
                t(
                    "DISCORD_REMOVE_ROLE_FROM_MEMBER",
                    ("Rolle entziehen", "Remove role from member"),
                    (
                        "Entzieht einem Servermitglied eine Rolle; braucht --confirm.",
                        "Takes a role away from a server member; requires --confirm.",
                    ),
                    &[
                        ("guild_id", s("Server-ID, z. B. aus DISCORD_LIST_GUILDS")),
                        ("user_id", s("User-ID, z. B. aus DISCORD_LIST_MEMBERS")),
                        ("role_id", s("Rollen-ID, z. B. aus DISCORD_LIST_ROLES")),
                    ],
                    &["guild_id", "user_id", "role_id"],
                    &[MANAGE_ROLES],
                    true,
                ),
                t(
                    "DISCORD_REMOVE_MEMBER_FROM_SERVER",
                    ("Mitglied entfernen", "Remove member from server"),
                    (
                        "Kickt ein Mitglied; es kann mit einem Invite zurückkehren; braucht --confirm.",
                        "Kicks a member; they can rejoin with an invite; requires --confirm.",
                    ),
                    &[
                        ("guild_id", s("Server-ID, z. B. aus DISCORD_LIST_GUILDS")),
                        ("user_id", s("User-ID, z. B. aus DISCORD_LIST_MEMBERS")),
                    ],
                    &["guild_id", "user_id"],
                    &[KICK_MEMBERS],
                    true,
                ),
                t(
                    "DISCORD_BAN_MEMBER",
                    ("Mitglied bannen", "Ban member"),
                    (
                        "Sperrt einen Nutzer dauerhaft aus dem Server; braucht --confirm.",
                        "Bans a user from the server so they cannot rejoin; requires --confirm.",
                    ),
                    &[
                        ("guild_id", s("Server-ID, z. B. aus DISCORD_LIST_GUILDS")),
                        ("user_id", s("User-ID; Entwicklermodus: beim Mitglied Copy User ID")),
                        ("ban_reason", s("Grund für das Server-Audit-Log, optional")),
                    ],
                    &["guild_id", "user_id"],
                    &[BAN_MEMBERS],
                    true,
                ),
                t(
                    "DISCORD_REMOVE_BAN_FROM_USER",
                    ("Sperre aufheben", "Remove ban from user"),
                    (
                        "Hebt eine Sperre auf, damit der Nutzer zurückkehren kann; braucht --confirm.",
                        "Lifts a ban so the user can rejoin the server; requires --confirm.",
                    ),
                    &[
                        ("guild_id", s("Server-ID, z. B. aus DISCORD_LIST_GUILDS")),
                        ("user_id", s("User-ID; Entwicklermodus: beim Mitglied Copy User ID")),
                        ("unban_reason", s("Grund für das Server-Audit-Log, optional")),
                    ],
                    &["guild_id", "user_id"],
                    &[BAN_MEMBERS],
                    true,
                ),
            ],
        };
        Self {
            spec,
            api_base: api_base.trim_end_matches('/').to_string(),
        }
    }

    /// `https://discord.com/api/v9` — the version every action but
    /// `send-message-with-bot.ts` uses.
    fn v9(&self) -> String {
        format!("{}/v9", self.api_base)
    }

    /// `https://discord.com/api/v10` — used by `send-message-with-bot.ts` only.
    fn v10(&self) -> String {
        format!("{}/v10", self.api_base)
    }

    fn guild_url(&self, guild_id: &str, tail: &str) -> String {
        format!("{}/guilds/{guild_id}{tail}", self.v9())
    }

    fn channel_url(&self, channel_id: &str) -> String {
        format!("{}/channels/{channel_id}", self.v9())
    }
}

/// `X-Audit-Log-Reason`, sent by `ban-a-guild-member.ts`, `create-guild-role.ts`
/// and `delete-guild-role.ts` when the caller passes a reason.
fn with_reason(mut req: HttpRequest, args: &Value, key: &str) -> Result<HttpRequest> {
    if let Some(reason) = opt_str(args, key) {
        req = req.header("x-audit-log-reason", header_safe(key, reason)?);
    }
    Ok(req)
}

/// `toColorInteger` from `create-guild-role.ts`.
fn color_integer(raw: &str) -> Result<u64> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        bail!("`role_color` cannot be empty");
    }
    if trimmed.bytes().all(|b| b.is_ascii_digit()) {
        let decimal: u64 = trimmed.parse().map_err(|_| {
            anyhow::anyhow!("Color must be between 0 and {MAX_COLOR_VALUE}, received \"{trimmed}\".")
        })?;
        if decimal > MAX_COLOR_VALUE {
            bail!("Color must be between 0 and {MAX_COLOR_VALUE}, received \"{trimmed}\".");
        }
        return Ok(decimal);
    }
    let hex = trimmed.strip_prefix('#').unwrap_or(trimmed);
    if hex.len() == 6 && hex.bytes().all(|b| b.is_ascii_hexdigit()) {
        if let Ok(value) = u64::from_str_radix(hex, 16) {
            return Ok(value);
        }
    }
    bail!("Color must be a hex value like #5865F2 or a decimal RGB number, received \"{trimmed}\".")
}

// ---------------------------------------------------------------- mappers

/// `discordMessageOutputSchema` of `send-message-with-bot.ts`.
fn flatten_message(m: &Value) -> Result<Value> {
    if m.get("id").and_then(Value::as_str).is_none() {
        bail!("Discord returned no message id");
    }
    let attachments: Vec<Value> = m["attachments"]
        .as_array()
        .map(|a| {
            a.iter()
                .map(|f| {
                    json!({
                        "id": f["id"], "filename": f["filename"], "size": f["size"],
                        "url": f["url"], "content_type": f["content_type"],
                    })
                })
                .collect()
        })
        .unwrap_or_default();
    Ok(json!({
        "id": m["id"],
        "channel_id": m["channel_id"],
        "content": m["content"],
        "timestamp": m["timestamp"],
        "edited_timestamp": m["edited_timestamp"],
        "pinned": m["pinned"],
        "tts": m["tts"],
        "mention_everyone": m["mention_everyone"],
        "author": {
            "id": m["author"]["id"],
            "username": m["author"]["username"],
            "global_name": m["author"]["global_name"],
            "bot": m["author"]["bot"],
        },
        "attachments": attachments,
    }))
}

/// `discordGetChannelActionOutputSchema`; `delete-channel.ts` returns the raw
/// body of the deleted channel.
fn flatten_channel(c: &Value) -> Value {
    json!({
        "id": c["id"],
        "name": c["name"],
        "type": c["type"],
        "guild_id": c["guild_id"],
        "parent_id": c["parent_id"],
        "topic": c["topic"],
        "nsfw": c["nsfw"],
    })
}

/// The body `create-channel.ts` answers with (`{id, name, …}`).
fn flatten_channel_ref(c: &Value) -> Value {
    json!({"id": c["id"], "name": c["name"]})
}

/// The body `create-guild-role.ts` answers with.
fn flatten_role_ref(r: &Value) -> Value {
    json!({"id": r["id"], "name": r["name"]})
}

fn flatten_guild(g: &Value) -> Value {
    json!({
        "id": g["id"], "name": g["name"], "icon": g["icon"], "owner": g["owner"],
        "permissions": g["permissions"], "features": g["features"],
    })
}

fn flatten_list_channel(c: &Value) -> Value {
    json!({
        "id": c["id"], "name": c["name"], "type": c["type"],
        "parent_id": c["parent_id"], "position": c["position"], "topic": c["topic"],
    })
}

fn flatten_role(r: &Value) -> Value {
    json!({
        "id": r["id"], "name": r["name"], "color": r["color"], "hoist": r["hoist"],
        "position": r["position"], "mentionable": r["mentionable"],
    })
}

fn flatten_member(m: &Value) -> Value {
    json!({
        "user_id": m["user"]["id"],
        "username": m["user"]["username"],
        "global_name": m["user"]["global_name"],
        "nick": m["nick"],
        "joined_at": m["joined_at"],
        "roles": m["roles"],
    })
}

fn page(items: Vec<Value>) -> Value {
    json!({"items": items.clone(), "count": items.len()})
}

fn array(response: &HttpResponse) -> Result<Vec<Value>> {
    match response.json()? {
        Value::Array(a) => Ok(a),
        _ => Err(anyhow::anyhow!("Discord returned a non-array response")),
    }
}

/// Discord answers these calls with `204 No Content`.
fn no_content(response: &HttpResponse) -> Result<Value> {
    if response.status == 204 {
        Ok(json!({"success": true}))
    } else {
        Err(anyhow::anyhow!(
            "Discord answered with status {} instead of 204",
            response.status
        ))
    }
}

impl ToolkitAdapter for Discord {
    fn spec(&self) -> &ToolkitSpec {
        &self.spec
    }

    #[allow(clippy::too_many_lines)]
    fn build_request(&self, tool: &str, args: &Value) -> Result<HttpRequest> {
        let req = match tool {
            // AP: src/lib/common/index.ts (`discordCommon.guilds` dropdown)
            "DISCORD_LIST_GUILDS" => {
                HttpRequest::new("GET", format!("{}/users/@me/guilds", self.v9()))
            }
            // AP: src/lib/common/index.ts (`discordCommon.channel` dropdown)
            "DISCORD_LIST_CHANNELS" => HttpRequest::new(
                "GET",
                self.guild_url(path_segment(args, "guild_id")?, "/channels"),
            ),
            // AP: src/lib/common/index.ts (`discordCommon.roles` dropdown)
            "DISCORD_LIST_ROLES" => HttpRequest::new(
                "GET",
                self.guild_url(path_segment(args, "guild_id")?, "/roles"),
            ),
            // AP: src/lib/actions/find-guild-member.ts (`fetchAllMembers`:
            //     `GET /guilds/${guildId}/members` with query params `limit` and `after`)
            "DISCORD_LIST_MEMBERS" => {
                let guild_id = path_segment(args, "guild_id")?;
                let limit = opt_u64(args, "limit", MEMBERS_PAGE_SIZE, 1, MEMBERS_PAGE_SIZE);
                let q = query(&[
                    ("limit", Some(limit.to_string())),
                    ("after", opt_str(args, "after").map(String::from)),
                ]);
                HttpRequest::new("GET", format!("{}{q}", self.guild_url(guild_id, "/members")))
            }
            // AP: src/lib/actions/send-message-with-bot.ts
            //     (`POST https://discord.com/api/v10/channels/${channelId}/messages` with `content`;
            //      the `files[]` multipart variant is not ported)
            "DISCORD_SEND_MESSAGE" => {
                let channel_id = path_segment(args, "channel_id")?;
                let content = req_str(args, "message")?;
                if content.chars().count() > 2000 {
                    bail!("`message` must be at most 2000 characters");
                }
                HttpRequest::new(
                    "POST",
                    format!("{}/channels/{channel_id}/messages", self.v10()),
                )
                .json(&json!({ "content": content }))
            }
            // AP: src/lib/actions/create-channel.ts
            //     (`POST /guilds/${guild_id}/channels`, body `{name, topic}`)
            "DISCORD_CREATE_CHANNEL" => {
                let guild_id = path_segment(args, "guild_id")?;
                let mut body = json!({ "name": req_str(args, "name")?.trim() });
                if let Some(topic) = opt_str(args, "topic") {
                    body["topic"] = json!(topic);
                }
                HttpRequest::new("POST", self.guild_url(guild_id, "/channels")).json(&body)
            }
            // AP: src/lib/actions/rename-channel.ts
            //     (`PATCH /channels/${channel_id}`, body `{name}`)
            "DISCORD_RENAME_CHANNEL" => {
                let channel_id = path_segment(args, "channel_id")?;
                let name = req_str(args, "name")?.trim();
                HttpRequest::new("PATCH", self.channel_url(channel_id)).json(&json!({ "name": name }))
            }
            // AP: src/lib/actions/delete-channel.ts (`DELETE /channels/${channel_id}`)
            "DISCORD_DELETE_CHANNEL" => {
                let channel_id = path_segment(args, "channel_id")?;
                HttpRequest::new("DELETE", self.channel_url(channel_id))
            }
            // AP: src/lib/actions/create-guild-role.ts
            //     (`POST /guilds/${guild_id}/roles`, body `{name, color, hoist, mentionable}`,
            //      header `X-Audit-Log-Reason`)
            "DISCORD_CREATE_ROLE" => {
                let guild_id = path_segment(args, "guild_id")?;
                let mut body = json!({
                    "name": req_str(args, "role_name")?.trim(),
                    "hoist": args.get("display_separated").and_then(Value::as_bool).unwrap_or(false),
                    "mentionable": args.get("role_mentionable").and_then(Value::as_bool).unwrap_or(false),
                });
                if let Some(color) = opt_str(args, "role_color") {
                    body["color"] = json!(color_integer(color)?);
                }
                with_reason(
                    HttpRequest::new("POST", self.guild_url(guild_id, "/roles")).json(&body),
                    args,
                    "creation_reason",
                )?
            }
            // AP: src/lib/actions/delete-guild-role.ts
            //     (`DELETE /guilds/${guild_id}/roles/${role_id}`, header `X-Audit-Log-Reason`)
            "DISCORD_DELETE_ROLE" => {
                let guild_id = path_segment(args, "guild_id")?;
                let role_id = path_segment(args, "role_id")?;
                with_reason(
                    HttpRequest::new(
                        "DELETE",
                        self.guild_url(guild_id, &format!("/roles/{role_id}")),
                    ),
                    args,
                    "deletion_reason",
                )?
            }
            // AP: src/lib/actions/add-role-to-member.ts
            //     (`PUT /guilds/${guild_id}/members/${user_id}/roles/${role_id}`)
            "DISCORD_ADD_ROLE_TO_MEMBER" => {
                let guild_id = path_segment(args, "guild_id")?;
                let user_id = path_segment(args, "user_id")?;
                let role_id = path_segment(args, "role_id")?;
                HttpRequest::new(
                    "PUT",
                    self.guild_url(guild_id, &format!("/members/{user_id}/roles/{role_id}")),
                )
            }
            // AP: src/lib/actions/remove-role-from-member.ts
            //     (`DELETE /guilds/${guild_id}/members/${user_id}/roles/${role_id}`)
            "DISCORD_REMOVE_ROLE_FROM_MEMBER" => {
                let guild_id = path_segment(args, "guild_id")?;
                let user_id = path_segment(args, "user_id")?;
                let role_id = path_segment(args, "role_id")?;
                HttpRequest::new(
                    "DELETE",
                    self.guild_url(guild_id, &format!("/members/{user_id}/roles/{role_id}")),
                )
            }
            // AP: src/lib/actions/remove-member-from-guild.ts
            //     (`DELETE /guilds/${guild_id}/members/${user_id}`)
            "DISCORD_REMOVE_MEMBER_FROM_SERVER" => {
                let guild_id = path_segment(args, "guild_id")?;
                let user_id = path_segment(args, "user_id")?;
                HttpRequest::new("DELETE", self.guild_url(guild_id, &format!("/members/{user_id}")))
            }
            // AP: src/lib/actions/ban-a-guild-member.ts
            //     (`PUT /guilds/${guild_id}/bans/${user_id}`, body `{reason}`,
            //      header `X-Audit-Log-Reason`)
            "DISCORD_BAN_MEMBER" => {
                let guild_id = path_segment(args, "guild_id")?;
                let user_id = path_segment(args, "user_id")?;
                let reason = opt_str(args, "ban_reason").map(|r| header_safe("ban_reason", r));
                let req = with_reason(
                    HttpRequest::new("PUT", self.guild_url(guild_id, &format!("/bans/{user_id}"))),
                    args,
                    "ban_reason",
                )?;
                match reason.transpose()? {
                    Some(r) => req.json(&json!({ "reason": r })),
                    None => req,
                }
            }
            // AP: src/lib/actions/remove-ban-from-user.ts
            //     (`DELETE /guilds/${guild_id}/bans/${user_id}`, body `{reason}`,
            //      header `X-Audit-Log-Reason`)
            "DISCORD_REMOVE_BAN_FROM_USER" => {
                let guild_id = path_segment(args, "guild_id")?;
                let user_id = path_segment(args, "user_id")?;
                let reason = opt_str(args, "unban_reason").map(|r| header_safe("unban_reason", r));
                let req = with_reason(
                    HttpRequest::new(
                        "DELETE",
                        self.guild_url(guild_id, &format!("/bans/{user_id}")),
                    ),
                    args,
                    "unban_reason",
                )?;
                match reason.transpose()? {
                    Some(r) => req.json(&json!({ "reason": r })),
                    None => req,
                }
            }
            other => bail!("unknown Discord tool `{other}`"),
        };
        Ok(req)
    }

    fn parse_response(&self, tool: &str, response: &HttpResponse) -> Result<Value> {
        Ok(match tool {
            // AP: src/lib/common/index.ts (`discordCommon.guilds`: `{id, name, icon, owner, …}`)
            "DISCORD_LIST_GUILDS" => page(array(response)?.iter().map(flatten_guild).collect()),
            // AP: src/lib/common/index.ts (`discordCommon.channel`: `{id, name, …}`)
            "DISCORD_LIST_CHANNELS" => {
                page(array(response)?.iter().map(flatten_list_channel).collect())
            }
            // AP: src/lib/common/index.ts (`discordCommon.roles`: `{id, name, color, hoist, position, mentionable}`)
            "DISCORD_LIST_ROLES" => page(array(response)?.iter().map(flatten_role).collect()),
            // AP: src/lib/actions/find-guild-member.ts (`GuildMember {user: {id, username, global_name}, nick}`)
            "DISCORD_LIST_MEMBERS" => page(array(response)?.iter().map(flatten_member).collect()),
            // AP: src/lib/actions/send-message-with-bot.ts (`res.body` = the created message)
            "DISCORD_SEND_MESSAGE" => flatten_message(&response.json()?)?,
            // AP: src/lib/actions/create-channel.ts (`res.body.id` / `res.body.name`)
            "DISCORD_CREATE_CHANNEL" => {
                let v = response.json()?;
                json!({"success": response.status == 201, "channel": flatten_channel_ref(&v)})
            }
            // AP: src/lib/actions/rename-channel.ts (`res.status >= 200 && < 300`)
            "DISCORD_RENAME_CHANNEL" => {
                json!({"success": (200..300).contains(&response.status)})
            }
            // AP: src/lib/actions/delete-channel.ts (`res.body` = the deleted channel)
            "DISCORD_DELETE_CHANNEL" => flatten_channel(&response.json()?),
            // AP: src/lib/actions/create-guild-role.ts (`res.body.id` / `res.body.name`)
            "DISCORD_CREATE_ROLE" => {
                let v = response.json()?;
                json!({"success": response.status == 201, "role": flatten_role_ref(&v)})
            }
            // AP: src/lib/actions/delete-guild-role.ts (`res.status === 204`)
            "DISCORD_DELETE_ROLE" => no_content(response)?,
            // AP: src/lib/actions/add-role-to-member.ts (`res.status === 204`)
            "DISCORD_ADD_ROLE_TO_MEMBER" => no_content(response)?,
            // AP: src/lib/actions/remove-role-from-member.ts (`res.status === 204`)
            "DISCORD_REMOVE_ROLE_FROM_MEMBER" => no_content(response)?,
            // AP: src/lib/actions/remove-member-from-guild.ts (`res.status === 204`)
            "DISCORD_REMOVE_MEMBER_FROM_SERVER" => no_content(response)?,
            // AP: src/lib/actions/ban-a-guild-member.ts (`res.status === 204`)
            "DISCORD_BAN_MEMBER" => no_content(response)?,
            // AP: src/lib/actions/remove-ban-from-user.ts (`res.status === 204`)
            "DISCORD_REMOVE_BAN_FROM_USER" => no_content(response)?,
            other => bail!("unknown Discord tool `{other}`"),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn d() -> Discord {
        Discord::with_api_base("https://d.test/api")
    }

    #[test]
    fn urls_and_versions() {
        let d = d();
        let r = d.build_request("DISCORD_LIST_GUILDS", &json!({})).unwrap();
        assert_eq!(r.method, "GET");
        assert_eq!(r.url, "https://d.test/api/v9/users/@me/guilds");

        let r = d
            .build_request("DISCORD_LIST_CHANNELS", &json!({"guild_id": "112233445566778899"}))
            .unwrap();
        assert_eq!(r.url, "https://d.test/api/v9/guilds/112233445566778899/channels");

        // `send-message-with-bot.ts` is the only action on v10.
        let r = d
            .build_request(
                "DISCORD_SEND_MESSAGE",
                &json!({"channel_id": "112233445566778899", "message": "hi"}),
            )
            .unwrap();
        assert_eq!(r.url, "https://d.test/api/v10/channels/112233445566778899/messages");

        // Members are paged like `fetchAllMembers`.
        let r = d
            .build_request(
                "DISCORD_LIST_MEMBERS",
                &json!({"guild_id": "112233445566778899", "limit": 2, "after": "99"}),
            )
            .unwrap();
        assert_eq!(
            r.url,
            "https://d.test/api/v9/guilds/112233445566778899/members?limit=2&after=99"
        );
        let r = d
            .build_request("DISCORD_LIST_MEMBERS", &json!({"guild_id": "112233445566778899"}))
            .unwrap();
        assert_eq!(
            r.url,
            "https://d.test/api/v9/guilds/112233445566778899/members?limit=1000"
        );
    }

    #[test]
    fn validation_and_colors() {
        let d = d();
        assert!(d
            .build_request("DISCORD_LIST_CHANNELS", &json!({"guild_id": "../etc"}))
            .is_err());
        assert!(d.build_request("DISCORD_ADD_ROLE_TO_MEMBER", &json!({"guild_id": "1"})).is_err());
        assert!(d
            .build_request("DISCORD_SEND_MESSAGE", &json!({"channel_id": "1", "message": "x".repeat(2001)}))
            .is_err());
        assert!(d.build_request("DISCORD_NOPE", &json!({})).is_err());

        assert_eq!(color_integer("#5865F2").unwrap(), 0x5865f2);
        assert_eq!(color_integer("5793266").unwrap(), 5_793_266);
        assert!(color_integer("#fff").is_err());
        assert!(color_integer("16777216").is_err());
        assert!(color_integer("blau").is_err());
    }

    #[test]
    fn bodies_and_headers() {
        let d = d();
        let r = d
            .build_request(
                "DISCORD_CREATE_ROLE",
                &json!({
                    "guild_id": "112233445566778899", "role_name": "Moderators",
                    "role_color": "#5865F2", "display_separated": true,
                    "role_mentionable": true, "creation_reason": "ticket #12"
                }),
            )
            .unwrap();
        let body: Value = serde_json::from_slice(r.body.as_deref().unwrap()).unwrap();
        assert_eq!(body["name"], "Moderators");
        assert_eq!(body["color"], 0x5865f2);
        assert_eq!(body["hoist"], json!(true));
        assert_eq!(body["mentionable"], json!(true));
        assert_eq!(r.headers["x-audit-log-reason"], "ticket #12");

        // Without a reason neither the header nor a body is sent.
        let r = d
            .build_request(
                "DISCORD_BAN_MEMBER",
                &json!({"guild_id": "112233445566778899", "user_id": "998877665544332211"}),
            )
            .unwrap();
        assert!(r.body.is_none());
        assert!(!r.headers.contains_key("x-audit-log-reason"));

        // CRLF in a header value is rejected.
        assert!(d
            .build_request(
                "DISCORD_BAN_MEMBER",
                &json!({"guild_id": "112233445566778899", "user_id": "998877665544332211",
                        "ban_reason": "spam\r\nX-Evil: 1"}),
            )
            .is_err());
    }
}