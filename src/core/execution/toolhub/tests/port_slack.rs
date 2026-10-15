//! Port tests for the Slack toolkit: one recorded case per ported action.
//! DE: erwartete Methode/URL/Body aus dem Activepieces-`run()`-Code abgeleitet.
//! EN: expected method/URL/body derived from the Activepieces `run()` code.
mod support;

use ctox_toolhub::toolkits::slack::Slack;
use ctox_toolhub::toolkits::ToolkitAdapter;
use serde_json::json;
use support::{assert_full_coverage, live, run, Case};

/// Promote a literal to `&'static` so the recorded cases can be statics-free but
/// still outlive the `Case` list.
fn st<T>(v: T) -> &'static T {
    Box::leak(Box::new(v))
}

fn adapter() -> Slack {
    Slack::with_api_base("https://slack.test/api")
}

/// Recorded Slack Web API payloads (shape taken from the real `slack.com/api`
/// responses the TypeScript actions consume).
fn cases() -> Vec<Case<'static>> {
    vec![
        Case {
            tool: "SLACK_LIST_CHANNELS",
            args: json!({}),
            method: "GET",
            url_contains: st([
                "/conversations.list",
                "types=public_channel&",
                "limit=200",
                "exclude_archived=true",
            ]),
            body_eq: st([]),
            response_status: 200,
            response: json!({"ok": true, "channels": [
                {"id": "C0123ABC", "name": "general", "is_private": false, "is_member": true, "num_members": 42},
                {"id": "G0123DEF", "name": "deployments", "is_private": true, "is_member": false, "num_members": 7}
            ], "response_metadata": {"next_cursor": "dGVhbTpDMDYxRkE1UEI="}}),
            result_eq: st([
                ("/channels/0/name", json!("general")),
                ("/channels/1/is_private", json!(true)),
                ("/next_cursor", json!("dGVhbTpDMDYxRkE1UEI=")),
            ]),
        },
        Case {
            tool: "SLACK_GET_CHANNEL_INFO",
            args: json!({"channel": "C0123ABC", "include_num_members": true}),
            method: "GET",
            url_contains: st([
                "/conversations.info",
                "channel=C0123ABC",
                "include_num_members=true",
            ]),
            body_eq: st([]),
            response_status: 200,
            response: json!({"ok": true, "channel": {"id": "C0123ABC", "name": "general",
                "is_private": false, "is_archived": false, "is_member": true, "num_members": 42,
                "topic": {"value": "Allgemeines", "creator": "U1", "last_set": 1717171717},
                "purpose": {"value": "Company-wide", "creator": "U1", "last_set": 1717171717}}}),
            result_eq: st([
                ("/channel/id", json!("C0123ABC")),
                ("/channel/num_members", json!(42)),
            ]),
        },
        Case {
            tool: "SLACK_GET_HISTORY",
            args: json!({"channel": "C0123ABC", "limit": 2}),
            method: "GET",
            url_contains: st(["/conversations.history", "channel=C0123ABC", "limit=2"]),
            body_eq: st([]),
            response_status: 200,
            response: json!({"ok": true, "messages": [
                {"type": "message", "user": "U01", "text": "Hallo Welt", "ts": "1717171717.000100"},
                {"type": "message", "user": "U02", "text": "Release 1.2.3 ist live", "ts": "1717171718.000200",
                 "thread_ts": "1717171710.000000", "reply_count": 3}
            ], "has_more": true, "response_metadata": {"next_cursor": "bWVzc2FnZTox"}}),
            result_eq: st([
                ("/messages/0/text", json!("Hallo Welt")),
                ("/messages/1/reply_count", json!(3)),
                ("/has_more", json!(true)),
            ]),
        },
        Case {
            tool: "SLACK_GET_MESSAGE",
            args: json!({"channel": "C0123ABC", "ts": "1717171718.000200"}),
            method: "GET",
            url_contains: st([
                "/conversations.history",
                "channel=C0123ABC",
                "oldest=1717171718.000200",
                "limit=1",
                "inclusive=true",
            ]),
            body_eq: st([]),
            response_status: 200,
            response: json!({"ok": true, "messages": [
                {"type": "message", "user": "U02", "text": "Release 1.2.3 ist live", "ts": "1717171718.000200"}
            ], "has_more": false}),
            result_eq: st([
                ("/ts", json!("1717171718.000200")),
                ("/text", json!("Release 1.2.3 ist live")),
            ]),
        },
        Case {
            tool: "SLACK_GET_THREAD",
            args: json!({"channel": "C0123ABC", "thread_ts": "1717171710.000000", "limit": 50}),
            method: "GET",
            url_contains: st([
                "/conversations.replies",
                "channel=C0123ABC",
                "ts=1717171710.000000",
                "limit=50",
            ]),
            body_eq: st([]),
            response_status: 200,
            response: json!({"ok": true, "messages": [
                {"user": "U01", "text": "Deploy läuft?", "ts": "1717171710.000000", "reply_count": 2},
                {"user": "U02", "text": "Ja, 2 Minuten noch.", "ts": "1717171711.000000", "thread_ts": "1717171710.000000"}
            ], "has_more": false, "response_metadata": {"next_cursor": ""}}),
            result_eq: st([
                ("/messages/0/text", json!("Deploy läuft?")),
                ("/messages/1/thread_ts", json!("1717171710.000000")),
            ]),
        },
        Case {
            tool: "SLACK_LIST_CHANNEL_MEMBERS",
            args: json!({"channel": "C0123ABC"}),
            method: "GET",
            url_contains: st(["/conversations.members", "channel=C0123ABC", "limit=1000"]),
            body_eq: st([]),
            response_status: 200,
            response: json!({"ok": true, "members": ["U01", "U02", "U03"], "response_metadata": {"next_cursor": ""}}),
            result_eq: st([("/members/0", json!("U01")), ("/count", json!(3))]),
        },
        Case {
            tool: "SLACK_LIST_USER_CONVERSATIONS",
            args: json!({"user": "U01", "types": "public_channel,private_channel", "limit": 100}),
            method: "GET",
            url_contains: st([
                "/users.conversations",
                "user=U01",
                "types=public_channel%2Cprivate_channel",
                "limit=100",
            ]),
            body_eq: st([]),
            response_status: 200,
            response: json!({"ok": true, "channels": [
                {"id": "C0123ABC", "name": "general", "is_private": false, "is_archived": false, "is_member": true},
                {"id": "D04XYZ", "name": "directmessage", "is_private": true, "is_archived": false, "is_im": true}
            ], "response_metadata": {"next_cursor": ""}}),
            result_eq: st([("/channels/0/name", json!("general")), ("/channels/1/id", json!("D04XYZ"))]),
        },
        Case {
            tool: "SLACK_LIST_SCHEDULED_MESSAGES",
            args: json!({"channel": "C0123ABC", "latest": "1730000000", "limit": 100}),
            method: "GET",
            url_contains: st([
                "/chat.scheduledMessages.list",
                "channel=C0123ABC",
                "latest=1730000000",
                "limit=100",
            ]),
            body_eq: st([]),
            response_status: 200,
            response: json!({"ok": true, "scheduled_messages": [
                {"id": "Q0123", "channel_id": "C0123ABC", "post_at": 1729000000,
                 "text": "Wartungsfenster beginnt"}
            ], "response_metadata": {"next_cursor": ""}}),
            result_eq: st([
                ("/scheduled_messages/0/id", json!("Q0123")),
                ("/scheduled_messages/0/post_at", json!(1729000000)),
            ]),
        },
        Case {
            tool: "SLACK_LIST_USERS",
            args: json!({"limit": 100}),
            method: "GET",
            url_contains: st(["/users.list", "limit=100"]),
            body_eq: st([]),
            response_status: 200,
            response: json!({"ok": true, "members": [
                {"id": "U01", "name": "mkh", "real_name": "Max Mustermann", "is_bot": false,
                 "is_admin": true, "deleted": false, "tz": "Europe/Berlin",
                 "profile": {"display_name": "mkh", "email": "max@example.com"}},
                {"id": "B01", "name": "deploybot", "real_name": "", "is_bot": true, "deleted": false}
            ], "response_metadata": {"next_cursor": ""}}),
            result_eq: st([
                ("/users/0/id", json!("U01")),
                ("/users/0/display_name", json!("mkh")),
                ("/users/1/is_bot", json!(true)),
            ]),
        },
        Case {
            tool: "SLACK_GET_USER",
            args: json!({"user": "U01"}),
            method: "GET",
            url_contains: st(["/users.info", "user=U01"]),
            body_eq: st([]),
            response_status: 200,
            response: json!({"ok": true, "user": {"id": "U01", "name": "mkh", "real_name": "Max Mustermann",
                "is_bot": false, "is_admin": true, "deleted": false, "tz": "Europe/Berlin",
                "profile": {"display_name": "mkh", "real_name": "Max Mustermann", "email": "max@example.com"}}}),
            result_eq: st([("/id", json!("U01")), ("/display_name", json!("mkh")), ("/tz", json!("Europe/Berlin"))]),
        },
        Case {
            tool: "SLACK_FIND_USER_BY_EMAIL",
            args: json!({"email": "max@example.com"}),
            method: "GET",
            url_contains: st(["/users.lookupByEmail", "email=max%40example.com"]),
            body_eq: st([]),
            response_status: 200,
            response: json!({"ok": true, "user": {"id": "U01", "name": "mkh", "real_name": "Max Mustermann",
                "is_bot": false, "is_admin": true, "deleted": false, "tz": "Europe/Berlin",
                "profile": {"display_name": "mkh", "email": "max@example.com"}}}),
            result_eq: st([("/id", json!("U01")), ("/name", json!("mkh"))]),
        },
        Case {
            tool: "SLACK_LIST_USER_GROUPS",
            args: json!({"include_users": true}),
            method: "GET",
            url_contains: st(["/usergroups.list", "include_users=true"]),
            body_eq: st([]),
            response_status: 200,
            response: json!({"ok": true, "usergroups": [
                {"id": "S01", "handle": "oncall", "name": "On-Call", "description": "Pager",
                 "user_count": 4, "users": ["U01", "U02"]},
                {"id": "S02", "handle": "releases", "name": "Releases", "user_count": 2}
            ]}),
            result_eq: st([
                ("/usergroups/0/handle", json!("oncall")),
                ("/usergroups/0/user_count", json!(4)),
            ]),
        },
        Case {
            tool: "SLACK_LIST_USER_GROUP_USERS",
            args: json!({"usergroup": "S01", "include_disabled": false}),
            method: "GET",
            url_contains: st(["/usergroups.users.list", "usergroup=S01", "include_disabled=false"]),
            body_eq: st([]),
            response_status: 200,
            response: json!({"ok": true, "users": ["U01", "U02", "U03", "U04"]}),
            result_eq: st([("/users/3", json!("U04")), ("/count", json!(4))]),
        },
        Case {
            tool: "SLACK_SEARCH_MESSAGES",
            args: json!({"query": "in:general deploy", "count": 20}),
            method: "GET",
            url_contains: st([
                "/search.messages",
                "query=in%3Ageneral+deploy",
                "count=20",
                "page=1",
            ]),
            body_eq: st([]),
            response_status: 200,
            response: json!({"ok": true, "messages": {"total": 1, "pagination": {"page": 1, "total_pages": 1},
                "matches": [{"type": "message", "ts": "1717171718.000200", "text": "Deploy erfolgreich",
                    "username": "mkh", "channel": {"name": "general", "id": "C0123ABC"},
                    "permalink": "https://acme.slack.com/archives/C0123ABC/p1717171718000200"}]}}),
            result_eq: st([
                ("/total", json!(1)),
                ("/matches/0/text", json!("Deploy erfolgreich")),
                ("/matches/0/channel", json!("general")),
            ]),
        },
        Case {
            tool: "SLACK_SEARCH_ALL",
            args: json!({"query": "release notes", "count": 100}),
            method: "GET",
            url_contains: st(["/search.all", "query=release+notes", "count=100"]),
            body_eq: st([]),
            response_status: 200,
            response: json!({"ok": true,
                "messages": {"total": 2, "matches": [
                    {"type": "message", "ts": "1717171718.000200", "text": "Release Notes 1.2.3"}]},
                "files": {"total": 1, "matches": [
                    {"type": "file", "id": "F01", "name": "release-notes-1.2.3.pdf", "title": "Release Notes"}]}}),
            result_eq: st([
                ("/message_count", json!(2)),
                ("/file_count", json!(1)),
                ("/messages/0/text", json!("Release Notes 1.2.3")),
            ]),
        },
        Case {
            tool: "SLACK_GET_MESSAGE_PERMALINK",
            args: json!({"channel": "C0123ABC", "message_ts": "1717171718.000200"}),
            method: "GET",
            url_contains: st([
                "/chat.getPermalink",
                "channel=C0123ABC",
                "message_ts=1717171718.000200",
            ]),
            body_eq: st([]),
            response_status: 200,
            response: json!({"ok": true, "channel": "C0123ABC",
                "permalink": "https://acme.slack.com/archives/C0123ABC/p1717171718000200"}),
            result_eq: st([
                ("/permalink", json!("https://acme.slack.com/archives/C0123ABC/p1717171718000200")),
                ("/channel", json!("C0123ABC")),
            ]),
        },
        Case {
            tool: "SLACK_LIST_FILES",
            args: json!({"channel": "C0123ABC", "count": 50, "page": 1}),
            method: "GET",
            url_contains: st(["/files.list", "channel=C0123ABC", "count=50", "page=1"]),
            body_eq: st([]),
            response_status: 200,
            response: json!({"ok": true, "files": [
                {"id": "F01", "name": "release-notes-1.2.3.pdf", "title": "Release Notes 1.2.3",
                 "mimetype": "application/pdf", "permalink": "https://files.slack.com/files-pri/T01-F01/download"}
            ], "paging": {"page": 1, "pages": 1, "total": 1}}),
            result_eq: st([
                ("/files/0/name", json!("release-notes-1.2.3.pdf")),
                ("/paging/total", json!(1)),
            ]),
        },
        // ---------------- write ----------------
        Case {
            tool: "SLACK_SEND_MESSAGE",
            args: json!({"channel": "C0123ABC", "text": "Deployment erfolgreich",
                         "thread_ts": "1717171710.000000"}),
            method: "POST",
            url_contains: st(["/chat.postMessage"]),
            body_eq: st([
                ("/channel", json!("C0123ABC")),
                ("/text", json!("Deployment erfolgreich")),
                ("/thread_ts", json!("1717171710.000000")),
                ("/blocks/0/type", json!("section")),
                ("/blocks/0/text/type", json!("mrkdwn")),
            ]),
            response_status: 200,
            response: json!({"ok": true, "channel": "C0123ABC", "ts": "1717171720.000300",
                "message": {"text": "Deployment erfolgreich"}}),
            result_eq: st([
                ("/ts", json!("1717171720.000300")),
                ("/channel", json!("C0123ABC")),
            ]),
        },
        Case {
            tool: "SLACK_SEND_EPHEMERAL_MESSAGE",
            args: json!({"channel": "C0123ABC", "user": "U01", "text": "Nur für dich"}),
            method: "POST",
            url_contains: st(["/chat.postEphemeral"]),
            body_eq: st([
                ("/channel", json!("C0123ABC")),
                ("/user", json!("U01")),
                ("/text", json!("Nur für dich")),
                ("/blocks/0/text/text", json!("Nur für dich")),
            ]),
            response_status: 200,
            response: json!({"ok": true, "message_ts": "1717171721.000400"}),
            result_eq: st([("/message_ts", json!("1717171721.000400"))]),
        },
        Case {
            tool: "SLACK_SCHEDULE_MESSAGE",
            args: json!({"channel": "C0123ABC", "post_at": 1730000000, "text": "Wartungsfenster",
                         "thread_ts": "1717171710.000000"}),
            method: "POST",
            url_contains: st(["/chat.scheduleMessage"]),
            body_eq: st([
                ("/channel", json!("C0123ABC")),
                ("/post_at", json!(1730000000)),
                ("/text", json!("Wartungsfenster")),
                ("/thread_ts", json!("1717171710.000000")),
            ]),
            response_status: 200,
            response: json!({"ok": true, "channel": "C0123ABC", "scheduled_message_id": "Q0123",
                "post_at": 1730000000}),
            result_eq: st([
                ("/scheduled_message_id", json!("Q0123")),
                ("/post_at", json!(1730000000)),
            ]),
        },
        Case {
            tool: "SLACK_UPDATE_MESSAGE",
            args: json!({"channel": "C0123ABC", "ts": "1717171718.000200", "text": "Release 1.2.4"}),
            method: "POST",
            url_contains: st(["/chat.update"]),
            body_eq: st([
                ("/channel", json!("C0123ABC")),
                ("/ts", json!("1717171718.000200")),
                ("/text", json!("Release 1.2.4")),
            ]),
            response_status: 200,
            response: json!({"ok": true, "channel": "C0123ABC", "ts": "1717171718.000200",
                "text": "Release 1.2.4", "message": {"text": "Release 1.2.4"}}),
            result_eq: st([
                ("/ts", json!("1717171718.000200")),
                ("/text", json!("Release 1.2.4")),
            ]),
        },
        Case {
            tool: "SLACK_DELETE_MESSAGE",
            args: json!({"channel": "C0123ABC", "ts": "1717171718.000200"}),
            method: "POST",
            url_contains: st(["/chat.delete"]),
            body_eq: st([("/channel", json!("C0123ABC")), ("/ts", json!("1717171718.000200"))]),
            response_status: 200,
            response: json!({"ok": true}),
            result_eq: st([("/ok", json!(true))]),
        },
        Case {
            tool: "SLACK_ADD_REACTION",
            args: json!({"channel": "C0123ABC", "ts": "1717171718.000200", "name": ":tada:"}),
            method: "POST",
            url_contains: st(["/reactions.add"]),
            body_eq: st([
                ("/channel", json!("C0123ABC")),
                ("/timestamp", json!("1717171718.000200")),
                ("/name", json!("tada")),
            ]),
            response_status: 200,
            response: json!({"ok": true}),
            result_eq: st([("/ok", json!(true))]),
        },
        Case {
            tool: "SLACK_REMOVE_REACTION",
            args: json!({"channel": "C0123ABC", "ts": "1717171718.000200", "name": "tada"}),
            method: "POST",
            url_contains: st(["/reactions.remove"]),
            body_eq: st([
                ("/channel", json!("C0123ABC")),
                ("/timestamp", json!("1717171718.000200")),
                ("/name", json!("tada")),
            ]),
            response_status: 200,
            response: json!({"ok": true}),
            result_eq: st([("/ok", json!(true))]),
        },
        Case {
            tool: "SLACK_JOIN_CHANNEL",
            args: json!({"channel": "C0123ABC"}),
            method: "POST",
            url_contains: st(["/conversations.join"]),
            body_eq: st([("/channel", json!("C0123ABC"))]),
            response_status: 200,
            response: json!({"ok": true, "channel": {"id": "C0123ABC", "name": "general"}}),
            result_eq: st([("/ok", json!(true))]),
        },
        Case {
            tool: "SLACK_LEAVE_CHANNEL",
            args: json!({"channel": "C0123ABC"}),
            method: "POST",
            url_contains: st(["/conversations.leave"]),
            body_eq: st([("/channel", json!("C0123ABC"))]),
            response_status: 200,
            response: json!({"ok": true}),
            result_eq: st([("/ok", json!(true))]),
        },
        Case {
            tool: "SLACK_CREATE_CHANNEL",
            args: json!({"name": "deployments", "is_private": true}),
            method: "POST",
            url_contains: st(["/conversations.create"]),
            body_eq: st([("/name", json!("deployments")), ("/is_private", json!(true))]),
            response_status: 200,
            response: json!({"ok": true, "channel": {"id": "C09NEW", "name": "deployments",
                "is_private": true, "is_member": true, "num_members": 1}}),
            result_eq: st([
                ("/channel/id", json!("C09NEW")),
                ("/channel/name", json!("deployments")),
            ]),
        },
        Case {
            tool: "SLACK_ARCHIVE_CHANNEL",
            args: json!({"channel": "C0123ABC"}),
            method: "POST",
            url_contains: st(["/conversations.archive"]),
            body_eq: st([("/channel", json!("C0123ABC"))]),
            response_status: 200,
            response: json!({"ok": true}),
            result_eq: st([("/ok", json!(true))]),
        },
        Case {
            tool: "SLACK_UNARCHIVE_CHANNEL",
            args: json!({"channel": "C0123ABC"}),
            method: "POST",
            url_contains: st(["/conversations.unarchive"]),
            body_eq: st([("/channel", json!("C0123ABC"))]),
            response_status: 200,
            response: json!({"ok": true}),
            result_eq: st([("/ok", json!(true))]),
        },
        Case {
            tool: "SLACK_RENAME_CHANNEL",
            args: json!({"channel": "C0123ABC", "name": "releases"}),
            method: "POST",
            url_contains: st(["/conversations.rename"]),
            body_eq: st([("/channel", json!("C0123ABC")), ("/name", json!("releases"))]),
            response_status: 200,
            response: json!({"ok": true, "channel": {"id": "C0123ABC", "name": "releases",
                "is_private": false, "is_member": true, "num_members": 42}}),
            result_eq: st([("/channel/name", json!("releases"))]),
        },
        Case {
            tool: "SLACK_SET_CHANNEL_TOPIC",
            args: json!({"channel": "C0123ABC", "topic": "Releases und Deployments"}),
            method: "POST",
            url_contains: st(["/conversations.setTopic"]),
            body_eq: st([
                ("/channel", json!("C0123ABC")),
                ("/topic", json!("Releases und Deployments")),
            ]),
            response_status: 200,
            response: json!({"ok": true}),
            result_eq: st([("/ok", json!(true))]),
        },
        Case {
            tool: "SLACK_SET_CHANNEL_PURPOSE",
            args: json!({"channel": "C0123ABC", "purpose": "Deployment-Koordination"}),
            method: "POST",
            url_contains: st(["/conversations.setPurpose"]),
            body_eq: st([
                ("/channel", json!("C0123ABC")),
                ("/purpose", json!("Deployment-Koordination")),
            ]),
            response_status: 200,
            response: json!({"ok": true}),
            result_eq: st([("/ok", json!(true))]),
        },
        Case {
            tool: "SLACK_INVITE_USER",
            args: json!({"channel": "C0123ABC", "user": "U01"}),
            method: "POST",
            url_contains: st(["/conversations.invite"]),
            body_eq: st([("/channel", json!("C0123ABC")), ("/users", json!("U01"))]),
            response_status: 200,
            response: json!({"ok": true, "channel": {"id": "C0123ABC", "name": "general"}}),
            result_eq: st([("/ok", json!(true))]),
        },
        Case {
            tool: "SLACK_REMOVE_USER_FROM_CHANNEL",
            args: json!({"channel": "C0123ABC", "user": "U01"}),
            method: "POST",
            url_contains: st(["/conversations.kick"]),
            body_eq: st([("/channel", json!("C0123ABC")), ("/user", json!("U01"))]),
            response_status: 200,
            response: json!({"ok": true}),
            result_eq: st([("/ok", json!(true))]),
        },
        Case {
            tool: "SLACK_MARK_CONVERSATION_READ",
            args: json!({"channel": "C0123ABC", "ts": "1717171718.000200"}),
            method: "POST",
            url_contains: st(["/conversations.mark"]),
            body_eq: st([("/channel", json!("C0123ABC")), ("/ts", json!("1717171718.000200"))]),
            response_status: 200,
            response: json!({"ok": true}),
            result_eq: st([("/ok", json!(true))]),
        },
    ]
}

