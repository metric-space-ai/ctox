//! Port tests for the Microsoft Teams toolkit (Microsoft Graph v1.0).
//! One recorded case per action; expected requests derived from the TypeScript
//! `run()` code of Activepieces `piece-microsoft-teams` (MIT).
mod support;

use ctox_toolhub::toolkits::teams::Teams;
use ctox_toolhub::toolkits::ToolkitAdapter;
use serde_json::{json, Value};
use support::Case;

const TEAM: &str = "team-1";
const CHANNEL: &str = "19:general@thread.skype";
const CHAT: &str = "19:chat-1@thread.skype";

fn channel(id: &str, name: &str) -> Value {
    json!({
        "id": id,
        "displayName": name,
        "description": "",
        "membershipType": "standard",
        "webUrl": format!("https://teams.microsoft.com/l/channel/{CHANNEL}"),
    })
}

fn chat_message(id: &str, content: &str) -> Value {
    json!({
        "id": id,
        "createdDateTime": "2026-03-04T09:12:00Z",
        "webUrl": format!("https://teams.microsoft.com/l/message/{CHAT}/{id}"),
        "body": {"contentType": "text", "content": content},
        "from": {
            "user": {"id": "user-anna", "displayName": "Anna Muster", "userPrincipalName": "anna@example.com"}
        },
        "mentions": [],
    })
}

