//! Trello port: one recorded case per action, expectations derived from the
//! Activepieces `run()` code (`GET/POST/PUT/DELETE` on `https://api.trello.com/1`;
//! `key`/`token` auth params are injected by the hub, never part of a tool call).
mod support;

use ctox_toolhub::toolkits::trello::Trello;
use ctox_toolhub::toolkits::ToolkitAdapter;
use serde_json::{json, Value};
use support::{assert_full_coverage, run, Case};

const BASE: &str = "https://trello.test/1";
const MEMBER: &str = "5ab1c2d3e4f56a7b8c9d0e1f";
const MEMBER2: &str = "5ab1c2d3e4f56a7b8c9d0e2a";
const ORG: &str = "5cd1c2d3e4f56a7b8c9d0e1f";
const BOARD: &str = "614d8a9b1c2d3e4f5a6b7c8d";
const BOARD2: &str = "5e9d8a9b1c2d3e4f5a6b7c8d";
const LIST: &str = "624d8a9b1c2d3e4f5a6b7c8d";
const CARD: &str = "5f4d8a9b1c2d3e4f5a6b7c8d";
const LABEL: &str = "634d8a9b1c2d3e4f5a6b7c8d";
const LABEL2: &str = "634d8a9b1c2d3e4f5a6b7c8e";
const COMMENT: &str = "644d8a9b1c2d3e4f5a6b7c8d";
const CHECKLIST: &str = "654d8a9b1c2d3e4f5a6b7c8d";

fn adapter() -> Trello {
    Trello::with_api_base(BASE)
}