#[test]
fn slack_actions_are_ported() {
    let a = adapter();
    let cases = cases();
    for c in &cases {
        run(&a, c);
    }
    assert_full_coverage(&a, &cases);
}

#[test]
fn catalog_metadata_is_complete() {
    for t in &adapter().spec().tools {
        assert!(!t.title.de.is_empty() && !t.title.en.is_empty(), "{}", t.slug);
        assert!(!t.description.de.is_empty() && !t.description.en.is_empty(), "{}", t.slug);
        assert!(!t.scopes.is_empty(), "no scopes for {}", t.slug);
    }
    let a = adapter();
    let mut mutating: Vec<&str> = a
        .spec()
        .tools
        .iter()
        .filter(|t| t.mutating)
        .map(|t| t.slug.as_str())
        .collect();
    mutating.sort_unstable();
    assert_eq!(
        mutating,
        [
            "SLACK_ADD_REACTION",
            "SLACK_ARCHIVE_CHANNEL",
            "SLACK_CREATE_CHANNEL",
            "SLACK_DELETE_MESSAGE",
            "SLACK_INVITE_USER",
            "SLACK_JOIN_CHANNEL",
            "SLACK_LEAVE_CHANNEL",
            "SLACK_MARK_CONVERSATION_READ",
            "SLACK_REMOVE_REACTION",
            "SLACK_REMOVE_USER_FROM_CHANNEL",
            "SLACK_RENAME_CHANNEL",
            "SLACK_SCHEDULE_MESSAGE",
            "SLACK_SEND_EPHEMERAL_MESSAGE",
            "SLACK_SEND_MESSAGE",
            "SLACK_SET_CHANNEL_PURPOSE",
            "SLACK_SET_CHANNEL_TOPIC",
            "SLACK_UNARCHIVE_CHANNEL",
            "SLACK_UPDATE_MESSAGE",
        ]
    );
}