#[test]
fn every_action_matches_the_typescript_request() {
    let teams = Teams::default();
    let e0 = [];
    let e1 = [("/id", json!("user-anna")), ("/userPrincipalName", json!("anna@example.com"))];
    let e2 = [];
    let e3 = [("/teams/0/id", json!(TEAM)), ("/teams/1/displayName", json!("Support"))];
    let e4 = [];
    let e5 = [("/channels/0/id", json!(CHANNEL)), ("/channels/1/displayName", json!("Ops"))];
    let e6 = [];
    let e7 = [("/channels/1/id", json!("19:secret@thread.skype"))];
    let e8 = [];
    let e9 = [("/members/0/userId", json!("user-anna")), ("/members/0/displayName", json!("Anna Muster"))];
    let e10 = [];
    let e11 = [("/found", json!(true)), ("/result/0/id", json!(CHANNEL))];
    let e12 = [];
    let e13 = [("/found", json!(true)), ("/result/0/userId", json!("user-anna"))];
    let e14 = [];
    let e15 = [("/messages/0/body/content", json!("Release 1.2 is live"))];
    let e16 = [];
    let e17 = [("/id", json!("1607129931226")), ("/content", json!("Danke!")), ("/from", json!("Anna Muster"))];
    let e18 = [];
    let e19 = [("/messages/0/body/content", json!("Ping?"))];
    let e20 = [];
    let e21 = [
                ("/id", json!("1700")),
                ("/content_type", json!("text")),
                ("/content", json!("Ping?")),
                ("/from", json!("Anna Muster")),
            ];
    let e22 = [];
    let e23 = [
                ("/chats/0/id", json!(CHAT)),
                ("/chats/0/chat_type", json!("group")),
                ("/chats/0/members/1/display_name", json!("Ben Beispiel")),
            ];
    let e24 = [];
    let e25 = [("/meetings/0/id", json!("MSoDcmlbmcyBjb25mZXJlbmNl"))];
    let e26 = [];
    let e27 = [("/recordings/0/id", json!("rec-1")), ("/recordings/0/length", json!(1834000))];
    let e28 = [];
    let e29 = [("/id", json!("rec-1")), ("/length", json!(1834000))];
    let e30 = [];
    let e31 = [("/transcripts/0/id", json!("tr-1"))];
    let e32 = [];
    let e33 = [("/content", json!("WEBVTT\n\n00:00:01.000 --> 00:00:04.000\nAnna: Release 1.2 is live.\n"))];
    let e34 = [
                ("/displayName", json!("Release 1.2")),
                ("/description", json!("Ship channel")),
            ];
    let e35 = [("/id", json!("19:release@thread.skype")), ("/displayName", json!("Release 1.2"))];
    let e36 = [
                ("/displayName", json!("Leads")),
                ("/membershipType", json!("private")),
            ];
    let e37 = [("/membershipType", json!("private")), ("/id", json!("19:leads@thread.skype"))];
    let e38 = [
                ("/body/content", json!("<div><at id=\"0\">Anna Muster</at> please review</div>")),
                ("/body/contentType", json!("html")),
                ("/mentions/0/mentioned/user/id", json!("user-anna")),
            ];
    let e39 = [("/id", json!("1610"))];
    let e40 = [
                ("/body/content", json!("Danke!")),
                ("/body/contentType", json!("text")),
            ];
    let e41 = [("/id", json!("1607129931226")), ("/body/content", json!("Danke!"))];
    let e42 = [
                ("/chatType", json!("group")),
                ("/members/0/@odata.type", json!("#microsoft.graph.aadUserConversationMember")),
                ("/members/0/user@odata.bind", json!("https://graph.microsoft.com/v1.0/users('user-anna')")),
                ("/members/1/user@odata.bind", json!("https://graph.microsoft.com/v1.0/users('user-ben')")),
            ];
    let e43 = [("/id", json!(CHAT)), ("/chatType", json!("group"))];
    let e44 = [
                ("/body/content", json!("Standup in 5 Minuten")),
                ("/body/contentType", json!("text")),
            ];
    let e45 = [("/id", json!("1701"))];
    let e46 = [];
    let e47 = [("/success", json!(true)), ("/status", json!(202))];
    let e48 = [
                ("/body/contentType", json!("html")),
                (
                    "/body/content",
                    json!("Release 1.2 freigeben?<attachment id=\"ctox-approval\"></attachment>"),
                ),
                ("/attachments/0/contentType", json!("application/vnd.microsoft.card.adaptive")),
            ];
    let e49 = [("/approved", json!(false)), ("/message_id", json!("1611"))];
    let e50 = [
                ("/attachments/0/id", json!("att-42")),
                (
                    "/body/content",
                    json!("Bitte freigeben<attachment id=\"att-42\"></attachment>"),
                ),
            ];
    let e51 = [("/approved", json!(false)), ("/message_id", json!("1702"))];
    let cases = vec![
        // ---- identity & discovery -------------------------------------------------
        Case {
            tool: "TEAMS_GET_ME",
            args: json!({}),
            method: "GET",
            url_contains: &["/v1.0/me"],
            body_eq: &e0,
            response_status: 200,
            response: json!({
                "@odata.context": "https://graph.microsoft.com/v1.0/$metadata#users/$entity",
                "id": "user-anna",
                "displayName": "Anna Muster",
                "userPrincipalName": "anna@example.com",
                "mail": "anna@example.com",
                "jobTitle": "Automation Lead",
            }),
            result_eq: &e1,
        },
        Case {
            tool: "TEAMS_LIST_MY_TEAMS",
            args: json!({}),
            method: "GET",
            url_contains: &["/v1.0/me/joinedTeams"],
            body_eq: &e2,
            response_status: 200,
            response: json!({
                "value": [
                    {"id": TEAM, "displayName": "Platform", "description": "Platform team"},
                    {"id": "team-2", "displayName": "Support", "description": ""},
                ],
            }),
            result_eq: &e3,
        },
        Case {
            tool: "TEAMS_LIST_CHANNELS",
            args: json!({"team_id": TEAM}),
            method: "GET",
            url_contains: &["/v1.0/teams/team-1/channels"],
            body_eq: &e4,
            response_status: 200,
            response: json!({"value": [channel(CHANNEL, "General"), channel("19:ops@thread.skype", "Ops")]}),
            result_eq: &e5,
        },
        Case {
            tool: "TEAMS_LIST_ALL_CHANNELS",
            args: json!({"team_id": TEAM}),
            method: "GET",
            url_contains: &["/v1.0/teams/team-1/allChannels"],
            body_eq: &e6,
            response_status: 200,
            response: json!({"value": [channel(CHANNEL, "General"), channel("19:secret@thread.skype", "Secret")] }),
            result_eq: &e7,
        },
        Case {
            tool: "TEAMS_LIST_TEAM_MEMBERS",
            args: json!({"team_id": TEAM}),
            method: "GET",
            url_contains: &["/v1.0/teams/team-1/members"],
            body_eq: &e8,
            response_status: 200,
            response: json!({
                "value": [
                    {"id": "member-1", "displayName": "Anna Muster", "userId": "user-anna",
                     "userPrincipalName": "anna@example.com", "email": "anna@example.com"},
                ],
            }),
            result_eq: &e9,
        },
        Case {
            // find-channel.ts: client.api(`/teams/${teamId}/allChannels`).filter(`displayName eq '...'`)
            tool: "TEAMS_FIND_CHANNEL",
            args: json!({"team_id": TEAM, "channel_name": "General"}),
            method: "GET",
            url_contains: &[
                "/v1.0/teams/team-1/allChannels",
                "$filter=displayName+eq+%27General%27",
            ],
            body_eq: &e10,
            response_status: 200,
            response: json!({"value": [channel(CHANNEL, "General")]}),
            result_eq: &e11,
        },
        Case {
            // find-team-member.ts: filter on aadUserConversationMember/email
            tool: "TEAMS_FIND_TEAM_MEMBER",
            args: json!({"team_id": TEAM, "search_by": "email", "search_value": "anna@example.com"}),
            method: "GET",
            url_contains: &[
                "/v1.0/teams/team-1/members",
                "aadUserConversationMember%2Femail+eq+%27anna%40example.com%27",
            ],
            body_eq: &e12,
            response_status: 200,
            response: json!({
                "value": [{"id": "member-1", "displayName": "Anna Muster", "userId": "user-anna",
                           "email": "anna@example.com"}],
            }),
            result_eq: &e13,
        },
        Case {
            tool: "TEAMS_LIST_CHANNEL_MESSAGES",
            args: json!({"team_id": TEAM, "channel_id": CHANNEL}),
            method: "GET",
            url_contains: &["/v1.0/teams/team-1/channels/19:general@thread.skype/messages", "$top=20"],
            body_eq: &e14,
            response_status: 200,
            response: json!({
                "value": [chat_message("1600", "Release 1.2 is live")],
                "@odata.nextLink": "https://graph.microsoft.com/v1.0/teams/team-1/channels/19:general@thread.skype/messages?$skiptoken=X",
            }),
            result_eq: &e15,
        },
        Case {
            // get-channel-message.ts with replyId -> .../messages/{id}/replies/{replyId}
            tool: "TEAMS_GET_CHANNEL_MESSAGE",
            args: json!({
                "team_id": TEAM,
                "channel_id": CHANNEL,
                "message_id": "1607129931225",
                "reply_id": "1607129931226",
            }),
            method: "GET",
            url_contains: &[
                "/v1.0/teams/team-1/channels/19:general@thread.skype/messages/1607129931225/replies/1607129931226",
            ],
            body_eq: &e16,
            response_status: 200,
            response: chat_message("1607129931226", "Danke!"),
            result_eq: &e17,
        },
        Case {
            tool: "TEAMS_LIST_CHAT_MESSAGES",
            args: json!({"chat_id": CHAT, "top": 5}),
            method: "GET",
            url_contains: &["/v1.0/chats/19:chat-1@thread.skype/messages", "$top=5"],
            body_eq: &e18,
            response_status: 200,
            response: json!({"value": [chat_message("1700", "Ping?")]}),
            result_eq: &e19,
        },
        Case {
            tool: "TEAMS_GET_CHAT_MESSAGE",
            args: json!({"chat_id": CHAT, "message_id": "1700"}),
            method: "GET",
            url_contains: &["/v1.0/chats/19:chat-1@thread.skype/messages/1700"],
            body_eq: &e20,
            response_status: 200,
            response: chat_message("1700", "Ping?"),
            result_eq: &e21,
        },
        // common/index.ts chatId dropdown: /chats with $expand=members and page size 50
        Case {
            tool: "TEAMS_LIST_CHATS",
            args: json!({}),
            method: "GET",
            url_contains: &["/v1.0/chats", "$expand=members", "$top=50"],
            body_eq: &e22,
            response_status: 200,
            response: json!({
                "value": [
                    {
                        "id": CHAT,
                        "chatType": "group",
                        "topic": "Release 1.2",
                        "createdDateTime": "2026-03-01T08:00:00Z",
                        "members": [
                            {"displayName": "Anna Muster", "user": {"id": "user-anna"}},
                            {"displayName": "Ben Beispiel", "user": {"id": "user-ben"}},
                        ],
                    },
                ],
            }),
            result_eq: &e23,
        },
        // graph.ts resolveMeetingId(): client.api('/me/onlineMeetings').filter(...)
        Case {
            tool: "TEAMS_FIND_MEETING",
            args: json!({
                "identifier_type": "joinWebUrl",
                "identifier_value": "https://teams.microsoft.com/l/meetup-join/19%3ameeting_abc",
            }),
            method: "GET",
            url_contains: &[
                "/v1.0/me/onlineMeetings",
                "JoinWebUrl+eq+%27https%3A%2F%2Fteams.microsoft.com%2Fl%2Fmeetup-join%2F19%253ameeting_abc%27",
            ],
            body_eq: &e24,
            response_status: 200,
            response: json!({
                "value": [{
                    "id": "MSoDcmlbmcyBjb25mZXJlbmNl",
                    "createdDateTime": "2026-03-02T10:00:00Z",
                    "joinWebUrl": "https://teams.microsoft.com/l/meetup-join/19%3ameeting_abc",
                    "subject": "Weekly sync",
                }],
            }),
            result_eq: &e25,
        },
        // ---- meeting recordings & transcripts --------------------------------------
        Case {
            tool: "TEAMS_LIST_MEETING_RECORDINGS",
            args: json!({"meeting_id": "MSoDcmlbmcyBjb25mZXJlbmNl", "top": 10}),
            method: "GET",
            url_contains: &["/v1.0/me/onlineMeetings/MSoDcmlbmcyBjb25mZXJlbmNl/recordings", "$top=10"],
            body_eq: &e26,
            response_status: 200,
            response: json!({
                "value": [{
                    "id": "rec-1",
                    "createdDateTime": "2026-03-02T10:30:00Z",
                    "contentUrl": "https://graph.microsoft.com/v1.0/me/onlineMeetings/MSoDcmlbmcyBjb25mZXJlbmNl/recordings/rec-1/content",
                    "length": 1834000,
                }],
            }),
            result_eq: &e27,
        },
        Case {
            tool: "TEAMS_GET_MEETING_RECORDING",
            args: json!({"meeting_id": "MSoDcmlbmcyBjb25mZXJlbmNl", "recording_id": "rec-1"}),
            method: "GET",
            url_contains: &["/v1.0/me/onlineMeetings/MSoDcmlbmcyBjb25mZXJlbmNl/recordings/rec-1"],
            body_eq: &e28,
            response_status: 200,
            response: json!({
                "id": "rec-1",
                "createdDateTime": "2026-03-02T10:30:00Z",
                "length": 1834000,
            }),
            result_eq: &e29,
        },
        Case {
            tool: "TEAMS_LIST_MEETING_TRANSCRIPTS",
            args: json!({"meeting_id": "MSoDcmlbmcyBjb25mZXJlbmNl"}),
            method: "GET",
            url_contains: &["/v1.0/me/onlineMeetings/MSoDcmlbmcyBjb25mZXJlbmNl/transcripts"],
            body_eq: &e30,
            response_status: 200,
            response: json!({
                "value": [{"id": "tr-1", "createdDateTime": "2026-03-02T10:31:00Z"}],
            }),
            result_eq: &e31,
        },
        Case {
            // get-meeting-transcript.ts: .header('Accept','text/vtt').responseType(TEXT)
            tool: "TEAMS_GET_MEETING_TRANSCRIPT",
            args: json!({"meeting_id": "MSoDcmlbmcyBjb25mZXJlbmNl", "transcript_id": "tr-1"}),
            method: "GET",
            url_contains: &["/v1.0/me/onlineMeetings/MSoDcmlbmcyBjb25mZXJlbmNl/transcripts/tr-1/content"],
            body_eq: &e32,
            response_status: 200,
            response: json!("WEBVTT\n\n00:00:01.000 --> 00:00:04.000\nAnna: Release 1.2 is live.\n"),
            result_eq: &e33,
        },
        // ---- write actions ---------------------------------------------------------
        Case {
            // create-channel.ts: client.api(`/teams/${teamId}/channels`).post({displayName, description})
            tool: "TEAMS_CREATE_CHANNEL",
            args: json!({
                "team_id": TEAM,
                "channel_display_name": "Release 1.2",
                "channel_description": "Ship channel",
            }),
            method: "POST",
            url_contains: &["/v1.0/teams/team-1/channels"],
            body_eq: &e34,
            response_status: 201,
            response: channel("19:release@thread.skype", "Release 1.2"),
            result_eq: &e35,
        },
        Case {
            // create-private-channel.ts adds membershipType: 'private'
            tool: "TEAMS_CREATE_PRIVATE_CHANNEL",
            args: json!({"team_id": TEAM, "channel_display_name": "Leads"}),
            method: "POST",
            url_contains: &["/v1.0/teams/team-1/channels"],
            body_eq: &e36,
            response_status: 201,
            response: json!({"id": "19:leads@thread.skype", "displayName": "Leads", "membershipType": "private"}),
            result_eq: &e37,
        },
        Case {
            // send-channel-message.ts posts {body:{content, contentType:'html'}} (+ mentions)
            tool: "TEAMS_SEND_CHANNEL_MESSAGE",
            args: json!({
                "team_id": TEAM,
                "channel_id": CHANNEL,
                "content": "<div><at id=\"0\">Anna Muster</at> please review</div>",
                "content_type": "html",
                "mentions": [
                    {"id": 0, "mentionText": "Anna Muster",
                     "mentioned": {"user": {"id": "user-anna", "displayName": "Anna Muster"}}},
                ],
            }),
            method: "POST",
            url_contains: &["/v1.0/teams/team-1/channels/19:general@thread.skype/messages"],
            body_eq: &e38,
            response_status: 201,
            response: json!({
                "id": "1610",
                "createdDateTime": "2026-03-04T09:30:00Z",
                "body": {"contentType": "html", "content": "<div>please review</div>"},
            }),
            result_eq: &e39,
        },
        Case {
            // reply-to-channel-message.ts
            tool: "TEAMS_REPLY_TO_CHANNEL_MESSAGE",
            args: json!({
                "team_id": TEAM,
                "channel_id": CHANNEL,
                "message_id": "1607129931225",
                "content": "Danke!",
            }),
            method: "POST",
            url_contains: &[
                "/v1.0/teams/team-1/channels/19:general@thread.skype/messages/1607129931225/replies",
            ],
            body_eq: &e40,
            response_status: 201,
            response: chat_message("1607129931226", "Danke!"),
            result_eq: &e41,
        },
        Case {
            // create-chat-and-send-message.ts: POST /chats with aadUserConversationMember binds
            tool: "TEAMS_CREATE_CHAT",
            args: json!({"member_ids": ["user-anna", "user-ben"]}),
            method: "POST",
            url_contains: &["/v1.0/chats"],
            body_eq: &e42,
            response_status: 201,
            response: json!({"id": CHAT, "chatType": "group", "topic": null}),
            result_eq: &e43,
        },
        Case {
            // send-chat-message.ts
            tool: "TEAMS_SEND_CHAT_MESSAGE",
            args: json!({"chat_id": CHAT, "content": "Standup in 5 Minuten"}),
            method: "POST",
            url_contains: &["/v1.0/chats/19:chat-1@thread.skype/messages"],
            body_eq: &e44,
            response_status: 201,
            response: chat_message("1701", "Standup in 5 Minuten"),
            result_eq: &e45,
        },
        Case {
            // delete-chat-message.ts: POST /users/{me.id}/chats/{chatId}/messages/{messageId}/softDelete
            tool: "TEAMS_DELETE_CHAT_MESSAGE",
            args: json!({"user_id": "user-anna", "chat_id": CHAT, "message_id": "1701"}),
            method: "POST",
            url_contains: &["/v1.0/users/user-anna/chats/19:chat-1@thread.skype/messages/1701/softDelete"],
            body_eq: &e46,
            response_status: 202,
            response: json!(null),
            result_eq: &e47,
        },
        Case {
            // request-approval-channel-message.ts: adaptive card + <attachment id>
            tool: "TEAMS_REQUEST_APPROVAL_IN_CHANNEL",
            args: json!({
                "team_id": TEAM,
                "channel_id": CHANNEL,
                "message": "Release 1.2 freigeben?",
                "confirmation_url": "https://ctox.test/confirm?action=approve",
            }),
            method: "POST",
            url_contains: &["/v1.0/teams/team-1/channels/19:general@thread.skype/messages"],
            body_eq: &e48,
            response_status: 201,
            response: chat_message("1611", "Release 1.2 freigeben?"),
            result_eq: &e49,
        },
        Case {
            // request-approval-direct-message.ts posts the same card into /chats/{chatId}/messages
            tool: "TEAMS_REQUEST_APPROVAL_DIRECT_MESSAGE",
            args: json!({
                "chat_id": CHAT,
                "message": "Bitte freigeben",
                "confirmation_url": "https://ctox.test/confirm?action=approve",
                "attachment_id": "att-42",
            }),
            method: "POST",
            url_contains: &["/v1.0/chats/19:chat-1@thread.skype/messages"],
            body_eq: &e50,
            response_status: 201,
            response: chat_message("1702", "Bitte freigeben"),
            result_eq: &e51,
        },
    ];
    support::assert_full_coverage(&teams, &cases);
    for c in &cases {
        support::run(&teams, c);
    }
}

#[test]
fn api_base_is_overridable_for_tests() {
    let teams = Teams::with_api_base("https://graph.microsoft.us");
    let req = teams
        .build_request("TEAMS_LIST_MY_TEAMS", &json!({}))
        .unwrap();
    assert_eq!(req.url, "https://graph.microsoft.us/v1.0/me/joinedTeams");
}

/// Live smoke test: read-only calls with `CTOX_TOOLHUB_LIVE_MICROSOFT_TEAMS_TOKEN`.
#[test]
#[ignore = "needs a real Graph token"]
fn live_read_only() {
    let teams = Teams::default();
    for (tool, args) in [
        ("TEAMS_GET_ME", json!({})),
        ("TEAMS_LIST_MY_TEAMS", json!({})),
        ("TEAMS_LIST_CHATS", json!({"top": 3})),
    ] {
        let Some(res) = support::live(&teams, tool, args) else {
            eprintln!("no live token set, skipping {tool}");
            return;
        };
        res.unwrap_or_else(|e| panic!("{tool} failed: {e}"));
    }
}