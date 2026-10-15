//! Asana port: one recorded case per action. Expectations are derived from the Activepieces
//! `run()` code — every action is a single `httpClient.sendRequest` against
//! `https://app.asana.com/api/1.0<path>` with `{data: …}` bodies and `opt_fields`/`limit`/
//! `offset` query parameters.
mod support;

use ctox_toolhub::toolkits::asana::Asana;
use serde_json::{json, Value};
use support::{live, run, Case};

const BASE: &str = "https://app.asana.com/api/1.0";
const ENDPOINT: &[&str] = &[BASE];

const WORKSPACE: &str = "1201234567890123";
const TEAM: &str = "1209876543210987";
const USER: &str = "1205555555555555";
const PROJECT: &str = "1207777777777777";
const SECTION: &str = "1208888888888888";
const TAG: &str = "1209999999999999";
const TASK: &str = "1204444444444444";
const TASK2: &str = "1203333333333333";
const STORY: &str = "1202222222222222";

fn adapter() -> Asana {
    Asana::with_api_base(BASE)
}

/// `&[(ptr, value)]` for a `Case` field: leaked so it lives as long as the case.
fn ptrs<const N: usize>(pairs: [(&'static str, Value); N]) -> &'static [(&'static str, Value)] {
    Box::leak(Box::new(pairs))
}

/// Asana list envelope `{data: [...], next_page: {offset, path, uri}}`.
fn list(items: Value) -> Value {
    json!({"data": items, "next_page": {"offset": "MjAyNi0xMC0wMVQwMDowMDowMC4wMDBa",
        "path": "/tasks?offset=2026-10-01T00:00:00.000Z", "uri": "https://app.asana.com/api/1.0/tasks?offset=2026-10-01T00:00:00.000Z"}})
}

fn task() -> Value {
    json!({
        "gid": TASK, "resource_type": "task", "resource_subtype": "default_task",
        "name": "Login page times out on Safari", "notes": "The spinner never stops.",
        "completed": false, "completed_at": null,
        "created_at": "2026-09-01T10:00:00.000Z", "modified_at": "2026-09-20T08:30:00.000Z",
        "due_on": "2026-10-15", "due_at": "2026-10-15T17:00:00.000Z",
        "start_on": null, "start_at": null,
        "assignee": {"gid": USER, "name": "Ada Lovelace", "email": "ada@acme.com"},
        "parent": null,
        "projects": [{"gid": PROJECT, "name": "Platform hardening"}],
        "memberships": [{"project": {"gid": PROJECT, "name": "Platform hardening"},
                         "section": {"gid": SECTION, "name": "In review"}}],
        "tags": [{"gid": TAG, "name": "urgent"}],
        "followers": [{"gid": USER, "name": "Ada Lovelace"}],
        "workspace": {"gid": WORKSPACE, "name": "Acme"},
        "num_subtasks": 2,
        "permalink_url": "https://app.asana.com/0/1201234567890123/1204444444444444/f"
    })
}

fn story() -> Value {
    json!({
        "gid": STORY, "resource_type": "story", "resource_subtype": "comment_added",
        "type": "comment", "text": "Reproduced on Safari 17.", "is_pinned": false,
        "html_text": {"body": "<body>Reproduced on Safari 17.</body>", "status": "OK"},
        "is_edited": false, "is_editable": true, "sticker_name": null,
        "created_at": "2026-09-21T09:00:00.000Z",
        "created_by": {"gid": USER, "name": "Ada Lovelace"},
        "target": {"gid": TASK, "name": "Login page times out on Safari"}
    })
}

#[test]
fn asana_actions_are_ported() {
    let a = adapter();
    let cases: Vec<Case> = vec![
        Case {
            tool: "ASANA_GET_CURRENT_USER",
            args: json!({}),
            method: "GET",
            url_contains: &[
                "https://app.asana.com/api/1.0/users/me?",
                "opt_fields=name%2Cemail%2Cworkspaces.name",
            ],
            body_eq: ptrs([]),
            response_status: 200,
            response: json!({"data": {
                "gid": USER, "resource_type": "user", "name": "Ada Lovelace",
                "email": "ada@acme.com",
                "workspaces": [{"gid": WORKSPACE, "resource_type": "workspace", "name": "Acme"}]}}),
            result_eq: ptrs([
                ("/gid", json!(USER)),
                ("/email", json!("ada@acme.com")),
                ("/workspaces/0/name", json!("Acme")),
            ]),
        },
        Case {
            tool: "ASANA_GET_USER",
            args: json!({"user": "me"}),
            method: "GET",
            url_contains: &[
                "https://app.asana.com/api/1.0/users/me?",
                "opt_fields=name%2Cemail%2Cworkspaces.name",
            ],
            body_eq: ptrs([]),
            response_status: 200,
            response: json!({"data": {
                "gid": USER, "name": "Ada Lovelace", "email": "ada@acme.com",
                "workspaces": [{"gid": WORKSPACE, "name": "Acme"}]}}),
            result_eq: ptrs([("/name", json!("Ada Lovelace"))]),
        },
        Case {
            tool: "ASANA_LIST_USERS",
            args: json!({"workspace": WORKSPACE, "limit": 25}),
            method: "GET",
            url_contains: &[
                "https://app.asana.com/api/1.0/users?",
                "workspace=1201234567890123",
                "opt_fields=name%2Cemail",
                "limit=25",
            ],
            body_eq: ptrs([]),
            response_status: 200,
            response: list(json!([
                {"gid": USER, "name": "Ada Lovelace", "email": "ada@acme.com"},
                {"gid": "1206666666666666", "name": "Grace Hopper", "email": "grace@acme.com"}])),
            result_eq: ptrs([
                ("/data/0/email", json!("ada@acme.com")),
                ("/data/1/name", json!("Grace Hopper")),
                ("/next_offset", json!("MjAyNi0xMC0wMVQwMDowMDowMC4wMDBa")),
            ]),
        },
        Case {
            tool: "ASANA_GET_WORKSPACE",
            args: json!({"workspace": WORKSPACE}),
            method: "GET",
            url_contains: &[
                "https://app.asana.com/api/1.0/workspaces/1201234567890123?",
                "opt_fields=name%2Cis_organization%2Cemail_domains",
            ],
            body_eq: ptrs([]),
            response_status: 200,
            response: json!({"data": {
                "gid": WORKSPACE, "name": "Acme", "is_organization": true,
                "email_domains": ["acme.com"]}}),
            result_eq: ptrs([
                ("/name", json!("Acme")),
                ("/is_organization", json!(true)),
                ("/email_domains/0", json!("acme.com")),
            ]),
        },
        Case {
            tool: "ASANA_LIST_WORKSPACES",
            args: json!({"limit": 50}),
            method: "GET",
            url_contains: &[
                "https://app.asana.com/api/1.0/workspaces?",
                "opt_fields=name%2Cis_organization%2Cemail_domains",
                "limit=50",
            ],
            body_eq: ptrs([]),
            response_status: 200,
            response: list(json!([
                {"gid": WORKSPACE, "name": "Acme", "is_organization": true, "email_domains": []}])),
            result_eq: ptrs([("/data/0/gid", json!(WORKSPACE))]),
        },
        Case {
            tool: "ASANA_GET_TEAM",
            args: json!({"team": TEAM}),
            method: "GET",
            url_contains: &[
                "https://app.asana.com/api/1.0/teams/1209876543210987?",
                "opt_fields=name%2Cdescription%2Chtml_description%2Cvisibility%2Cendorsed%2Corganization.name%2Cpermalink_url",
            ],
            body_eq: ptrs([]),
            response_status: 200,
            response: json!({"data": {
                "gid": TEAM, "name": "Platform", "description": "Platform engineering",
                "html_description": "<body>Platform engineering</body>", "visibility": "private",
                "endorsed": false, "organization": {"gid": WORKSPACE, "name": "Acme"},
                "permalink_url": "https://app.asana.com/0/1201234567890123/list"}}),
            result_eq: ptrs([
                ("/name", json!("Platform")),
                ("/visibility", json!("private")),
                ("/organization/name", json!("Acme")),
            ]),
        },
        Case {
            tool: "ASANA_LIST_TEAMS",
            args: json!({"workspace": WORKSPACE, "offset": "MjAyNi0xMC0wMQ"}),
            method: "GET",
            url_contains: &[
                "https://app.asana.com/api/1.0/workspaces/1201234567890123/teams?",
                "opt_fields=name%2Cdescription%2Cvisibility%2Corganization.name%2Cpermalink_url",
                "limit=50",
                "offset=MjAyNi0xMC0wMQ",
            ],
            body_eq: ptrs([]),
            response_status: 200,
            response: json!({"data": [{"gid": TEAM, "name": "Platform"}], "next_page": null}),
            result_eq: ptrs([("/data/0/name", json!("Platform")), ("/next_offset", Value::Null)]),
        },
        Case {
            tool: "ASANA_GET_PROJECT",
            args: json!({"project": PROJECT}),
            method: "GET",
            url_contains: &[
                "https://app.asana.com/api/1.0/projects/1207777777777777?",
                "opt_fields=name%2Carchived%2Ccolor%2Ccompleted%2Ccompleted_at%2Ccreated_at%2Cmodified_at%2Cdefault_view%2Cdue_on%2Cstart_on%2Cnotes%2Cowner.name%2Cprivacy_setting%2Cteam.name%2Cworkspace.name%2Ccurrent_status_update.title%2Ccurrent_status_update.status_type%2Cproject_brief%2Cmembers.name%2Cfollowers.name%2Cpermalink_url",
            ],
            body_eq: ptrs([]),
            response_status: 200,
            response: json!({"data": {
                "gid": PROJECT, "name": "Platform hardening", "archived": false,
                "color": "dark-blue", "completed": false, "completed_at": null,
                "created_at": "2026-08-01T09:00:00.000Z", "modified_at": "2026-09-20T09:00:00.000Z",
                "default_view": "list", "due_on": "2026-11-30", "start_on": "2026-09-01",
                "notes": "Make the platform boring.",
                "owner": {"gid": USER, "name": "Ada Lovelace"},
                "privacy_setting": "private_to_team",
                "team": {"gid": TEAM, "name": "Platform"},
                "workspace": {"gid": WORKSPACE, "name": "Acme"},
                "current_status_update": {"gid": "1200000000000000", "title": "On track",
                                          "status_type": "on_track"},
                "project_brief": null,
                "members": [{"gid": USER, "name": "Ada Lovelace"}],
                "followers": [{"gid": USER, "name": "Ada Lovelace"}],
                "permalink_url": "https://app.asana.com/0/1201234567890123/1207777777777777/f"}}),
            result_eq: ptrs([
                ("/name", json!("Platform hardening")),
                ("/archived", json!(false)),
                ("/current_status_update/status_type", json!("on_track")),
                ("/members/0/name", json!("Ada Lovelace")),
            ]),
        },
        Case {
            tool: "ASANA_LIST_PROJECTS",
            args: json!({"workspace": WORKSPACE, "archived": false}),
            method: "GET",
            url_contains: &[
                "https://app.asana.com/api/1.0/projects?",
                "workspace=1201234567890123",
                "archived=false",
                "opt_fields=name%2Carchived%2Ccolor%2Ccreated_at%2Cmodified_at%2Cdue_on%2Cowner.name%2Cprivacy_setting%2Cteam.name%2Cpermalink_url",
                "limit=50",
            ],
            body_eq: ptrs([]),
            response_status: 200,
            response: list(json!([{"gid": PROJECT, "name": "Platform hardening", "archived": false}])),
            result_eq: ptrs([("/data/0/name", json!("Platform hardening"))]),
        },
        Case {
            tool: "ASANA_GET_SECTION",
            args: json!({"section": SECTION}),
            method: "GET",
            url_contains: &[
                "https://app.asana.com/api/1.0/sections/1208888888888888?",
                "opt_fields=name%2Ccreated_at%2Cproject.name",
            ],
            body_eq: ptrs([]),
            response_status: 200,
            response: json!({"data": {"gid": SECTION, "name": "In review",
                "created_at": "2026-08-01T09:00:00.000Z",
                "project": {"gid": PROJECT, "name": "Platform hardening"}}}),
            result_eq: ptrs([("/name", json!("In review")), ("/project/name", json!("Platform hardening"))]),
        },
        Case {
            tool: "ASANA_LIST_SECTIONS",
            args: json!({"project": PROJECT}),
            method: "GET",
            url_contains: &[
                "https://app.asana.com/api/1.0/projects/1207777777777777/sections?",
                "opt_fields=name%2Ccreated_at%2Cproject.name",
                "limit=50",
            ],
            body_eq: ptrs([]),
            response_status: 200,
            response: list(json!([
                {"gid": SECTION, "name": "Todo"},
                {"gid": "1206666666666666", "name": "In review"}])),
            result_eq: ptrs([("/data/0/name", json!("Todo")), ("/data/1/name", json!("In review"))]),
        },
        Case {
            tool: "ASANA_GET_TAG",
            args: json!({"tag": TAG}),
            method: "GET",
            url_contains: &[
                "https://app.asana.com/api/1.0/tags/1209999999999999?",
                "opt_fields=name%2Ccolor%2Cnotes%2Ccreated_at%2Cfollowers.name%2Cworkspace.name%2Cpermalink_url",
            ],
            body_eq: ptrs([]),
            response_status: 200,
            response: json!({"data": {
                "gid": TAG, "name": "urgent", "color": "red", "notes": "Needs a second pair of eyes",
                "created_at": "2026-08-01T09:00:00.000Z",
                "followers": [{"gid": USER, "name": "Ada Lovelace"}],
                "workspace": {"gid": WORKSPACE, "name": "Acme"},
                "permalink_url": "https://app.asana.com/0/1201234567890123/1209999999999999/f"}}),
            result_eq: ptrs([("/name", json!("urgent")), ("/color", json!("red"))]),
        },
        Case {
            tool: "ASANA_LIST_TAGS",
            args: json!({"workspace": WORKSPACE}),
            method: "GET",
            url_contains: &[
                "https://app.asana.com/api/1.0/tags?",
                "workspace=1201234567890123",
                "opt_fields=name%2Ccolor%2Ccreated_at%2Cpermalink_url",
                "limit=50",
            ],
            body_eq: ptrs([]),
            response_status: 200,
            response: list(json!([{"gid": TAG, "name": "urgent", "color": "red"}])),
            result_eq: ptrs([("/data/0/gid", json!(TAG))]),
        },
        Case {
            tool: "ASANA_GET_TASK",
            args: json!({"task": TASK}),
            method: "GET",
            url_contains: &[
                "https://app.asana.com/api/1.0/tasks/1204444444444444?",
                "opt_fields=name%2Cresource_subtype%2Cnotes%2Ccompleted%2Ccompleted_at%2Ccreated_at%2Cmodified_at%2Cdue_on%2Cdue_at%2Cstart_on%2Cstart_at%2Cassignee.name%2Cassignee.email%2Cparent.name%2Cprojects.name%2Cmemberships.project.name%2Cmemberships.section.name%2Ctags.name%2Cfollowers.name%2Cworkspace.name%2Cnum_subtasks%2Cpermalink_url",
            ],
            body_eq: ptrs([]),
            response_status: 200,
            response: json!({"data": task()}),
            result_eq: ptrs([
                ("/name", json!("Login page times out on Safari")),
                ("/assignee/email", json!("ada@acme.com")),
                ("/memberships/0/section/name", json!("In review")),
                ("/tags/0/name", json!("urgent")),
                ("/num_subtasks", json!(2)),
                ("/completed", json!(false)),
            ]),
        },
        Case {
            tool: "ASANA_LIST_PROJECT_TASKS",
            args: json!({"project": PROJECT, "completed_since": "now", "limit": 100}),
            method: "GET",
            url_contains: &[
                "https://app.asana.com/api/1.0/projects/1207777777777777/tasks?",
                "completed_since=now",
                "opt_fields=name%2Cresource_subtype%2Ccompleted%2Ccompleted_at%2Ccreated_at%2Cmodified_at%2Cdue_on%2Cdue_at%2Cstart_on%2Cassignee.name%2Cparent.name%2Cpermalink_url",
                "limit=100",
            ],
            body_eq: ptrs([]),
            response_status: 200,
            response: list(json!([task()])),
            result_eq: ptrs([("/data/0/gid", json!(TASK))]),
        },
        Case {
            tool: "ASANA_LIST_SUBTASKS",
            args: json!({"task": TASK}),
            method: "GET",
            url_contains: &[
                "https://app.asana.com/api/1.0/tasks/1204444444444444/subtasks?",
                "limit=50",
            ],
            body_eq: ptrs([]),
            response_status: 200,
            response: list(json!([{"gid": TASK2, "name": "Collect screenshots"}])),
            result_eq: ptrs([("/data/0/name", json!("Collect screenshots"))]),
        },
        Case {
            tool: "ASANA_LIST_TASK_TAGS",
            args: json!({"task": TASK}),
            method: "GET",
            url_contains: &[
                "https://app.asana.com/api/1.0/tasks/1204444444444444/tags?",
                "opt_fields=name%2Ccolor%2Ccreated_at%2Cpermalink_url",
            ],
            body_eq: ptrs([]),
            response_status: 200,
            response: list(json!([{"gid": TAG, "name": "urgent", "color": "red"}])),
            result_eq: ptrs([("/data/0/name", json!("urgent"))]),
        },
        Case {
            tool: "ASANA_GET_STORY",
            args: json!({"story": STORY}),
            method: "GET",
            url_contains: &[
                "https://app.asana.com/api/1.0/stories/1202222222222222?",
                "opt_fields=created_at%2Ccreated_by.name%2Cresource_subtype%2Ctype%2Ctext%2Chtml_text%2Cis_pinned%2Cis_edited%2Cis_editable%2Csticker_name%2Ctarget.name",
            ],
            body_eq: ptrs([]),
            response_status: 200,
            response: json!({"data": story()}),
            result_eq: ptrs([
                ("/text", json!("Reproduced on Safari 17.")),
                ("/is_pinned", json!(false)),
                ("/created_by/name", json!("Ada Lovelace")),
                ("/target/name", json!("Login page times out on Safari")),
            ]),
        },
        Case {
            tool: "ASANA_LIST_TASK_STORIES",
            args: json!({"task": TASK, "limit": 10}),
            method: "GET",
            url_contains: &[
                "https://app.asana.com/api/1.0/tasks/1204444444444444/stories?",
                "opt_fields=created_at%2Ccreated_by.name%2Cresource_subtype%2Ctype%2Ctext%2Cis_pinned%2Cis_edited",
                "limit=10",
            ],
            body_eq: ptrs([]),
            response_status: 200,
            response: list(json!([
                {"gid": STORY, "created_at": "2026-09-21T09:00:00.000Z",
                 "created_by": {"gid": USER, "name": "Ada Lovelace"},
                 "resource_subtype": "comment_added", "type": "comment",
                 "text": "Reproduced on Safari 17.", "is_pinned": false, "is_edited": false}])),
            result_eq: ptrs([("/data/0/text", json!("Reproduced on Safari 17."))]),
        },
        Case {
            tool: "ASANA_SEARCH_TASKS",
            args: json!({"workspace": WORKSPACE, "text": "login timeout",
                         "resource_subtype": "default_task", "assignee_ids": ["me", USER],
                         "project_ids": [PROJECT], "completed": false,
                         "due_on_after": "2026-10-01", "modified_at_after": "2026-10-01T00:00:00Z",
                         "sort_by": "due_date", "sort_ascending": true, "limit": 25}),
            method: "GET",
            url_contains: &[
                "https://app.asana.com/api/1.0/workspaces/1201234567890123/tasks/search?",
                "text=login+timeout",
                "resource_subtype=default_task",
                "assignee.any=me%2C1205555555555555",
                "projects.any=1207777777777777",
                "completed=false",
                "due_on.after=2026-10-01",
                "modified_at.after=2026-10-01T00%3A00%3A00Z",
                "sort_by=due_date",
                "sort_ascending=true",
                "limit=25",
            ],
            body_eq: ptrs([]),
            response_status: 200,
            response: json!({"data": [
                {"gid": TASK, "name": "Login page times out on Safari", "completed": false}]}),
            result_eq: ptrs([("/data/0/name", json!("Login page times out on Safari"))]),
        },
        Case {
            tool: "ASANA_SEARCH_WORKSPACE_OBJECTS",
            args: json!({"workspace": WORKSPACE, "resource_type": "project",
                         "query": "Q4 launch", "count": 20}),
            method: "GET",
            url_contains: &[
                "https://app.asana.com/api/1.0/workspaces/1201234567890123/typeahead?",
                "resource_type=project",
                "query=Q4+launch",
                "count=20",
            ],
            body_eq: ptrs([]),
            response_status: 200,
            response: json!({"data": [
                {"gid": PROJECT, "resource_type": "project", "name": "Q4 launch"}]}),
            result_eq: ptrs([
                ("/data/0/gid", json!(PROJECT)),
                ("/data/0/name", json!("Q4 launch")),
            ]),
        },
        Case {
            tool: "ASANA_CREATE_TASK",
            args: json!({"project": PROJECT, "name": "Draft launch email",
                         "notes": "Short and to the point.", "assignee": USER,
                         "due_at": "2026-10-15T17:00:00Z", "tag_gids": [TAG]}),
            method: "POST",
            url_contains: &["https://app.asana.com/api/1.0/tasks"],
            body_eq: ptrs([
                ("/data/name", json!("Draft launch email")),
                ("/data/projects", json!([PROJECT])),
                ("/data/notes", json!("Short and to the point.")),
                ("/data/assignee", json!(USER)),
                ("/data/due_at", json!("2026-10-15T17:00:00Z")),
                ("/data/tags", json!([TAG])),
            ]),
            response_status: 200,
            response: json!({"data": task()}),
            result_eq: ptrs([("/gid", json!(TASK)), ("/name", json!("Login page times out on Safari"))]),
        },
        Case {
            tool: "ASANA_UPDATE_TASK",
            args: json!({"task": TASK, "name": "Login page times out on Safari 17",
                         "due_on": "2026-10-16", "completed": true}),
            method: "PUT",
            url_contains: &[
                "https://app.asana.com/api/1.0/tasks/1204444444444444?",
                "opt_fields=name%2Cresource_subtype%2Cnotes%2Ccompleted",
            ],
            body_eq: ptrs([
                ("/data/name", json!("Login page times out on Safari 17")),
                ("/data/due_on", json!("2026-10-16")),
                ("/data/completed", json!(true)),
            ]),
            response_status: 200,
            response: json!({"data": task()}),
            result_eq: ptrs([("/gid", json!(TASK))]),
        },
        Case {
            tool: "ASANA_DELETE_TASK",
            args: json!({"task": TASK}),
            method: "DELETE",
            url_contains: ENDPOINT,
            body_eq: ptrs([]),
            response_status: 200,
            response: json!({"data": {"gid": TASK, "resource_type": "task",
                                      "resource_subtype": "default_task", "name": "Login page times out on Safari"}}),
            result_eq: ptrs([("/success", json!(true)), ("/gid", json!(TASK))]),
        },
        Case {
            tool: "ASANA_CREATE_SUBTASK",
            args: json!({"task": TASK, "name": "Collect screenshots",
                         "notes": "Safari and iOS.", "assignee": "me",
                         "due_on": "2026-10-10", "followers": [USER]}),
            method: "POST",
            url_contains: &[
                "https://app.asana.com/api/1.0/tasks/1204444444444444/subtasks?",
                "opt_fields=name%2Cresource_subtype%2Cnotes",
            ],
            body_eq: ptrs([
                ("/data/name", json!("Collect screenshots")),
                ("/data/notes", json!("Safari and iOS.")),
                ("/data/assignee", json!("me")),
                ("/data/due_on", json!("2026-10-10")),
                ("/data/followers", json!([USER])),
            ]),
            response_status: 200,
            response: json!({"data": task()}),
            result_eq: ptrs([("/gid", json!(TASK)), ("/num_subtasks", json!(2))]),
        },
        Case {
            tool: "ASANA_ADD_TASK_COMMENT",
            args: json!({"task": TASK, "text": "Fix is in review.", "is_pinned": true}),
            method: "POST",
            url_contains: &[
                "https://app.asana.com/api/1.0/tasks/1204444444444444/stories?",
                "opt_fields=created_at%2Ccreated_by.name%2Cresource_subtype%2Ctype%2Ctext%2Chtml_text",
            ],
            body_eq: ptrs([
                ("/data/text", json!("Fix is in review.")),
                ("/data/is_pinned", json!(true)),
            ]),
            response_status: 201,
            response: json!({"data": story()}),
            result_eq: ptrs([("/gid", json!(STORY)), ("/is_editable", json!(true))]),
        },
        Case {
            tool: "ASANA_ADD_TAG_TO_TASK",
            args: json!({"task": TASK, "tag": TAG}),
            method: "POST",
            url_contains: &["https://app.asana.com/api/1.0/tasks/1204444444444444/addTag"],
            body_eq: ptrs([("/data/tag", json!(TAG))]),
            response_status: 200,
            response: json!({"data": {"gid": TASK, "resource_type": "task",
                                      "name": "Login page times out on Safari"}}),
            result_eq: ptrs([("/success", json!(true)), ("/gid", json!(TASK))]),
        },
        Case {
            tool: "ASANA_ADD_TASK_TO_PROJECT",
            args: json!({"task": TASK, "project": PROJECT, "section": SECTION, "insert_after": TASK2}),
            method: "POST",
            url_contains: &["https://app.asana.com/api/1.0/tasks/1204444444444444/addProject"],
            body_eq: ptrs([
                ("/data/project", json!(PROJECT)),
                ("/data/section", json!(SECTION)),
                ("/data/insert_after", json!(TASK2)),
            ]),
            response_status: 200,
            response: json!({"data": {"gid": TASK, "resource_type": "task"}}),
            result_eq: ptrs([("/success", json!(true))]),
        },
        Case {
            tool: "ASANA_MOVE_TASK_TO_SECTION",
            args: json!({"section": SECTION, "task": TASK, "insert_before": TASK2}),
            method: "POST",
            url_contains: &["https://app.asana.com/api/1.0/sections/1208888888888888/addTask"],
            body_eq: ptrs([
                ("/data/task", json!(TASK)),
                ("/data/insert_before", json!(TASK2)),
            ]),
            response_status: 200,
            response: json!({"data": {"gid": TASK, "resource_type": "task"}}),
            result_eq: ptrs([("/success", json!(true))]),
        },
        Case {
            tool: "ASANA_CREATE_SECTION",
            args: json!({"project": PROJECT, "name": "Done", "insert_after": SECTION}),
            method: "POST",
            url_contains: &[
                "https://app.asana.com/api/1.0/projects/1207777777777777/sections?",
                "opt_fields=name%2Ccreated_at%2Cproject.name",
            ],
            body_eq: ptrs([
                ("/data/name", json!("Done")),
                ("/data/insert_after", json!(SECTION)),
            ]),
            response_status: 201,
            response: json!({"data": {"gid": "1201111111111111", "name": "Done",
                "created_at": "2026-10-01T09:00:00.000Z",
                "project": {"gid": PROJECT, "name": "Platform hardening"}}}),
            result_eq: ptrs([("/name", json!("Done")), ("/gid", json!("1201111111111111"))]),
        },
    ];
    support::assert_full_coverage(&a, &cases);
    for c in &cases {
        run(&a, c);
    }
}

/// Provider errors (`{"errors": [{"message": …}]}`) become tool errors.
#[test]
fn asana_errors_surface_as_tool_errors() {
    use ctox_toolhub::http::HttpResponse;
    use ctox_toolhub::toolkits::ToolkitAdapter;

    let a = adapter();
    let resp = HttpResponse {
        status: 404,
        body: serde_json::to_vec(
            &json!({"errors": [{"message": "Not a recognized ID: 1204444444444444"}]}),
        )
        .unwrap(),
        session_id: None,
    };
    let err = a.parse_response("ASANA_GET_TASK", &resp).unwrap_err();
    assert!(
        err.to_string().contains("Not a recognized ID: 1204444444444444"),
        "{err}"
    );
}

/// Live read-only calls through the hub exec path (`Authorization: Bearer <token>`).
#[test]
#[ignore]
fn live_read_only_actions() {
    let a = adapter();
    for (tool, args) in [
        ("ASANA_GET_CURRENT_USER", json!({})),
        ("ASANA_GET_USER", json!({"user": "me"})),
        ("ASANA_LIST_WORKSPACES", json!({"limit": 1})),
    ] {
        let Some(result) = live(&a, tool, args) else { return };
        let v = result.unwrap_or_else(|e| panic!("{tool}: {e}"));
        assert!(v.is_object(), "{tool}: expected an object, got {v}");
    }
}