/// Slack reports failures as HTTP 200 + `ok:false`; that must surface as an error.
#[test]
fn slack_error_envelope_is_an_error() {
    let a = adapter();
    let resp = ctox_toolhub::http::HttpResponse {
        status: 200,
        body: br#"{"ok":false,"error":"missing_scope"}"#.to_vec(),
        session_id: None,
    };
    let err = a
        .parse_response("SLACK_GET_HISTORY", &resp)
        .unwrap_err()
        .to_string();
    assert!(err.contains("missing_scope"), "{err}");
}

/// Path/query injection through ID arguments is rejected before any request is built.
#[test]
fn ids_are_validated() {
    let a = adapter();
    for (tool, args) in [
        ("SLACK_GET_HISTORY", json!({"channel": "C1&types=x"})),
        ("SLACK_GET_HISTORY", json!({"channel": "../../conversations.open"})),
        ("SLACK_JOIN_CHANNEL", json!({"channel": "C1 2"})),
        ("SLACK_MARK_CONVERSATION_READ", json!({"channel": "C1", "ts": "1.2/3"})),
        ("SLACK_GET_USER", json!({"user": ""})),
        ("SLACK_GET_CHANNEL_INFO", json!({})),
        ("SLACK_SEND_MESSAGE", json!({"channel": "C1", "thread_ts": "a&b"})),
        ("NOPE", json!({})),
    ] {
        assert!(a.build_request(tool, &args).is_err(), "{tool} accepted {args}");
    }
    // Empty search query is rejected (req_str trims).
    assert!(a
        .build_request("SLACK_SEARCH_ALL", &json!({"query": "  "}))
        .is_err());
}

/// Live read-only calls (bot token). Skipped unless CTOX_TOOLHUB_LIVE_SLACK_TOKEN is set.
/// Run: CTOX_TOOLHUB_LIVE_SLACK_TOKEN=xoxb-… cargo test --test port_slack -- --ignored
#[test]
#[ignore]
fn live_read_only_calls() {
    let a = Slack::default();
    for (tool, args) in [
        ("SLACK_LIST_CHANNELS", json!({"limit": 5})),
        ("SLACK_LIST_USERS", json!({"limit": 5})),
        ("SLACK_LIST_USER_GROUPS", json!({})),
    ] {
        let Some(res) = live(&a, tool, args) else {
            eprintln!("CTOX_TOOLHUB_LIVE_SLACK_TOKEN not set; skipping live {tool}");
            return;
        };
        println!("{tool}: {}", res.unwrap());
    }
}