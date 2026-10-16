//! Todoist port: one recorded case per tool. Every method, path and query/body
//! field is taken from the Activepieces piece
//! (`lib/common/client/rest-client.ts`, `labels-client.ts`,
//! `projects-sections-client.ts` and the `lib/actions/*.ts` that call
//! `httpClient` directly). Base URL `https://api.todoist.com/api/v1`.
mod support;

use ctox_toolhub::toolkits::todoist::Todoist;
use ctox_toolhub::toolkits::ToolkitAdapter;
use serde_json::{json, Value};
use support::{live, run, Case};

const API: &str = "https://todoist.test/api/v1";

fn adapter() -> Todoist {
    Todoist::with_api_base(API)
}

/// Todoist ids are the 16 character base32 form.
const PROJECT: &str = "6X4Vv2hfXjqWf3wj";
const SECTION: &str = "6X7rM8997g3RQmvh";
const TASK: &str = "6X8rd0997g4SRwix";
const SUBTASK: &str = "6X9te1aa8h5TSXjy";
const LABEL: &str = "6X5Ww3giYkrVg4xk";
const COMMENT: &str = "2992679862";
const USER: &str = "2671355";

/// `&[(ptr, value)]` for a `Case` field: leaked so it lives as long as the case.
fn ptrs<const N: usize>(pairs: [(&'static str, Value); N]) -> &'static [(&'static str, Value)] {
    Box::leak(Box::new(pairs))
}

fn page(items: Value) -> Value {
    json!({"results": items, "next_cursor": null})
}

fn task() -> Value {
    json!({
        "id": TASK, "project_id": PROJECT, "section_id": SECTION, "parent_id": null,
        "content": "Buy milk", "description": "Two litres of semi-skimmed",
        "is_completed": false, "labels": ["errands"], "added_by_uid": USER,
        "assigner_id": null, "responsible_uid": USER, "comment_count": 1,
        "created_at": "2026-10-01T09:12:00.000Z", "updated_at": "2026-10-02T08:00:00.000Z",
        "due": {"date": "2026-10-15", "string": "15 Oct", "is_recurring": false,
                "datetime": null, "timezone": null},
        "priority": 3, "url": format!("https://app.todoist.com/app/task/{TASK}"),
        "order": 1, "day_order": -1, "added_at": "2026-10-01T09:12:00.000Z",
        "checked": false, "is_deleted": false
    })
}

fn comment() -> Value {
    json!({
        "id": COMMENT, "task_id": TASK, "project_id": null, "content": "Oat milk is on sale",
        "posted_at": "2026-10-02T08:00:00.000Z", "attachment": null
    })
}

#[test]
fn todoist_actions_are_ported() {
    let a = adapter();
    let cases: Vec<Case> = vec![
        Case {
            tool: "TODOIST_GET_CURRENT_USER",
            args: json!({}),
            method: "GET",
            url_contains: &[API, "/api/v1/user"],
            body_eq: ptrs([]),
            response_status: 200,
            response: json!({
                "id": USER, "email": "ada@example.com", "full_name": "Ada Lovelace",
                "lang": "en", "inbox_project_id": "6X4Vv2hfXjqWf3wj", "karma": 5821.5,
                "karma_trend": "-0.13", "is_premium": true, "premium_status": "active",
                "mfa_enabled": true, "daily_goal": 5, "weekly_goal": 20,
                "completed_count": 1487, "completed_today": 3, "joined_at": "2019-03-04T10:00:00Z",
                "tz_info": {"timezone": "Europe/Berlin", "utc_offset": 7200}
            }),
            result_eq: ptrs([
                ("/email", json!("ada@example.com")),
                ("/full_name", json!("Ada Lovelace")),
                ("/karma", json!(5821.5)),
                ("/inbox_project_id", json!("6X4Vv2hfXjqWf3wj")),
            ]),
        },
        Case {
            tool: "TODOIST_LIST_PROJECTS",
            args: json!({}),
            method: "GET",
            url_contains: &[API, "/api/v1/projects"],
            body_eq: ptrs([]),
            response_status: 200,
            response: page(json!([
                {"id": PROJECT, "name": "Work", "color": "charcoal", "parent_id": null,
                 "order": 1, "is_favorite": true, "is_archived": false, "view_style": "list",
                 "description": "Client work", "url": format!("https://app.todoist.com/app/project/{PROJECT}")},
                {"id": "6X6Xx4hjZlsWg5yl", "name": "Groceries", "color": "lime_green",
                 "parent_id": null, "order": 2, "is_favorite": false, "is_archived": false,
                 "view_style": "board", "description": "", "url": "https://app.todoist.com/app/project/6X6Xx4hjZlsWg5yl"}
            ])),
            result_eq: ptrs([
                ("/count", json!(2)),
                ("/projects/0/id", json!(PROJECT)),
                ("/projects/0/name", json!("Work")),
                ("/projects/0/view_style", json!("list")),
            ]),
        },
        Case {
            tool: "TODOIST_GET_PROJECT",
            args: json!({"project_id": PROJECT}),
            method: "GET",
            url_contains: &[API, "/api/v1/projects/6X4Vv2hfXjqWf3wj"],
            body_eq: ptrs([]),
            response_status: 200,
            response: json!({
                "id": PROJECT, "name": "Work", "color": "charcoal", "parent_id": null,
                "order": 1, "is_favorite": true, "is_archived": false, "view_style": "list",
                "description": "Client work", "url": format!("https://app.todoist.com/app/project/{PROJECT}")
            }),
            result_eq: ptrs([
                ("/id", json!(PROJECT)),
                ("/name", json!("Work")),
                ("/is_favorite", json!(true)),
            ]),
        },
        Case {
            tool: "TODOIST_LIST_ARCHIVED_PROJECTS",
            args: json!({}),
            method: "GET",
            url_contains: &[API, "/api/v1/projects/archived"],
            body_eq: ptrs([]),
            response_status: 200,
            response: page(json!([
                {"id": "6X3Nn2kdZwrTc1zp", "name": "Old campaign", "is_archived": true}
            ])),
            result_eq: ptrs([
                ("/count", json!(1)),
                ("/projects/0/name", json!("Old campaign")),
            ]),
        },
        Case {
            tool: "TODOIST_LIST_SECTIONS",
            args: json!({"project_id": PROJECT}),
            method: "GET",
            url_contains: &[API, "/api/v1/sections", "project_id=6X4Vv2hfXjqWf3wj"],
            body_eq: ptrs([]),
            response_status: 200,
            response: page(json!([
                {"id": SECTION, "name": "Todo", "project_id": PROJECT, "order": 1,
                 "is_archived": false, "added_at": "2026-09-01T09:00:00.000Z",
                 "updated_at": "2026-09-01T09:00:00.000Z", "archived_at": null}
            ])),
            result_eq: ptrs([
                ("/count", json!(1)),
                ("/sections/0/id", json!(SECTION)),
                ("/sections/0/project_id", json!(PROJECT)),
            ]),
        },
        Case {
            tool: "TODOIST_GET_SECTION",
            args: json!({"section_id": SECTION}),
            method: "GET",
            url_contains: &[API, "/api/v1/sections/6X7rM8997g3RQmvh"],
            body_eq: ptrs([]),
            response_status: 200,
            response: json!({
                "id": SECTION, "name": "Todo", "project_id": PROJECT, "order": 1,
                "is_archived": false, "added_at": "2026-09-01T09:00:00.000Z",
                "updated_at": "2026-09-01T09:00:00.000Z", "archived_at": null
            }),
            result_eq: ptrs([("/id", json!(SECTION)), ("/name", json!("Todo"))]),
        },
        Case {
            tool: "TODOIST_LIST_ARCHIVED_SECTIONS",
            args: json!({"project_id": PROJECT}),
            method: "GET",
            url_contains: &[API, "/api/v1/sections/archived", "project_id=6X4Vv2hfXjqWf3wj"],
            body_eq: ptrs([]),
            response_status: 200,
            // Legacy envelope: the page comes under `sections`, not `results`.
            response: json!({"sections": [
                {"id": "6X2Mm1jcYvsQd0yo", "name": "Done 2025", "project_id": PROJECT,
                 "order": 4, "is_archived": true}
            ], "has_more": false, "next_cursor": null}),
            result_eq: ptrs([
                ("/count", json!(1)),
                ("/sections/0/name", json!("Done 2025")),
            ]),
        },
        Case {
            tool: "TODOIST_LIST_LABELS",
            args: json!({"cursor": "abcdef"}),
            method: "GET",
            url_contains: &[API, "/api/v1/labels", "cursor=abcdef"],
            body_eq: ptrs([]),
            response_status: 200,
            response: page(json!([
                {"id": LABEL, "name": "Errands", "color": "berry_red", "order": 1,
                 "is_favorite": true},
                {"id": "6X1Lm0hbZurPcxyn", "name": "Waiting", "color": "orange",
                 "order": 2, "is_favorite": false}
            ])),
            result_eq: ptrs([
                ("/count", json!(2)),
                ("/labels/0/name", json!("Errands")),
                ("/labels/1/is_favorite", json!(false)),
            ]),
        },
        Case {
            tool: "TODOIST_GET_LABEL",
            args: json!({"label_id": LABEL}),
            method: "GET",
            url_contains: &[API, "/api/v1/labels/6X5Ww3giYkrVg4xk"],
            body_eq: ptrs([]),
            response_status: 200,
            response: json!({"id": LABEL, "name": "Errands", "color": "berry_red",
                             "order": 1, "is_favorite": true}),
            result_eq: ptrs([
                ("/id", json!(LABEL)),
                ("/name", json!("Errands")),
                ("/color", json!("berry_red")),
            ]),
        },
        Case {
            tool: "TODOIST_LIST_SHARED_LABELS",
            args: json!({"omit_personal": true}),
            method: "GET",
            url_contains: &[API, "/api/v1/labels/shared", "omit_personal=true"],
            body_eq: ptrs([]),
            response_status: 200,
            response: page(json!(["team-inbox", "errands"])),
            result_eq: ptrs([
                ("/count", json!(2)),
                ("/shared_labels/0", json!("team-inbox")),
            ]),
        },
        Case {
            tool: "TODOIST_GET_TASK",
            args: json!({"task_id": TASK}),
            method: "GET",
            url_contains: &[API, "/api/v1/tasks/6X8rd0997g4SRwix"],
            body_eq: ptrs([]),
            response_status: 200,
            response: task(),
            result_eq: ptrs([
                ("/id", json!(TASK)),
                ("/content", json!("Buy milk")),
                ("/is_completed", json!(false)),
                ("/due/date", json!("2026-10-15")),
                ("/project_id", json!(PROJECT)),
            ]),
        },
        Case {
            tool: "TODOIST_FILTER_TASKS",
            args: json!({"query": "today | overdue", "lang": "en", "limit": 25}),
            method: "GET",
            url_contains: &[
                API,
                "/api/v1/tasks/filter",
                "query=today+%7C+overdue",
                "lang=en",
                "limit=25",
            ],
            body_eq: ptrs([]),
            response_status: 200,
            response: page(json!([task()])),
            result_eq: ptrs([
                ("/count", json!(1)),
                ("/tasks/0/id", json!(TASK)),
                ("/tasks/0/labels/0", json!("errands")),
            ]),
        },
        Case {
            tool: "TODOIST_LIST_COMPLETED_TASKS",
            args: json!({"since": "2026-06-01T00:00:00Z", "until": "2026-06-27T23:59:59Z",
                         "project_id": PROJECT, "limit": 50}),
            method: "GET",
            url_contains: &[
                API,
                "/api/v1/tasks/completed/by_completion_date",
                "since=2026-06-01T00%3A00%3A00Z",
                "until=2026-06-27T23%3A59%3A59Z",
                "project_id=6X4Vv2hfXjqWf3wj",
                "limit=50",
            ],
            body_eq: ptrs([]),
            response_status: 200,
            response: page(json!([{
                "id": TASK, "user_id": USER, "project_id": PROJECT, "section_id": SECTION,
                "parent_id": null, "added_by_uid": USER, "assigned_by_uid": null,
                "responsible_uid": USER, "completed_by_uid": USER, "labels": ["errands"],
                "checked": true, "is_deleted": false, "content": "Buy milk",
                "description": "Two litres of semi-skimmed", "priority": 3, "note_count": 0,
                "due": {"date": "2026-06-10", "string": "10 Jun", "is_recurring": false},
                "added_at": "2026-06-08T07:00:00.000Z",
                "completed_at": "2026-06-10T18:20:00.000Z",
                "updated_at": "2026-06-10T18:20:00.000Z"
            }])),
            result_eq: ptrs([
                ("/count", json!(1)),
                ("/tasks/0/completed_at", json!("2026-06-10T18:20:00.000Z")),
                ("/tasks/0/checked", json!(true)),
            ]),
        },
        Case {
            tool: "TODOIST_LIST_COMPLETED_TASKS_BY_DUE_DATE",
            args: json!({"since": "2026-06-01", "until": "2026-06-27", "filter_query": "#Work"}),
            method: "GET",
            url_contains: &[
                API,
                "/api/v1/tasks/completed/by_due_date",
                "since=2026-06-01",
                "until=2026-06-27",
                "filter_query=%23Work",
                // `fetchCompletedByDate` defaults the limit to 200.
                "limit=200",
            ],
            body_eq: ptrs([]),
            response_status: 200,
            response: page(json!([{
                "id": TASK, "project_id": PROJECT, "content": "Buy milk", "checked": true,
                "is_deleted": false, "completed_at": "2026-06-10T18:20:00.000Z"
            }])),
            result_eq: ptrs([
                ("/count", json!(1)),
                ("/tasks/0/id", json!(TASK)),
            ]),
        },
        Case {
            tool: "TODOIST_LIST_ACTIVITY_LOG",
            args: json!({"object_type": "item", "event_type": "completed", "limit": 10}),
            method: "GET",
            url_contains: &[
                API,
                "/api/v1/activities",
                "object_type=item",
                "event_type=completed",
                "limit=10",
            ],
            body_eq: ptrs([]),
            response_status: 200,
            response: page(json!([{
                "id": "2992679901", "event_type": "completed", "event_date": "2026-10-02T08:00:00Z",
                "object_type": "item", "object_id": TASK, "parent_project_id": PROJECT,
                "parent_item_id": null, "initiator_id": USER, "extra_data": null
            }])),
            result_eq: ptrs([
                ("/count", json!(1)),
                ("/events/0/event_type", json!("completed")),
                ("/events/0/object_id", json!(TASK)),
            ]),
        },
        Case {
            tool: "TODOIST_LIST_COMMENTS",
            args: json!({"task_id": TASK}),
            method: "GET",
            url_contains: &[API, "/api/v1/comments", "task_id=6X8rd0997g4SRwix"],
            body_eq: ptrs([]),
            response_status: 200,
            response: page(json!([comment()])),
            result_eq: ptrs([
                ("/count", json!(1)),
                ("/comments/0/id", json!(COMMENT)),
                ("/comments/0/content", json!("Oat milk is on sale")),
            ]),
        },
        Case {
            tool: "TODOIST_GET_COMMENT",
            args: json!({"comment_id": COMMENT}),
            method: "GET",
            url_contains: &[API, "/api/v1/comments/2992679862"],
            body_eq: ptrs([]),
            response_status: 200,
            response: comment(),
            result_eq: ptrs([
                ("/id", json!(COMMENT)),
                ("/task_id", json!(TASK)),
            ]),
        },
        Case {
            tool: "TODOIST_CREATE_TASK",
            args: json!({"content": " Buy milk ", "project_id": PROJECT, "section_id": SECTION,
                         "description": "Two litres", "labels": ["errands"], "priority": 3,
                         "due_date": "2026-10-15"}),
            method: "POST",
            url_contains: &[API, "/api/v1/tasks"],
            body_eq: ptrs([
                ("/content", json!("Buy milk")),
                ("/project_id", json!(PROJECT)),
                ("/section_id", json!(SECTION)),
                ("/description", json!("Two litres")),
                ("/labels/0", json!("errands")),
                ("/priority", json!(3)),
                ("/due_date", json!("2026-10-15")),
            ]),
            response_status: 200,
            response: task(),
            result_eq: ptrs([("/id", json!(TASK)), ("/content", json!("Buy milk"))]),
        },
        Case {
            tool: "TODOIST_UPDATE_TASK",
            args: json!({"task_id": TASK, "content": "Buy oat milk", "priority": 4,
                         "due_date": "tomorrow 5pm"}),
            method: "POST",
            url_contains: &[API, "/api/v1/tasks/6X8rd0997g4SRwix"],
            body_eq: ptrs([
                ("/content", json!("Buy oat milk")),
                ("/priority", json!(4)),
                // `dueDateParams` sends free text as `due_string`.
                ("/due_string", json!("tomorrow 5pm")),
            ]),
            response_status: 200,
            response: task(),
            result_eq: ptrs([("/id", json!(TASK))]),
        },
        Case {
            tool: "TODOIST_COMPLETE_TASK",
            args: json!({"task_id": TASK}),
            method: "POST",
            url_contains: &[API, "/api/v1/tasks/6X8rd0997g4SRwix/close"],
            body_eq: ptrs([]),
            response_status: 204,
            response: Value::Null,
            result_eq: ptrs([("/success", json!(true))]),
        },
        Case {
            tool: "TODOIST_REOPEN_TASK",
            args: json!({"task_id": TASK}),
            method: "POST",
            url_contains: &[API, "/api/v1/tasks/6X8rd0997g4SRwix/reopen"],
            body_eq: ptrs([]),
            response_status: 204,
            response: Value::Null,
            result_eq: ptrs([("/success", json!(true))]),
        },
        Case {
            tool: "TODOIST_MOVE_TASK",
            args: json!({"task_id": TASK, "section_id": SECTION}),
            method: "POST",
            url_contains: &[API, "/api/v1/tasks/6X8rd0997g4SRwix/move"],
            body_eq: ptrs([("/section_id", json!(SECTION))]),
            response_status: 200,
            response: task(),
            result_eq: ptrs([
                ("/id", json!(TASK)),
                ("/section_id", json!(SECTION)),
            ]),
        },
        Case {
            tool: "TODOIST_DELETE_TASK",
            args: json!({"task_id": TASK}),
            method: "DELETE",
            url_contains: &[API, "/api/v1/tasks/6X8rd0997g4SRwix"],
            body_eq: ptrs([]),
            response_status: 204,
            response: Value::Null,
            result_eq: ptrs([("/success", json!(true)), ("/deleted", json!(true))]),
        },
        Case {
            tool: "TODOIST_QUICK_ADD_TASK",
            args: json!({"text": "Submit report by Friday 5pm #Work p2", "auto_reminder": true}),
            method: "POST",
            url_contains: &[API, "/api/v1/tasks/quick"],
            body_eq: ptrs([
                ("/text", json!("Submit report by Friday 5pm #Work p2")),
                ("/auto_reminder", json!(true)),
            ]),
            response_status: 200,
            response: task(),
            result_eq: ptrs([("/id", json!(TASK))]),
        },
        Case {
            tool: "TODOIST_CREATE_PROJECT",
            args: json!({"name": "Home", "color": "charcoal", "view_style": "board",
                         "is_favorite": true}),
            method: "POST",
            url_contains: &[API, "/api/v1/projects"],
            body_eq: ptrs([
                ("/name", json!("Home")),
                ("/color", json!("charcoal")),
                ("/view_style", json!("board")),
                ("/is_favorite", json!(true)),
            ]),
            response_status: 200,
            response: json!({"id": "6X3Oo2leZxsRe1zq", "name": "Home", "color": "charcoal",
                             "view_style": "board", "is_favorite": true, "parent_id": null}),
            result_eq: ptrs([
                ("/id", json!("6X3Oo2leZxsRe1zq")),
                ("/name", json!("Home")),
            ]),
        },
        Case {
            tool: "TODOIST_UPDATE_PROJECT",
            args: json!({"project_id": PROJECT, "name": "Client work", "description": "Docs"}),
            method: "POST",
            url_contains: &[API, "/api/v1/projects/6X4Vv2hfXjqWf3wj"],
            body_eq: ptrs([
                ("/name", json!("Client work")),
                ("/description", json!("Docs")),
            ]),
            response_status: 200,
            response: json!({"id": PROJECT, "name": "Client work", "description": "Docs",
                             "is_favorite": true, "view_style": "list"}),
            result_eq: ptrs([
                ("/id", json!(PROJECT)),
                ("/name", json!("Client work")),
            ]),
        },
        Case {
            tool: "TODOIST_UNARCHIVE_PROJECT",
            args: json!({"project_id": "6X3Nn2kdZwrTc1zp"}),
            method: "POST",
            url_contains: &[API, "/api/v1/projects/6X3Nn2kdZwrTc1zp/unarchive"],
            body_eq: ptrs([]),
            response_status: 204,
            response: Value::Null,
            result_eq: ptrs([("/success", json!(true))]),
        },
        Case {
            tool: "TODOIST_CREATE_SECTION",
            args: json!({"name": "In review", "project_id": PROJECT, "order": 2}),
            method: "POST",
            url_contains: &[API, "/api/v1/sections"],
            body_eq: ptrs([
                ("/name", json!("In review")),
                ("/project_id", json!(PROJECT)),
                ("/order", json!(2)),
            ]),
            response_status: 200,
            response: json!({"id": "6X0Lh9gaZyrQbxvm", "name": "In review",
                             "project_id": PROJECT, "order": 2, "is_archived": false}),
            result_eq: ptrs([
                ("/id", json!("6X0Lh9gaZyrQbxvm")),
                ("/project_id", json!(PROJECT)),
            ]),
        },
        Case {
            tool: "TODOIST_UPDATE_SECTION",
            args: json!({"section_id": SECTION, "name": "To do"}),
            method: "POST",
            url_contains: &[API, "/api/v1/sections/6X7rM8997g3RQmvh"],
            body_eq: ptrs([("/name", json!("To do"))]),
            response_status: 200,
            response: json!({"id": SECTION, "name": "To do", "project_id": PROJECT, "order": 1}),
            result_eq: ptrs([("/id", json!(SECTION)), ("/name", json!("To do"))]),
        },
        Case {
            tool: "TODOIST_CREATE_TASK_COMMENT",
            args: json!({"task_id": TASK, "content": "Oat milk is on sale"}),
            method: "POST",
            url_contains: &[API, "/api/v1/comments"],
            body_eq: ptrs([
                ("/task_id", json!(TASK)),
                ("/content", json!("Oat milk is on sale")),
            ]),
            response_status: 200,
            response: comment(),
            result_eq: ptrs([
                ("/id", json!(COMMENT)),
                ("/task_id", json!(TASK)),
            ]),
        },
    ];

    support::assert_full_coverage(&a, &cases);
    for c in &cases {
        run(&a, c);
    }
}

#[test]
fn path_ids_are_validated_move_destinations_are_body_fields() {
    let a = adapter();
    // `task_id` is interpolated into the path, so it goes through `path_segment`.
    assert!(a
        .build_request("TODOIST_MOVE_TASK", &json!({"task_id": "../etc"}))
        .is_err());
    // `parent_id` is a JSON body field (POST /tasks/{id}/move), not a path segment.
    let req = a
        .build_request(
            "TODOIST_MOVE_TASK",
            &json!({"task_id": TASK, "parent_id": SUBTASK}),
        )
        .unwrap();
    let body: Value = serde_json::from_slice(req.body.as_deref().unwrap()).unwrap();
    assert_eq!(body["parent_id"], json!(SUBTASK));
}

/// Live read-only calls through the hub exec path. Token from
/// `CTOX_TOOLHUB_LIVE_TODOIST_TOKEN`; never printed.
///   CTOX_TOOLHUB_LIVE_TODOIST_TOKEN=… cargo test --test port_todoist -- --ignored
#[test]
#[ignore]
fn live_read_only_actions() {
    let a = adapter();
    for (tool, args) in [
        ("TODOIST_GET_CURRENT_USER", json!({})),
        ("TODOIST_LIST_PROJECTS", json!({})),
        ("TODOIST_LIST_LABELS", json!({})),
    ] {
        let Some(result) = live(&a, tool, args) else {
            return; // no token configured -> skip silently
        };
        let v = result.unwrap_or_else(|e| panic!("{tool} failed live: {e}"));
        assert!(v.is_object(), "{tool}: expected an object, got {v}");
    }
}