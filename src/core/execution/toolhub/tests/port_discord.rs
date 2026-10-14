//! Discord port: one recorded case per tool. Every expectation is derived from
//! the Activepieces `run()` code (`src/lib/actions/*.ts`, `src/lib/common/index.ts`):
//! the URLs are the ones the piece sends, the API versions (v9 / v10) and the
//! `X-Audit-Log-Reason` headers are the ones it sets.
mod support;

use ctox_toolhub::toolkits::discord::Discord;
use serde_json::{json, Value};
use support::{live, run, Case};

const GUILDS_URL: &str = "https://discord.test/api/v9/users/@me/guilds";
const GUILD: &str = "112233445566778899";
const GUILD2: &str = "223344556677889900";
const CHANNELS_URL: &str = "https://discord.test/api/v9/guilds/112233445566778899/channels";
const ROLES_URL: &str = "https://discord.test/api/v9/guilds/112233445566778899/roles";
const MEMBERS_URL: &str = "https://discord.test/api/v9/guilds/112233445566778899/members";
const CHANNEL: &str = "334455667788990011";
const ROLE: &str = "556677889900112233";
const USER: &str = "667788990011223344";
const OUTSIDER: &str = "778899001122334455";
const BOT: &str = "998877665544332211";

fn adapter() -> Discord {
    Discord::with_api_base("https://discord.test/api")
}

