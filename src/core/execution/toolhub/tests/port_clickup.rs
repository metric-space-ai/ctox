//! ClickUp port: one recorded case per action, expectations derived from the
//! Activepieces `run()` code (all calls hit `https://api.clickup.com/api/v2/…`
//! with a bearer token injected by the hub).
mod support;

use ctox_toolhub::toolkits::clickup::ClickUp;
use ctox_toolhub::toolkits::ToolkitAdapter;
use serde_json::{json, Value};
use support::{live, run, Case};

const WS: &str = "9015113512";
const SPACE: &str = "9006001234";
const FOLDER: &str = "7ab90ecb61ba";
const LIST: &str = "90020065432";
const TASK: &str = "86b0c01d3";
const TASK2: &str = "86b0c9f2a";
const COMMENT: &str = "comment-41";
const FIELD: &str = "0a3c2c11-5d6f-4d9e-bd53-d1b4c6f4b96d";
const TIMER: &str = "timer-77";

fn adapter() -> ClickUp {
    ClickUp::with_api_base("https://api.clickup.test")
}

/// `&[(ptr, value)]` for a `Case` field: leaked so it lives as long as the case.
fn ptrs<const N: usize>(pairs: [(&'static str, Value); N]) -> &'static [(&'static str, Value)] {
    Box::leak(Box::new(pairs))
}

/// Leaked formatted URL fragment (`Case.url_contains` needs `&'static [&'static str]`).
fn url(fragment: String) -> &'static str {
    Box::leak(fragment.into_boxed_str())
}

/// Leaked fragment list, so `url_contains: urls(&[…])` keeps a `'static` slice.
fn urls(frags: &[&'static str]) -> &'static [&'static str] {
    Box::leak(frags.to_vec().into_boxed_slice())
}

/// Recorded task payload, shaped like a real `GET /api/v2/task/{id}` response.
fn task() -> Value {
    json!({
        "id": TASK,
        "custom_id": null,
        "name": "Port the ClickUp piece",
        "text_content": "Port the ClickUp piece",
        "description": "All 30 actions, endpoints from the TS source.",
        "status": {"status": "in progress", "type": "active", "orderindex": 2},
        "orderindex": "1.00000000000000000000000000000000",
        "date_created": "17672256000000",
        "date_updated": "1790812800000",
        "date_closed": null,
        "archived": false,
        "creator": {"id": 186, "username": "ada", "email": "ada@acme.com"},
        "assignees": [{"id": 186, "username": "ada"}],
        "watchers": [],
        "checklists": [],
        "tags": ["porting"],
        "parent": null,
        "priority": {"priority": "normal", "orderindex": "3"},
        "due_date": "1792519200000",
        "start_date": "1790812800000",
        "time_estimate": "3600000",
        "custom_fields": [{
            "id": FIELD, "name": "Effort", "type": "short_text",
            "value": "high", "required": false
        }],
        "list": {"id": LIST, "name": "Q4 Launch"},
        "project": {"id": FOLDER, "name": "Campaigns"},
        "space": {"id": SPACE},
        "url": format!("https://app.clickup.com/t/{TASK}"),
    })
}

/// Second recorded task in the same list (done, untagged).
fn task2() -> Value {
    json!({
        "id": TASK2,
        "name": "Wire up the hub registration",
        "status": {"status": "complete", "type": "closed", "orderindex": 24},
        "date_created": "1790812800000",
        "date_updated": "1790900000000",
        "date_closed": "1790900000000",
        "archived": false,
        "creator": {"id": 186, "username": "ada"},
        "assignees": [],
        "watchers": [],
        "checklists": [],
        "tags": [],
        "parent": null,
        "priority": null,
        "due_date": null,
        "start_date": null,
        "time_estimate": null,
        "custom_fields": [],
        "list": {"id": LIST, "name": "Q4 Launch"},
        "project": {"id": FOLDER, "name": "Campaigns"},
        "space": {"id": SPACE},
        "url": format!("https://app.clickup.com/t/{TASK2}"),
    })
}

fn recorded_list() -> Value {
    json!({
        "id": LIST,
        "name": "Q4 Launch",
        "orderindex": 0,
        "content": "Launch checklist for Q4.",
        "status": {"status": "active", "type": "open"},
        "priority": null,
        "assignee": null,
        "task_count": 12,
        "due_date": null,
        "start_date": null,
        "archived": false,
        "permission_level": "create",
        "space": {"id": SPACE, "name": "Marketing"},
        "folder": {"id": FOLDER, "name": "Campaigns", "hidden": false},
    })
}

fn recorded_comment() -> Value {
    json!({
        "id": COMMENT,
        "comment_text": "Ready for review.",
        "attached": [],
        "assignee": {"id": 186, "username": "ada", "email": "ada@acme.com"},
        "resolved": false,
        "user": {"id": 186, "username": "ada", "email": "ada@acme.com", "initials": "AL"},
        "date": "1790812800000",
        "histid": "5113493017032723456",
    })
}

#[test]
fn clickup_actions_are_ported() {
    let a = adapter();
    let cases: Vec<Case> = vec![
        // ------------------------------------------------------------- reads
        Case {
            tool: "CLICKUP_LIST_WORKSPACES",
            args: json!({}),
            method: "GET",
            url_contains: urls(&["https://api.clickup.test/api/v2/team"]),
            body_eq: ptrs([]),
            response_status: 200,
            response: json!({"teams": [{
                "id": WS, "name": "Acme", "color": "#7b68ee", "avatar": null,
                "members": [{"user": {"id": 186, "username": "ada", "email": "ada@acme.com"}}]
            }]}),
            result_eq: ptrs([
                ("/count", json!(1)),
                ("/items/0/id", json!(WS)),
                ("/items/0/members/0/user/username", json!("ada")),
            ]),
        },
        Case {
            tool: "CLICKUP_GET_SPACES",
            args: json!({"workspace_id": WS}),
            method: "GET",
            url_contains: urls(&[url(format!("api/v2/team/{WS}/space"))]),
            body_eq: ptrs([]),
            response_status: 200,
            response: json!({"spaces": [{
                "id": SPACE, "name": "Marketing", "private": false,
                "multiple_assignees": true, "archived": false,
                "statuses": [{"status": "to do", "type": "open"}, {"status": "complete", "type": "closed"}]
            }]}),
            result_eq: ptrs([
                ("/count", json!(1)),
                ("/items/0/name", json!("Marketing")),
                ("/items/0/multiple_assignees", json!(true)),
            ]),
        },
        Case {
            tool: "CLICKUP_GET_SPACE",
            args: json!({"space_id": SPACE}),
            method: "GET",
            url_contains: urls(&[url(format!("api/v2/space/{SPACE}"))]),
            body_eq: ptrs([]),
            response_status: 200,
            response: json!({
                "id": SPACE, "name": "Marketing", "private": false,
                "multiple_assignees": true, "archived": false
            }),
            result_eq: ptrs([("/id", json!(SPACE)), ("/name", json!("Marketing"))]),
        },
        Case {
            tool: "CLICKUP_GET_FOLDERS",
            args: json!({"space_id": SPACE}),
            method: "GET",
            url_contains: urls(&[url(format!("api/v2/space/{SPACE}/folder"))]),
            body_eq: ptrs([]),
            response_status: 200,
            response: json!({"folders": [{
                "id": FOLDER, "name": "Campaigns", "hidden": false, "archived": false,
                "space": {"id": SPACE}, "task_count": "12", "list_count": "2"
            }]}),
            result_eq: ptrs([
                ("/count", json!(1)),
                ("/items/0/name", json!("Campaigns")),
                ("/items/0/list_count", json!("2")),
            ]),
        },
        Case {
            tool: "CLICKUP_GET_FOLDER_LISTS",
            args: json!({"folder_id": FOLDER}),
            method: "GET",
            url_contains: urls(&[url(format!("api/v2/folder/{FOLDER}/list"))]),
            body_eq: ptrs([]),
            response_status: 200,
            response: json!({"lists": [recorded_list()]}),
            result_eq: ptrs([
                ("/count", json!(1)),
                ("/items/0/name", json!("Q4 Launch")),
                ("/items/0/folder/id", json!(FOLDER)),
            ]),
        },
        Case {
            tool: "CLICKUP_GET_FOLDERLESS_LISTS",
            args: json!({"space_id": SPACE}),
            method: "GET",
            url_contains: urls(&[url(format!("api/v2/space/{SPACE}/list"))]),
            body_eq: ptrs([]),
            response_status: 200,
            response: json!({"lists": [{
                "id": "90020099001", "name": "Backlog", "orderindex": 0,
                "content": "", "status": {"status": "active"}, "task_count": 4,
                "space": {"id": SPACE}, "folder": null
            }]}),
            result_eq: ptrs([
                ("/count", json!(1)),
                ("/items/0/name", json!("Backlog")),
                ("/items/0/space/id", json!(SPACE)),
            ]),
        },
        Case {
            tool: "CLICKUP_GET_LIST",
            args: json!({"list_id": LIST}),
            method: "GET",
            url_contains: urls(&[url(format!("api/v2/list/{LIST}"))]),
            body_eq: ptrs([]),
            response_status: 200,
            response: recorded_list(),
            result_eq: ptrs([
                ("/id", json!(LIST)),
                ("/name", json!("Q4 Launch")),
                ("/task_count", json!(12)),
                ("/folder/id", json!(FOLDER)),
            ]),
        },
        Case {
            tool: "CLICKUP_GET_LIST_CUSTOM_FIELDS",
            args: json!({"list_id": LIST}),
            method: "GET",
            url_contains: urls(&[url(format!("api/v2/list/{LIST}/field"))]),
            body_eq: ptrs([]),
            response_status: 200,
            response: json!({"fields": [
                {"id": FIELD, "name": "Effort", "type": "short_text", "date_created": "1767225600000",
                 "hide_from_guests": false, "type_config": {}, "required": false},
                {"id": "b0a1c2d3-4e5f-4a6b-8c7d-9e0f1a2b3c4d", "name": "Channel",
                 "type": "drop_down", "date_created": "1767225600000", "hide_from_guests": false,
                 "type_config": {"options": [{"id": "opt-1", "name": "Email"}]}, "required": false}
            ]}),
            result_eq: ptrs([
                ("/count", json!(2)),
                ("/items/0/name", json!("Effort")),
                ("/items/1/type", json!("drop_down")),
            ]),
        },
        Case {
            tool: "CLICKUP_GET_TASK",
            args: json!({"task_id": TASK, "include_subtasks": true}),
            method: "GET",
            url_contains: urls(&[url(format!("api/v2/task/{TASK}?include_subtasks=true"))]),
            body_eq: ptrs([]),
            response_status: 200,
            response: task(),
            result_eq: ptrs([
                ("/name", json!("Port the ClickUp piece")),
                ("/status/status", json!("in progress")),
                ("/list/id", json!(LIST)),
                ("/priority/priority", json!("normal")),
            ]),
        },
        Case {
            tool: "CLICKUP_LIST_LIST_TASKS",
            args: json!({"list_id": LIST, "order_by": "created", "include_closed": true}),
            method: "GET",
            url_contains: urls(&[url(format!("api/v2/list/{LIST}/task?")), "page=0", "reverse=false",
                            "include_closed=true", "subtasks=false", "order_by=created"]),
            body_eq: ptrs([]),
            response_status: 200,
            response: json!({"tasks": [task(), task2()], "last_page": true}),
            result_eq: ptrs([
                ("/count", json!(2)),
                ("/items/0/id", json!(TASK)),
                ("/items/1/id", json!(TASK2)),
                ("/last_page", json!(true)),
            ]),
        },
        Case {
            tool: "CLICKUP_LIST_WORKSPACE_TASKS",
            args: json!({"workspace_id": WS, "space_ids": [SPACE], "folder_ids": [FOLDER],
                         "list_ids": [LIST], "assignees": [186], "tags": ["porting"],
                         "order_by": "updated", "include_closed": true}),
            method: "GET",
            url_contains: urls(&[url(format!("api/v2/team/{WS}/task?")), "page=0", "reverse=false",
                            "include_closed=true", "order_by=updated",
                            url(format!("space_ids={SPACE}")), url(format!("project_ids={FOLDER}")),
                            url(format!("list_ids={LIST}")), "assignees=186", "tags=porting"]),
            body_eq: ptrs([]),
            response_status: 200,
            response: json!({"tasks": [task()], "last_page": false}),
            result_eq: ptrs([
                ("/count", json!(1)),
                ("/items/0/tags", json!(["porting"])),
            ]),
        },
        Case {
            tool: "CLICKUP_GET_TASK_COMMENTS",
            args: json!({"task_id": TASK}),
            method: "GET",
            url_contains: urls(&[url(format!("api/v2/task/{TASK}/comment"))]),
            body_eq: ptrs([]),
            response_status: 200,
            response: json!({"comments": [recorded_comment()]}),
            result_eq: ptrs([
                ("/count", json!(1)),
                ("/items/0/comment_text", json!("Ready for review.")),
                ("/items/0/user/username", json!("ada")),
            ]),
        },
        Case {
            tool: "CLICKUP_GET_TASK_MEMBERS",
            args: json!({"task_id": TASK}),
            method: "GET",
            url_contains: urls(&[url(format!("api/v2/task/{TASK}/member"))]),
            body_eq: ptrs([]),
            response_status: 200,
            response: json!([
                {"user": {"id": 186, "username": "ada", "email": "ada@acme.com", "color": "#7b68ee"}},
                {"user": {"id": 245, "username": "grace", "email": "grace@acme.com", "color": "#00ff00"}}
            ]),
            result_eq: ptrs([
                ("/count", json!(2)),
                ("/members/1/user/username", json!("grace")),
            ]),
        },
        Case {
            tool: "CLICKUP_GET_SPACE_TAGS",
            args: json!({"space_id": SPACE}),
            method: "GET",
            url_contains: urls(&[url(format!("api/v2/space/{SPACE}/tag"))]),
            body_eq: ptrs([]),
            response_status: 200,
            response: json!({"tags": [
                {"id": "tag-1", "name": "porting", "tag_fg": "#ffffff", "tag_bg": "#7b68ee",
                 "creator": 186},
                {"id": "tag-2", "name": "blocked", "tag_fg": "#000000", "tag_bg": "#ff0000",
                 "creator": 186}
            ]}),
            result_eq: ptrs([
                ("/count", json!(2)),
                ("/items/0/name", json!("porting")),
                ("/items/1/tag_bg", json!("#ff0000")),
            ]),
        },
        Case {
            tool: "CLICKUP_LIST_TIME_ENTRIES",
            args: json!({"workspace_id": WS, "start_date": "2026-10-01T00:00:00Z",
                         "end_date": "2026-10-08T00:00:00Z", "task_id": TASK,
                         "assignee": [186], "include_task_tags": true}),
            method: "GET",
            url_contains: urls(&[url(format!("api/v2/team/{WS}/time_entries?")),
                            "assignee=186", "include_task_tags=true", "include_location_names=false",
                            "start_date=1790812800000", "end_date=1791417600000",
                            url(format!("task_id={TASK}"))]),
            body_eq: ptrs([]),
            response_status: 200,
            response: json!({"data": [{
                "id": TIMER, "task_id": TASK, "wid": WS, "start": "1790830000000",
                "end": "1790833600000", "duration": "3600000", "billable": true,
                "description": "porting the chat actions", "tags": [], "at": "1790833600123-0500",
                "task_name": "Port the ClickUp piece", "user": {"id": 186, "username": "ada"}
            }], "last_page": true, "has_more": false}),
            result_eq: ptrs([
                ("/count", json!(1)),
                ("/items/0/duration", json!("3600000")),
                ("/items/0/billable", json!(true)),
            ]),
        },
        // ---------------------------------------------------------- mutations
        Case {
            tool: "CLICKUP_CREATE_TASK",
            args: json!({
                "list_id": LIST, "name": "Review the ClickUp port",
                "description": "# Checklist\n- endpoints", "use_markdown": true,
                "status_id": "to do", "priority_id": 2, "assignee_id": [186],
                "due_date": "2026-10-20T18:00:00Z", "due_date_time": true,
                "time_estimate": 3600000,
                "custom_fields": {FIELD: "high"},
            }),
            method: "POST",
            url_contains: urls(&[url(format!("api/v2/list/{LIST}/task"))]),
            body_eq: ptrs([
                ("/name", json!("Review the ClickUp port")),
                ("/markdown_content", json!("# Checklist\n- endpoints")),
                ("/status", json!("to do")),
                ("/priority", json!(2)),
                ("/assignees", json!([186])),
                ("/due_date", json!(1792519200000_i64)),
                ("/due_date_time", json!(true)),
                ("/time_estimate", json!(3600000)),
                ("/custom_fields/0/id", json!(FIELD)),
                ("/custom_fields/0/value", json!("high")),
            ]),
            response_status: 200,
            response: task(),
            result_eq: ptrs([
                ("/id", json!(TASK)),
                ("/name", json!("Port the ClickUp piece")),
                ("/status/status", json!("in progress")),
            ]),
        },
        Case {
            tool: "CLICKUP_UPDATE_TASK",
            args: json!({"task_id": TASK, "name": "Review the ClickUp port (v2)",
                         "status_id": "in review", "priority_id": 1,
                         "add_assignee": [245], "rem_assignee": [186]}),
            method: "PUT",
            url_contains: urls(&[url(format!("api/v2/task/{TASK}"))]),
            body_eq: ptrs([
                ("/name", json!("Review the ClickUp port (v2)")),
                ("/status", json!("in review")),
                ("/priority", json!(1)),
                ("/assignees/add", json!([245])),
                ("/assignees/rem", json!([186])),
            ]),
            response_status: 200,
            response: task(),
            result_eq: ptrs([("/id", json!(TASK)), ("/status/type", json!("active"))]),
        },
        Case {
            tool: "CLICKUP_SET_TASK_STATUS",
            args: json!({"task_id": TASK, "status_id": "complete"}),
            method: "PUT",
            url_contains: urls(&[url(format!("api/v2/task/{TASK}"))]),
            body_eq: ptrs([("/status", json!("complete"))]),
            response_status: 200,
            response: json!({
                "id": TASK, "name": "Port the ClickUp piece",
                "status": {"status": "complete", "type": "closed", "orderindex": 4},
                "date_closed": "1790900000000"
            }),
            result_eq: ptrs([
                ("/status/status", json!("complete")),
                ("/date_closed", json!("1790900000000")),
            ]),
        },
        Case {
            tool: "CLICKUP_DELETE_TASK",
            args: json!({"task_id": TASK}),
            method: "DELETE",
            url_contains: urls(&[url(format!("api/v2/task/{TASK}"))]),
            body_eq: ptrs([]),
            response_status: 200,
            response: json!({}),
            result_eq: ptrs([]),
        },
        Case {
            tool: "CLICKUP_CREATE_TASK_COMMENT",
            args: json!({"task_id": TASK, "comment": "Ready for review.",
                         "assignee": 245}),
            method: "POST",
            url_contains: urls(&[url(format!("api/v2/task/{TASK}/comment"))]),
            body_eq: ptrs([
                ("/comment_text", json!("Ready for review.")),
                ("/assignee", json!(245)),
                ("/notify_all", json!(true)),
            ]),
            response_status: 200,
            response: json!({
                "id": COMMENT,
                "comment": [{"text": "Ready for review.", "assignee": {"id": 245}}],
                "histid": "5113493017032723999"
            }),
            result_eq: ptrs([("/id", json!(COMMENT))]),
        },
        Case {
            tool: "CLICKUP_UPDATE_COMMENT",
            args: json!({"comment_id": COMMENT, "comment_text": "Ready for review (edited).",
                         "resolved": true}),
            method: "PUT",
            url_contains: urls(&[url(format!("api/v2/comment/{COMMENT}"))]),
            body_eq: ptrs([
                ("/comment_text", json!("Ready for review (edited).")),
                ("/resolved", json!(true)),
            ]),
            response_status: 200,
            response: json!({}),
            result_eq: ptrs([]),
        },
        Case {
            tool: "CLICKUP_DELETE_COMMENT",
            args: json!({"comment_id": COMMENT}),
            method: "DELETE",
            url_contains: urls(&[url(format!("api/v2/comment/{COMMENT}"))]),
            body_eq: ptrs([]),
            response_status: 200,
            response: json!({}),
            result_eq: ptrs([]),
        },
        Case {
            tool: "CLICKUP_CREATE_FOLDER",
            args: json!({"space_id": SPACE, "name": "Campaigns 2027"}),
            method: "POST",
            url_contains: urls(&[url(format!("api/v2/space/{SPACE}/folder"))]),
            body_eq: ptrs([("/name", json!("Campaigns 2027"))]),
            response_status: 200,
            response: json!({
                "id": "7ab90ecbff12", "name": "Campaigns 2027", "hidden": false,
                "space": {"id": SPACE}, "task_count": "0", "list_count": "0"
            }),
            result_eq: ptrs([
                ("/name", json!("Campaigns 2027")),
                ("/space/id", json!(SPACE)),
            ]),
        },
        Case {
            tool: "CLICKUP_CREATE_FOLDERLESS_LIST",
            args: json!({"space_id": SPACE, "name": "Backlog 2027"}),
            method: "POST",
            url_contains: urls(&[url(format!("api/v2/space/{SPACE}/list"))]),
            body_eq: ptrs([("/name", json!("Backlog 2027"))]),
            response_status: 200,
            response: json!({
                "id": "90020099002", "name": "Backlog 2027", "orderindex": 0,
                "content": "", "status": {"status": "active"}, "space": {"id": SPACE}
            }),
            result_eq: ptrs([
                ("/id", json!("90020099002")),
                ("/space/id", json!(SPACE)),
            ]),
        },
        Case {
            tool: "CLICKUP_UPDATE_LIST",
            args: json!({"list_id": LIST, "name": "Q4 Launch (renamed)", "content": "Updated brief."}),
            method: "PUT",
            url_contains: urls(&[url(format!("api/v2/list/{LIST}"))]),
            body_eq: ptrs([
                ("/name", json!("Q4 Launch (renamed)")),
                ("/content", json!("Updated brief.")),
            ]),
            response_status: 200,
            response: {
                let mut v = recorded_list();
                v["name"] = json!("Q4 Launch (renamed)");
                v
            },
            result_eq: ptrs([("/name", json!("Q4 Launch (renamed)")), ("/task_count", json!(12))]),
        },
        Case {
            tool: "CLICKUP_DELETE_LIST",
            args: json!({"list_id": LIST}),
            method: "DELETE",
            url_contains: urls(&[url(format!("api/v2/list/{LIST}"))]),
            body_eq: ptrs([]),
            response_status: 200,
            response: json!({}),
            result_eq: ptrs([]),
        },
        Case {
            tool: "CLICKUP_CREATE_SPACE",
            args: json!({"workspace_id": WS, "name": "Research", "multiple_assignees": false}),
            method: "POST",
            url_contains: urls(&[url(format!("api/v2/team/{WS}/space"))]),
            body_eq: ptrs([
                ("/name", json!("Research")),
                ("/multiple_assignees", json!(false)),
            ]),
            response_status: 200,
            response: json!({
                "id": "9006008877", "name": "Research", "private": false,
                "multiple_assignees": false, "archived": false
            }),
            result_eq: ptrs([
                ("/id", json!("9006008877")),
                ("/multiple_assignees", json!(false)),
            ]),
        },
        Case {
            tool: "CLICKUP_ADD_TAG_TO_TASK",
            args: json!({"task_id": TASK, "tag_name": "in review"}),
            method: "POST",
            url_contains: urls(&[url(format!("api/v2/task/{TASK}/tag/in%20review"))]),
            body_eq: ptrs([]),
            response_status: 200,
            response: json!({}),
            result_eq: ptrs([]),
        },
        Case {
            tool: "CLICKUP_REMOVE_TAG_FROM_TASK",
            args: json!({"task_id": TASK, "tag_name": "porting"}),
            method: "DELETE",
            url_contains: urls(&[url(format!("api/v2/task/{TASK}/tag/porting"))]),
            body_eq: ptrs([]),
            response_status: 200,
            response: json!({}),
            result_eq: ptrs([]),
        },
        Case {
            tool: "CLICKUP_CREATE_TIME_ENTRY",
            args: json!({"workspace_id": WS, "start": "2026-10-01T09:30:00Z",
                         "duration": 1800000, "description": "reviewing the port",
                         "task_id": TASK, "billable": true}),
            method: "POST",
            url_contains: urls(&[url(format!("api/v2/team/{WS}/time_entries"))]),
            body_eq: ptrs([
                ("/start", json!(1790847000000_i64)),
                ("/duration", json!(1800000)),
                ("/billable", json!(true)),
                ("/description", json!("reviewing the port")),
                ("/tid", json!(TASK)),
            ]),
            response_status: 200,
            response: json!({"data": [{
                "id": "timer-99", "task_id": TASK, "wid": WS,
                "start": "1790847000000", "duration": "1800000", "billable": true
            }]}),
            result_eq: ptrs([("/data/0/id", json!("timer-99"))]),
        },
    ];
    support::assert_full_coverage(&a, &cases);
    for c in &cases {
        run(&a, c);
    }
}

#[test]
fn invalid_arguments_are_rejected() {
    let a = adapter();
    // Path traversal and separators in path params are rejected by the guard.
    assert!(a.build_request("CLICKUP_GET_TASK", &json!({"task_id": "../x"})).is_err());
    assert!(a.build_request("CLICKUP_GET_LIST", &json!({"list_id": "9002/x"})).is_err());
    // Tags are free text but must not escape the path segment.
    assert!(a
        .build_request("CLICKUP_ADD_TAG_TO_TASK", &json!({"task_id": TASK, "tag_name": "a?b"}))
        .is_err());
    // Enum + required-value checks.
    assert!(a
        .build_request("CLICKUP_LIST_LIST_TASKS", &json!({"list_id": LIST, "order_by": "bogus"}))
        .is_err());
    assert!(a.build_request("CLICKUP_SET_TASK_STATUS", &json!({"task_id": TASK})).is_err());
    assert!(a.build_request("CLICKUP_UPDATE_TASK", &json!({"task_id": TASK})).is_err());
    assert!(a.build_request("CLICKUP_UPDATE_LIST", &json!({"list_id": LIST})).is_err());
    assert_eq!(
        a.build_request("CLICKUP_LIST_WORKSPACES", &json!({})).unwrap().url,
        "https://api.clickup.test/api/v2/team"
    );
}

/// Live read-only calls through the hub exec path (OAuth2 bearer token).
/// Token from `CTOX_TOOLHUB_LIVE_CLICKUP_TOKEN`, never printed.
#[test]
#[ignore]
fn live_read_only_actions() {
    let a = adapter();
    let Some(result) = live(&a, "CLICKUP_LIST_WORKSPACES", json!({})) else {
        return;
    };
    let workspaces = result.unwrap_or_else(|e| panic!("CLICKUP_LIST_WORKSPACES: {e}"));
    let Some(workspace_id) = support::find_str(&workspaces, "id") else {
        panic!("CLICKUP_LIST_WORKSPACES: no workspace id in {workspaces}");
    };
    let spaces = live(&a, "CLICKUP_GET_SPACES", json!({"workspace_id": workspace_id}))
        .expect("token present")
        .unwrap_or_else(|e| panic!("CLICKUP_GET_SPACES: {e}"));
    let Some(space_id) = support::find_str(&spaces, "id") else {
        panic!("CLICKUP_GET_SPACES: no space id in {spaces}");
    };
    let folders = live(&a, "CLICKUP_GET_FOLDERS", json!({"space_id": space_id}))
        .expect("token present")
        .unwrap_or_else(|e| panic!("CLICKUP_GET_FOLDERS: {e}"));
    assert!(folders.is_object(), "CLICKUP_GET_FOLDERS: expected an object, got {folders}");
}