/// `&[&str]` for a `Case` field, leaked so it lives as long as the case
/// (same trick as `ptrs` below for expected body/result pointers).
fn urls<const N: usize>(items: [String; N]) -> &'static [&'static str] {
    let owned: Vec<&'static str> = items
        .into_iter()
        .map(|s| Box::leak(s.into_boxed_str()) as &'static str)
        .collect();
    Box::leak(owned.into_boxed_slice())
}

/// `&[(ptr, value)]` for a `Case` field: leaked so it lives as long as the case.
fn ptrs<const N: usize>(pairs: [(&'static str, Value); N]) -> &'static [(&'static str, Value)] {
    Box::leak(Box::new(pairs))
}

fn member(id: &str, username: &str) -> Value {
    json!({
        "id": id,
        "username": username,
        "fullName": if username == "ada" { "Ada Lovelace" } else { "Grace Hopper" },
        "email": format!("{username}@example.com"),
        "url": format!("https://trello.com/{username}"),
        "avatarUrl": format!("https://trello-members.s3.amazonaws.com/{id}/avatar.png"),
        "bio": "Platform team",
        "confirmed": true,
        "idOrganizations": [ORG]
    })
}

fn board(id: &str, name: &str) -> Value {
    json!({
        "id": id, "name": name, "desc": "Roadmap board", "closed": false,
        "url": format!("https://trello.com/b/{id}/platform"),
        "shortUrl": format!("https://trello.com/b/{id}"),
        "idOrganization": ORG,
        "prefs": {"permissionLevel": "org", "background": "blue"},
        "dateLastActivity": "2026-09-20T08:30:00.000Z"
    })
}

fn list() -> Value {
    json!({
        "id": LIST, "name": "In Progress", "idBoard": BOARD, "closed": false,
        "pos": 65535, "subscribed": false
    })
}

fn card() -> Value {
    json!({
        "id": CARD, "name": "Login page times out on Safari",
        "desc": "Safari shows a spinner forever.", "closed": false,
        "idList": LIST, "idBoard": BOARD,
        "url": "https://trello.com/c/5f4d8a9b/12-login-page-times-out-on-safari",
        "shortUrl": "https://trello.com/c/5f4d8a9b", "pos": 16384,
        "due": "2026-10-01T12:00:00.000Z", "dueComplete": false,
        "labels": [{"id": LABEL, "idBoard": BOARD, "name": "Bug", "color": "red"}],
        "idMembers": [MEMBER],
        "dateLastActivity": "2026-09-21T09:00:00.000Z"
    })
}

fn label(id: &str, name: &str, color: &str) -> Value {
    json!({"id": id, "idBoard": BOARD, "name": name, "color": color})
}

fn comment() -> Value {
    json!({
        "id": COMMENT, "idMemberCreator": MEMBER,
        "data": {"text": "Repro confirmed on Safari 17.4", "card": {"id": CARD, "name": "Login page times out on Safari"}},
        "type": "commentCard", "date": "2026-09-21T09:00:00.000Z",
        "memberCreator": {"id": MEMBER, "username": "ada", "fullName": "Ada Lovelace"}
    })
}

fn checklist() -> Value {
    json!({
        "id": CHECKLIST, "idCard": CARD, "name": "Acceptance",
        "checkItems": [
            {"id": "664d8a9b1c2d3e4f5a6b7c81", "state": "incomplete", "name": "Repro on Safari 17"},
            {"id": "664d8a9b1c2d3e4f5a6b7c82", "state": "complete", "name": "Check Chrome"}
        ]
    })
}

fn attachment() -> Value {
    json!({
        "id": "674d8a9b1c2d3e4f5a6b7c8d", "bytes": null, "isUpload": false,
        "date": "2026-09-19T10:00:00.000Z", "mimeType": null, "name": "Sentry issue",
        "url": "https://sentry.example.com/issues/42", "idMember": MEMBER,
        "edgeColor": null
    })
}

#[test]
fn trello_actions_are_ported() {
    let a = adapter();
    let cases: Vec<Case> = vec![
        // AP: src/lib/actions/ai/get-my-member.ts
        Case {
            tool: "TRELLO_GET_MY_MEMBER",
            args: json!({}),
            method: "GET",
            url_contains: urls([format!("{BASE}/members/me")]),
            body_eq: ptrs([]),
            response_status: 200,
            response: member(MEMBER, "ada"),
            result_eq: ptrs([
                ("/username", json!("ada")),
                ("/fullName", json!("Ada Lovelace")),
                ("/email", json!("ada@example.com")),
                ("/idOrganizations/0", json!(ORG)),
            ]),
        },
        // AP: src/lib/actions/ai/get-member.ts
        Case {
            tool: "TRELLO_GET_MEMBER",
            args: json!({"member_id": "grace"}),
            method: "GET",
            url_contains: urls([format!("{BASE}/members/grace")]),
            body_eq: ptrs([]),
            response_status: 200,
            response: member(MEMBER2, "grace"),
            result_eq: ptrs([("/id", json!(MEMBER2)), ("/username", json!("grace"))]),
        },
        // AP: src/lib/actions/ai/search-members.ts
        Case {
            tool: "TRELLO_SEARCH_MEMBERS",
            args: json!({"query": "ada", "board_id": BOARD, "only_org_members": true, "limit": 5}),
            method: "GET",
            url_contains: urls([format!(
                "{BASE}/search/members?query=ada&idBoard={BOARD}&onlyOrgMembers=true&limit=5"
            )]),
            body_eq: ptrs([]),
            response_status: 200,
            response: json!([member(MEMBER, "ada")]),
            result_eq: ptrs([
                ("/count", json!(1)),
                ("/members/0/id", json!(MEMBER)),
                ("/members/0/fullName", json!("Ada Lovelace")),
            ]),
        },
        // AP: src/lib/actions/ai/search-cards.ts
        Case {
            tool: "TRELLO_SEARCH_CARDS",
            args: json!({"query": "login bug", "board_ids": [BOARD, BOARD2], "limit": 10}),
            method: "GET",
            url_contains: urls([format!(
                "{BASE}/search?query=login+bug&modelTypes=cards\
                 &idBoards={BOARD}%2C{BOARD2}&cards_limit=10"
            )]),
            body_eq: ptrs([]),
            response_status: 200,
            response: json!({
                "cards": [card()],
                "boards": [board(BOARD, "Platform")],
                "members": [],
                "query": "login bug"
            }),
            result_eq: ptrs([
                ("/count", json!(1)),
                ("/cards/0/id", json!(CARD)),
                ("/cards/0/name", json!("Login page times out on Safari")),
            ]),
        },
        // AP: src/lib/actions/ai/list-boards.ts
        Case {
            tool: "TRELLO_LIST_BOARDS",
            args: json!({"filter": "open"}),
            method: "GET",
            url_contains: urls([format!("{BASE}/members/me/boards?filter=open")]),
            body_eq: ptrs([]),
            response_status: 200,
            response: json!([board(BOARD, "Platform"), board(BOARD2, "Infra")]),
            result_eq: ptrs([
                ("/count", json!(2)),
                ("/boards/0/id", json!(BOARD)),
                ("/boards/1/name", json!("Infra")),
            ]),
        },
        // AP: src/lib/actions/ai/get-board.ts
        Case {
            tool: "TRELLO_GET_BOARD",
            args: json!({"board_id": BOARD}),
            method: "GET",
            url_contains: urls([format!("{BASE}/boards/{BOARD}")]),
            body_eq: ptrs([]),
            response_status: 200,
            response: board(BOARD, "Platform"),
            result_eq: ptrs([
                ("/name", json!("Platform")),
                ("/idOrganization", json!(ORG)),
                ("/prefs/permissionLevel", json!("org")),
            ]),
        },
        // AP: src/lib/actions/ai/list-board-labels.ts
        Case {
            tool: "TRELLO_LIST_BOARD_LABELS",
            args: json!({"board_id": BOARD, "limit": 100}),
            method: "GET",
            url_contains: urls([format!("{BASE}/boards/{BOARD}/labels?limit=100")]),
            body_eq: ptrs([]),
            response_status: 200,
            response: json!([label(LABEL, "Bug", "red"), label(LABEL2, "Feature", "green")]),
            result_eq: ptrs([
                ("/count", json!(2)),
                ("/labels/0/name", json!("Bug")),
                ("/labels/1/color", json!("green")),
            ]),
        },
        // AP: src/lib/actions/ai/list-board-members.ts
        Case {
            tool: "TRELLO_LIST_BOARD_MEMBERS",
            args: json!({"board_id": BOARD}),
            method: "GET",
            url_contains: urls([format!("{BASE}/boards/{BOARD}/members")]),
            body_eq: ptrs([]),
            response_status: 200,
            response: json!([member(MEMBER, "ada"), member(MEMBER2, "grace")]),
            result_eq: ptrs([
                ("/count", json!(2)),
                ("/members/0/username", json!("ada")),
                ("/members/1/id", json!(MEMBER2)),
            ]),
        },
        // AP: src/lib/actions/ai/list-lists.ts
        Case {
            tool: "TRELLO_LIST_LISTS",
            args: json!({"board_id": BOARD, "filter": "open"}),
            method: "GET",
            url_contains: urls([format!("{BASE}/boards/{BOARD}/lists?filter=open")]),
            body_eq: ptrs([]),
            response_status: 200,
            response: json!([
                {"id": "624d8a9b1c2d3e4f5a6b7c81", "name": "Todo", "idBoard": BOARD,
                 "closed": false, "pos": 8192},
                list()
            ]),
            result_eq: ptrs([
                ("/count", json!(2)),
                ("/lists/0/name", json!("Todo")),
                ("/lists/1/idBoard", json!(BOARD)),
            ]),
        },
        // AP: src/lib/actions/ai/get-list.ts
        Case {
            tool: "TRELLO_GET_LIST",
            args: json!({"list_id": LIST}),
            method: "GET",
            url_contains: urls([format!("{BASE}/lists/{LIST}")]),
            body_eq: ptrs([]),
            response_status: 200,
            response: list(),
            result_eq: ptrs([
                ("/name", json!("In Progress")),
                ("/idBoard", json!(BOARD)),
                ("/closed", json!(false)),
            ]),
        },
        // AP: src/lib/actions/ai/list-cards-in-list.ts
        Case {
            tool: "TRELLO_LIST_CARDS_IN_LIST",
            args: json!({"list_id": LIST, "filter": "open"}),
            method: "GET",
            url_contains: urls([format!("{BASE}/lists/{LIST}/cards?filter=open")]),
            body_eq: ptrs([]),
            response_status: 200,
            response: json!([card()]),
            result_eq: ptrs([
                ("/count", json!(1)),
                ("/cards/0/id", json!(CARD)),
                ("/cards/0/idList", json!(LIST)),
            ]),
        },
        // AP: src/lib/actions/ai/get-card-ai.ts
        Case {
            tool: "TRELLO_GET_CARD",
            args: json!({"card_id": CARD}),
            method: "GET",
            url_contains: urls([format!("{BASE}/cards/{CARD}")]),
            body_eq: ptrs([]),
            response_status: 200,
            response: card(),
            result_eq: ptrs([
                ("/name", json!("Login page times out on Safari")),
                ("/idList", json!(LIST)),
                ("/labels/0/name", json!("Bug")),
                ("/idMembers/0", json!(MEMBER)),
                ("/due", json!("2026-10-01T12:00:00.000Z")),
            ]),
        },
        // AP: src/lib/actions/ai/list-card-checklists.ts
        Case {
            tool: "TRELLO_LIST_CARD_CHECKLISTS",
            args: json!({"card_id": CARD}),
            method: "GET",
            url_contains: urls([format!("{BASE}/cards/{CARD}/checklists?checkItems=all")]),
            body_eq: ptrs([]),
            response_status: 200,
            response: json!([checklist()]),
            result_eq: ptrs([
                ("/count", json!(1)),
                ("/checklists/0/name", json!("Acceptance")),
                ("/checklists/0/checkItems/1/state", json!("complete")),
            ]),
        },
        // AP: src/lib/actions/ai/list-card-comments.ts
        Case {
            tool: "TRELLO_LIST_CARD_COMMENTS",
            args: json!({"card_id": CARD, "limit": 50}),
            method: "GET",
            url_contains: urls([format!(
                "{BASE}/cards/{CARD}/actions?filter=commentCard&limit=50"
            )]),
            body_eq: ptrs([]),
            response_status: 200,
            response: json!([comment()]),
            result_eq: ptrs([
                ("/count", json!(1)),
                ("/comments/0/id", json!(COMMENT)),
                ("/comments/0/data/text", json!("Repro confirmed on Safari 17.4")),
            ]),
        },
        // AP: src/lib/actions/ai/list-card-attachments-ai.ts
        Case {
            tool: "TRELLO_LIST_CARD_ATTACHMENTS",
            args: json!({"card_id": CARD}),
            method: "GET",
            url_contains: urls([format!("{BASE}/cards/{CARD}/attachments")]),
            body_eq: ptrs([]),
            response_status: 200,
            response: json!([attachment()]),
            result_eq: ptrs([
                ("/count", json!(1)),
                ("/attachments/0/name", json!("Sentry issue")),
                ("/attachments/0/url", json!("https://sentry.example.com/issues/42")),
            ]),
        },
        // AP: src/lib/actions/ai/list-organization-boards.ts
        Case {
            tool: "TRELLO_LIST_ORGANIZATION_BOARDS",
            args: json!({"org_id": ORG}),
            method: "GET",
            url_contains: urls([format!("{BASE}/organizations/{ORG}/boards")]),
            body_eq: ptrs([]),
            response_status: 200,
            response: json!([board(BOARD, "Platform")]),
            result_eq: ptrs([("/count", json!(1)), ("/boards/0/id", json!(BOARD))]),
        },
        // AP: src/lib/actions/ai/create-card-ai.ts
        Case {
            tool: "TRELLO_CREATE_CARD",
            args: json!({
                "list_id": LIST, "name": "Ship login fix",
                "description": "Fixes the Safari spinner.", "position": "top",
                "labels": [LABEL]
            }),
            method: "POST",
            url_contains: urls([format!("{BASE}/cards?idList={LIST}")]),
            body_eq: ptrs([
                ("/name", json!("Ship login fix")),
                ("/desc", json!("Fixes the Safari spinner.")),
                ("/pos", json!("top")),
                ("/idLabels", json!([LABEL])),
            ]),
            response_status: 200,
            response: card(),
            result_eq: ptrs([("/id", json!(CARD)), ("/idList", json!(LIST))]),
        },
        // AP: src/lib/actions/ai/update-card-ai.ts
        Case {
            tool: "TRELLO_UPDATE_CARD",
            args: json!({
                "card_id": CARD, "name": "Ship login fix (Safari 17)",
                "due": "2026-10-01T12:00:00.000Z", "closed": false
            }),
            method: "PUT",
            url_contains: urls([format!("{BASE}/cards/{CARD}")]),
            body_eq: ptrs([
                ("/name", json!("Ship login fix (Safari 17)")),
                ("/due", json!("2026-10-01T12:00:00.000Z")),
                ("/closed", json!(false)),
            ]),
            response_status: 200,
            response: card(),
            result_eq: ptrs([("/name", json!("Login page times out on Safari"))]),
        },
        // AP: src/lib/actions/ai/delete-card-ai.ts
        Case {
            tool: "TRELLO_DELETE_CARD",
            args: json!({"card_id": CARD}),
            method: "DELETE",
            url_contains: urls([format!("{BASE}/cards/{CARD}")]),
            body_eq: ptrs([]),
            response_status: 200,
            response: card(),
            result_eq: ptrs([("/id", json!(CARD)), ("/closed", json!(false))]),
        },
        // AP: src/lib/actions/ai/add-comment-to-card.ts
        Case {
            tool: "TRELLO_ADD_COMMENT_TO_CARD",
            args: json!({"card_id": CARD, "text": "Repro confirmed on Safari 17.4"}),
            method: "POST",
            url_contains: urls([format!(
                "{BASE}/cards/{CARD}/actions/comments?text=Repro+confirmed+on+Safari+17.4"
            )]),
            body_eq: ptrs([]),
            response_status: 200,
            response: comment(),
            result_eq: ptrs([
                ("/id", json!(COMMENT)),
                ("/type", json!("commentCard")),
                ("/data/text", json!("Repro confirmed on Safari 17.4")),
            ]),
        },
        // AP: src/lib/actions/ai/delete-comment.ts
        Case {
            tool: "TRELLO_DELETE_COMMENT",
            args: json!({"card_id": CARD, "comment_id": COMMENT}),
            method: "DELETE",
            url_contains: urls([format!("{BASE}/cards/{CARD}/actions/{COMMENT}/comments")]),
            body_eq: ptrs([]),
            response_status: 200,
            response: json!(null),
            result_eq: ptrs([]),
        },
        // AP: src/lib/actions/ai/add-label-to-card.ts
        Case {
            tool: "TRELLO_ADD_LABEL_TO_CARD",
            args: json!({"card_id": CARD, "label_id": LABEL2}),
            method: "POST",
            url_contains: urls([format!("{BASE}/cards/{CARD}/idLabels?value={LABEL2}")]),
            body_eq: ptrs([]),
            response_status: 200,
            response: card(),
            result_eq: ptrs([("/id", json!(CARD)), ("/labels/0/color", json!("red"))]),
        },
        // AP: src/lib/actions/ai/remove-label-from-card.ts
        Case {
            tool: "TRELLO_REMOVE_LABEL_FROM_CARD",
            args: json!({"card_id": CARD, "label_id": LABEL}),
            method: "DELETE",
            url_contains: urls([format!("{BASE}/cards/{CARD}/idLabels/{LABEL}")]),
            body_eq: ptrs([]),
            response_status: 200,
            response: card(),
            result_eq: ptrs([("/id", json!(CARD))]),
        },
        // AP: src/lib/actions/ai/add-member-to-card.ts
        Case {
            tool: "TRELLO_ADD_MEMBER_TO_CARD",
            args: json!({"card_id": CARD, "member_id": MEMBER2}),
            method: "POST",
            url_contains: urls([format!("{BASE}/cards/{CARD}/idMembers?value={MEMBER2}")]),
            body_eq: ptrs([]),
            response_status: 200,
            response: card(),
            result_eq: ptrs([("/idMembers/0", json!(MEMBER))]),
        },
        // AP: src/lib/actions/ai/create-board-label.ts
        Case {
            tool: "TRELLO_CREATE_BOARD_LABEL",
            args: json!({"board_id": BOARD, "name": "Urgent", "color": "red"}),
            method: "POST",
            url_contains: urls([format!("{BASE}/labels?idBoard={BOARD}&name=Urgent&color=red")]),
            body_eq: ptrs([]),
            response_status: 200,
            response: label("634d8a9b1c2d3e4f5a6b7c9a", "Urgent", "red"),
            result_eq: ptrs([
                ("/name", json!("Urgent")),
                ("/color", json!("red")),
                ("/idBoard", json!(BOARD)),
            ]),
        },
        // AP: src/lib/actions/ai/move-card.ts
        Case {
            tool: "TRELLO_MOVE_CARD",
            args: json!({"card_id": CARD, "list_id": "624d8a9b1c2d3e4f5a6b7c99"}),
            method: "PUT",
            url_contains: urls([format!(
                "{BASE}/cards/{CARD}/idList?value=624d8a9b1c2d3e4f5a6b7c99"
            )]),
            body_eq: ptrs([]),
            response_status: 200,
            response: card(),
            result_eq: ptrs([("/id", json!(CARD)), ("/idList", json!(LIST))]),
        },
        // AP: src/lib/actions/ai/archive-card.ts
        Case {
            tool: "TRELLO_ARCHIVE_CARD",
            args: json!({"card_id": CARD}),
            method: "PUT",
            url_contains: urls([format!("{BASE}/cards/{CARD}/closed?value=true")]),
            body_eq: ptrs([]),
            response_status: 200,
            response: card(),
            result_eq: ptrs([("/id", json!(CARD))]),
        },
        // AP: src/lib/actions/ai/create-list.ts
        Case {
            tool: "TRELLO_CREATE_LIST",
            args: json!({"board_id": BOARD, "name": "Done", "position": "bottom"}),
            method: "POST",
            url_contains: urls([format!("{BASE}/lists?name=Done&idBoard={BOARD}&pos=bottom")]),
            body_eq: ptrs([]),
            response_status: 200,
            response: json!({
                "id": "624d8a9b1c2d3e4f5a6b7c77", "name": "Done", "idBoard": BOARD,
                "closed": false, "pos": 40960, "subscribed": false
            }),
            result_eq: ptrs([("/name", json!("Done")), ("/idBoard", json!(BOARD))]),
        },
        // AP: src/lib/actions/ai/create-board.ts
        Case {
            tool: "TRELLO_CREATE_BOARD",
            args: json!({"name": "Platform", "description": "Roadmap board", "org_id": ORG}),
            method: "POST",
            url_contains: urls([format!(
                "{BASE}/boards?name=Platform&desc=Roadmap+board&idOrganization={ORG}"
            )]),
            body_eq: ptrs([]),
            response_status: 200,
            response: board(BOARD, "Platform"),
            result_eq: ptrs([
                ("/id", json!(BOARD)),
                ("/idOrganization", json!(ORG)),
                ("/desc", json!("Roadmap board")),
            ]),
        },
        // AP: src/lib/actions/ai/add-checklist-to-card.ts
        Case {
            tool: "TRELLO_ADD_CHECKLIST_TO_CARD",
            args: json!({"card_id": CARD, "name": "Acceptance"}),
            method: "POST",
            url_contains: urls([format!("{BASE}/cards/{CARD}/checklists?name=Acceptance")]),
            body_eq: ptrs([]),
            response_status: 200,
            response: checklist(),
            result_eq: ptrs([
                ("/name", json!("Acceptance")),
                ("/idCard", json!(CARD)),
                ("/checkItems/0/state", json!("incomplete")),
            ]),
        },
    ];
    assert_full_coverage(&a, &cases);
    for c in &cases {
        run(&a, c);
    }
}

/// Live smoke test: read-only calls against `https://api.trello.com/1`.
/// `CTOX_TOOLHUB_LIVE_TRELLO_TOKEN` must hold the Trello credential as
/// `api_key:token` (exactly what the piece stores as username/password);
/// the test is skipped when unset and panics on failure.
#[test]
#[ignore]
fn live_read_only() {
    let t = Trello::default();
    // Trello authenticates with the Basic scheme; support::live maps an unknown
    // kind to Bearer, which this toolkit does not offer, so the credential runs
    // through the same hub exec path via live_exec with the Basic scheme.
    for (tool, args) in [
        ("TRELLO_GET_MY_MEMBER", json!({})),
        ("TRELLO_LIST_BOARDS", json!({})),
        ("TRELLO_SEARCH_CARDS", json!({"query": "login"})),
    ] {
        let spec = t
            .spec()
            .tools
            .iter()
            .find(|s| s.slug == tool)
            .expect("tool in catalog");
        assert!(!spec.mutating, "{tool} must be read-only for the live smoke test");
        let Some(result) =
            support::live_exec("trello", ctox_toolhub::AuthSchemeKind::Basic, tool, args)
        else {
            eprintln!("skip: CTOX_TOOLHUB_LIVE_TRELLO_TOKEN not set");
            return;
        };
        let out = result.expect("live call failed");
        assert!(!out.to_string().is_empty(), "{tool}: empty result");
    }
}