/// `&[(ptr, value)]` for a `Case` field: leaked so it lives as long as the case.
fn ptrs<const N: usize>(pairs: [(&'static str, Value); N]) -> &'static [(&'static str, Value)] {
    Box::leak(Box::new(pairs))
}

fn guild(id: &str, name: &str) -> Value {
    json!({
        "id": id, "name": name, "icon": "a1b2c3d4e5f6a1b2c3d4e5f6a1b2c3d4", "owner": false,
        "permissions": "137439266304", "features": ["COMMUNITY", "NEWS"],
        "description": "Acme Community", "verification_level": 1, "member_count": 4821,
    })
}

/// Recorded `GET /guilds/{id}/channels` entry.
fn channel(id: &str, name: &str, kind: u64) -> Value {
    json!({
        "id": id, "type": kind, "guild_id": GUILD, "name": name, "position": 0,
        "parent_id": Value::Null, "last_message_id": null, "rate_limit_per_user": 0,
        "topic": Value::Null, "nft": false, "nsfw": false, "permission_overwrites": [],
    })
}

#[test]
fn discord_actions_are_ported() {
    let a = adapter();
    let cases: Vec<Case> = vec![
        Case {
            tool: "DISCORD_LIST_GUILDS",
            args: json!({}),
            method: "GET",
            url_contains: &[GUILDS_URL],
            body_eq: ptrs([]),
            response_status: 200,
            response: json!([
                guild(GUILD, "Acme Community"),
                guild(GUILD2, "Acme Dev")
            ]),
            result_eq: ptrs([
                ("/count", json!(2)),
                ("/items/0/name", json!("Acme Community")),
                ("/items/1/id", json!(GUILD2)),
            ]),
        },
        Case {
            tool: "DISCORD_LIST_CHANNELS",
            args: json!({"guild_id": GUILD}),
            method: "GET",
            url_contains: &[CHANNELS_URL],
            body_eq: ptrs([]),
            response_status: 200,
            response: json!([
                channel(CHANNEL, "general", 0),
                channel("334455667788990012", "project-updates", 0),
                channel("334455667788990013", "Staff", 4)
            ]),
            result_eq: ptrs([
                ("/count", json!(3)),
                ("/items/0/id", json!(CHANNEL)),
                ("/items/0/name", json!("general")),
                ("/items/2/type", json!(4)),
            ]),
        },
        Case {
            tool: "DISCORD_LIST_ROLES",
            args: json!({"guild_id": GUILD}),
            method: "GET",
            url_contains: &[ROLES_URL],
            body_eq: ptrs([]),
            response_status: 200,
            response: json!([
                {"id": "445566778899001122", "name": "@everyone", "color": 0, "hoist": false,
                 "position": 0, "permissions": "137439266304", "mentionable": false},
                {"id": ROLE, "name": "Moderators", "color": 5793266, "hoist": true,
                 "position": 3, "permissions": "268435456", "mentionable": false}
            ]),
            result_eq: ptrs([
                ("/count", json!(2)),
                ("/items/1/name", json!("Moderators")),
                ("/items/1/color", json!(5793266)),
                ("/items/1/hoist", json!(true)),
            ]),
        },
        Case {
            tool: "DISCORD_LIST_MEMBERS",
            args: json!({"guild_id": GUILD, "limit": 2, "after": "667788990011223300"}),
            method: "GET",
            url_contains: &[MEMBERS_URL, "limit=2", "after=667788990011223300"],
            body_eq: ptrs([]),
            response_status: 200,
            response: json!([
                {"user": {"id": USER, "username": "ada", "global_name": "Ada Lovelace",
                          "bot": false, "avatar": "1a2b3c4d5e6f"},
                 "nick": "Countess", "roles": ["556677889900112233"], "joined_at": "2025-01-04T12:00:00.000Z"},
                {"user": {"id": OUTSIDER, "username": "grace", "global_name": null,
                          "bot": false, "avatar": null},
                 "nick": null, "roles": ["445566778899001122"], "joined_at": "2025-06-21T08:30:00.000Z"}
            ]),
            result_eq: ptrs([
                ("/count", json!(2)),
                ("/items/0/user_id", json!(USER)),
                ("/items/0/username", json!("ada")),
                ("/items/0/global_name", json!("Ada Lovelace")),
                ("/items/0/nick", json!("Countess")),
                ("/items/1/global_name", Value::Null),
            ]),
        },
        Case {
            tool: "DISCORD_SEND_MESSAGE",
            args: json!({"channel_id": CHANNEL, "message": "Deploy 2026.10.14 finished ✅"}),
            method: "POST",
            // `send-message-with-bot.ts` is the only action on API v10.
            url_contains: &["https://discord.test/api/v10/channels/334455667788990011/messages"],
            body_eq: ptrs([("/content", json!("Deploy 2026.10.14 finished ✅"))]),
            response_status: 200,
            response: json!({
                "id": "889900112233445566", "type": 0,
                "content": "Deploy 2026.10.14 finished ✅", "channel_id": CHANNEL,
                "author": {"id": BOT, "username": "Deploy Bot", "global_name": "Deploy Bot",
                           "bot": true, "avatar": "9f8e7d6c5b4a"},
                "attachments": [], "embeds": [], "mention_everyone": false,
                "mentions": [], "mention_roles": [], "pinned": false, "tts": false,
                "timestamp": "2026-10-14T09:12:33.000Z", "edited_timestamp": null,
                "flags": 0
            }),
            result_eq: ptrs([
                ("/id", json!("889900112233445566")),
                ("/channel_id", json!(CHANNEL)),
                ("/content", json!("Deploy 2026.10.14 finished ✅")),
                ("/author/username", json!("Deploy Bot")),
                ("/author/bot", json!(true)),
                ("/attachments", json!([])),
            ]),
        },
        Case {
            tool: "DISCORD_CREATE_CHANNEL",
            args: json!({"guild_id": GUILD, "name": "project-updates", "topic": "Wöchentliche Releases"}),
            method: "POST",
            url_contains: &[CHANNELS_URL],
            body_eq: ptrs([
                ("/name", json!("project-updates")),
                ("/topic", json!("Wöchentliche Releases")),
            ]),
            response_status: 201,
            response: json!({
                "id": "334455667788990014", "type": 0, "guild_id": GUILD,
                "name": "project-updates", "position": 3, "parent_id": null,
                "topic": "Wöchentliche Releases", "nsfw": false, "rate_limit_per_user": 0,
                "permission_overwrites": []
            }),
            result_eq: ptrs([
                ("/success", json!(true)),
                ("/channel/id", json!("334455667788990014")),
                ("/channel/name", json!("project-updates")),
            ]),
        },
        Case {
            tool: "DISCORD_RENAME_CHANNEL",
            args: json!({"channel_id": CHANNEL, "name": "general-chat"}),
            method: "PATCH",
            url_contains: &["https://discord.test/api/v9/channels/334455667788990011"],
            body_eq: ptrs([("/name", json!("general-chat"))]),
            response_status: 200,
            response: json!({
                "id": CHANNEL, "type": 0, "guild_id": GUILD, "name": "general-chat",
                "position": 0, "parent_id": null, "topic": null, "nsfw": false
            }),
            result_eq: ptrs([("/success", json!(true))]),
        },
        Case {
            tool: "DISCORD_DELETE_CHANNEL",
            args: json!({"channel_id": "334455667788990013"}),
            method: "DELETE",
            url_contains: &["https://discord.test/api/v9/channels/334455667788990013"],
            body_eq: ptrs([]),
            response_status: 200,
            // `delete-channel.ts` returns the raw body of the deleted channel.
            response: json!({
                "id": "334455667788990013", "type": 4, "guild_id": GUILD,
                "name": "staff", "position": 2, "parent_id": null, "topic": null, "nsfw": false
            }),
            result_eq: ptrs([
                ("/id", json!("334455667788990013")),
                ("/name", json!("staff")),
                ("/type", json!(4)),
                ("/guild_id", json!(GUILD)),
            ]),
        },
        Case {
            tool: "DISCORD_CREATE_ROLE",
            args: json!({
                "guild_id": GUILD, "role_name": "Moderators", "role_color": "#5865F2",
                "display_separated": true, "role_mentionable": false,
                "creation_reason": "ticket #12"
            }),
            method: "POST",
            url_contains: &[ROLES_URL],
            body_eq: ptrs([
                ("/name", json!("Moderators")),
                // `toColorInteger` turns #5865F2 into 5793266.
                ("/color", json!(5793266)),
                ("/hoist", json!(true)),
                ("/mentionable", json!(false)),
            ]),
            response_status: 201,
            response: json!({
                "id": "556677889900112244", "name": "Moderators", "color": 5793266,
                "hoist": true, "position": 4,
                "permissions": "268435456", "managed": false, "mentionable": false,
                "icon": null, "unicode_emoji": null, "flags": 0
            }),
            result_eq: ptrs([
                ("/success", json!(true)),
                ("/role/id", json!("556677889900112244")),
                ("/role/name", json!("Moderators")),
            ]),
        },
        Case {
            tool: "DISCORD_DELETE_ROLE",
            args: json!({"guild_id": GUILD, "role_id": ROLE, "deletion_reason": "obsolete"}),
            method: "DELETE",
            url_contains: &["https://discord.test/api/v9/guilds/112233445566778899/roles/556677889900112233"],
            body_eq: ptrs([]),
            response_status: 204,
            response: json!({}),
            result_eq: ptrs([("/success", json!(true))]),
        },
        Case {
            tool: "DISCORD_ADD_ROLE_TO_MEMBER",
            args: json!({"guild_id": GUILD, "user_id": USER, "role_id": ROLE}),
            method: "PUT",
            url_contains: &[
                "https://discord.test/api/v9/guilds/112233445566778899/members/667788990011223344/roles/556677889900112233",
            ],
            body_eq: ptrs([]),
            response_status: 204,
            response: json!({}),
            result_eq: ptrs([("/success", json!(true))]),
        },
        Case {
            tool: "DISCORD_REMOVE_ROLE_FROM_MEMBER",
            args: json!({"guild_id": GUILD, "user_id": USER, "role_id": ROLE}),
            method: "DELETE",
            url_contains: &[
                "https://discord.test/api/v9/guilds/112233445566778899/members/667788990011223344/roles/556677889900112233",
            ],
            body_eq: ptrs([]),
            response_status: 204,
            response: json!({}),
            result_eq: ptrs([("/success", json!(true))]),
        },
        Case {
            tool: "DISCORD_REMOVE_MEMBER_FROM_SERVER",
            args: json!({"guild_id": GUILD, "user_id": OUTSIDER}),
            method: "DELETE",
            url_contains: &[
                "https://discord.test/api/v9/guilds/112233445566778899/members/778899001122334455",
            ],
            body_eq: ptrs([]),
            response_status: 204,
            response: json!({}),
            result_eq: ptrs([("/success", json!(true))]),
        },
        Case {
            tool: "DISCORD_BAN_MEMBER",
            args: json!({"guild_id": GUILD, "user_id": OUTSIDER, "ban_reason": "spam"}),
            method: "PUT",
            url_contains: &["https://discord.test/api/v9/guilds/112233445566778899/bans/778899001122334455"],
            body_eq: ptrs([("/reason", json!("spam"))]),
            response_status: 204,
            response: json!({}),
            result_eq: ptrs([("/success", json!(true))]),
        },
        Case {
            tool: "DISCORD_REMOVE_BAN_FROM_USER",
            args: json!({"guild_id": GUILD, "user_id": OUTSIDER}),
            method: "DELETE",
            url_contains: &["https://discord.test/api/v9/guilds/112233445566778899/bans/778899001122334455"],
            body_eq: ptrs([]),
            response_status: 204,
            response: json!({}),
            result_eq: ptrs([("/success", json!(true))]),
        },
    ];
    support::assert_full_coverage(&a, &cases);
    for c in &cases {
        run(&a, c);
    }
}

#[test]
fn ids_are_validated_and_audit_reasons_are_header_safe() {
    use ctox_toolhub::toolkits::ToolkitAdapter;

    let a = adapter();
    for (tool, args) in [
        ("DISCORD_LIST_CHANNELS", json!({"guild_id": "../../users/@me"})),
        ("DISCORD_BAN_MEMBER", json!({"guild_id": GUILD, "user_id": "1 2"})),
        (
            "DISCORD_BAN_MEMBER",
            json!({"guild_id": GUILD, "user_id": OUTSIDER, "ban_reason": "spam\r\nX-Evil: 1"}),
        ),
        ("DISCORD_CREATE_ROLE", json!({"guild_id": GUILD, "role_name": "Mods", "role_color": "chartreuse"})),
    ] {
        assert!(
            a.build_request(tool, &args).is_err(),
            "{tool} accepted {args}"
        );
    }

    // Without a reason neither `X-Audit-Log-Reason` nor a body is sent.
    let req = a
        .build_request("DISCORD_REMOVE_BAN_FROM_USER", &json!({"guild_id": GUILD, "user_id": OUTSIDER}))
        .unwrap();
    assert!(req.body.is_none());
    assert!(!req.headers.contains_key("x-audit-log-reason"));
}

/// Live read-only calls through the hub exec path (bot token in
/// `Authorization: Bot <token>`).
#[test]
#[ignore]
fn live_read_only_actions() {
    let a = adapter();
    let Some(guilds) = live(&a, "DISCORD_LIST_GUILDS", json!({})) else {
        return;
    };
    let guilds = guilds.unwrap_or_else(|e| panic!("DISCORD_LIST_GUILDS: {e}"));
    let Some(first) = guilds["items"].as_array().and_then(|i| i.first()) else {
        return;
    };
    let guild_id = first["id"].clone();
    for (tool, args) in [
        ("DISCORD_LIST_CHANNELS", json!({"guild_id": guild_id.clone()})),
        ("DISCORD_LIST_ROLES", json!({"guild_id": guild_id})),
    ] {
        let Some(result) = live(&a, tool, args) else { return };
        let v = result.unwrap_or_else(|e| panic!("{tool}: {e}"));
        assert!(v.is_object(), "{tool}: expected an object, got {v}");
    }
}