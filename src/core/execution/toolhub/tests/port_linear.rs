//! Linear port: one recorded case per action, expectations derived from the
//! Activepieces `run()` code (all calls are `POST /graphql` with `{query, variables}`).
mod support;

use ctox_toolhub::toolkits::linear::Linear;
use serde_json::{json, Value};
use support::{live, run, Case};

const ENDPOINT: &str = "https://linear.test/graphql";
const URL: &[&str] = &[ENDPOINT];

const TEAM: &str = "3d4b0f1e-1c2a-4f3b-8a5e-9c7d2e6f1a10";
const USER: &str = "9f2c7a44-5b3e-4d6c-9a1b-2c8e5f7d3b21";
const STATE: &str = "a4b5c6d7-e8f9-4a0b-9c1d-2e3f4a5b6c7d";
const LABEL: &str = "b7c8d9e0-f1a2-4b3c-8d4e-5f6a7b8c9d0e";
const PROJECT: &str = "c8d9e0f1-a2b3-4c4d-9e5f-6a7b8c9d0e1f";
const CYCLE: &str = "d9e0f1a2-b3c4-4d5e-8f6a-7b8c9d0e1f2a";
const ISSUE: &str = "e0f1a2b3-c4d5-4e6f-9a7b-8c9d0e1f2a3b";
const ISSUE2: &str = "f1a2b3c4-d5e6-4f7a-8b8c-9d0e1f2a3b4c";
const COMMENT: &str = "a2b3c4d5-e6f7-4a8b-9c9d-0e1f2a3b4c5d";
const REPLY: &str = "b3c4d5e6-f7a8-4b9c-8d0e-1f2a3b4c5d6e";
const UPDATE: &str = "c4d5e6f7-a8b9-4c0d-9e1f-2a3b4c5d6e7f";

fn adapter() -> Linear {
    Linear::with_api_base("https://linear.test")
}

fn issue(id: &str, identifier: &str) -> Value {
    json!({
        "id": id, "identifier": identifier, "number": 123, "title": "Login page times out on Safari",
        "description": "Safari shows a spinner forever.", "url": "https://linear.app/acme/issue/ENG-123",
        "priority": 2, "priorityLabel": "High", "estimate": 3, "dueDate": "2026-10-15",
        "branchName": "feat/login-timeout", "createdAt": "2026-09-01T10:00:00.000Z",
        "updatedAt": "2026-09-20T08:30:00.000Z", "startedAt": "2026-09-05T09:00:00.000Z",
        "completedAt": null, "canceledAt": null, "archivedAt": null, "trashed": false,
        "state": {"id": STATE, "name": "In Progress", "type": "started"},
        "team": {"id": TEAM, "key": "ENG", "name": "Engineering"},
        "assignee": {"id": USER, "name": "Ada Lovelace", "email": "ada@acme.com"},
        "creator": {"id": USER, "name": "Ada Lovelace", "email": "ada@acme.com"},
        "project": {"id": PROJECT, "name": "Platform hardening"},
        "projectMilestone": null,
        "cycle": {"id": CYCLE, "number": 7, "name": "Q3 hardening"},
        "parent": null,
        "labels": {"pageInfo": {"hasNextPage": false, "endCursor": null},
                   "nodes": [{"id": LABEL, "name": "Bug"}]}
    })
}

fn comment(id: &str, body: &str) -> Value {
    json!({
        "id": id, "body": body, "url": "https://linear.app/acme/comment/1",
        "createdAt": "2026-09-21T09:00:00.000Z", "updatedAt": "2026-09-21T09:00:00.000Z",
        "editedAt": null, "resolvedAt": null,
        "user": {"id": USER, "name": "Ada Lovelace", "email": "ada@acme.com"},
        "resolvingUser": null, "issue": {"id": ISSUE, "identifier": "ENG-123", "title": "Login page times out on Safari"},
        "parent": null
    })
}

/// `&[(ptr, value)]` for a `Case` field: leaked so it lives as long as the case.
fn ptrs<const N: usize>(pairs: [(&'static str, Value); N]) -> &'static [(&'static str, Value)] {
    Box::leak(Box::new(pairs))
}

fn connection(nodes: Value) -> Value {
    json!({"pageInfo": {"hasNextPage": false, "endCursor": null}, "nodes": nodes})
}

#[test]
fn linear_actions_are_ported() {
    let a = adapter();
    let cases: Vec<Case> = vec![
        Case {
            tool: "LINEAR_GET_VIEWER",
            args: json!({}),
            method: "POST",
            url_contains: URL,
            body_eq: ptrs([]),
            response_status: 200,
            response: json!({"data": {"viewer": {
                "id": USER, "name": "Ada Lovelace", "displayName": "ada", "email": "ada@acme.com",
                "active": true, "admin": true, "guest": false, "url": "https://linear.app/acme/u/ada",
                "avatarUrl": "https://avatar.linear.app/ada", "timezone": "Europe/Berlin",
                "createdAt": "2024-02-01T09:00:00.000Z",
                "organization": {"id": "0a1b2c3d-4e5f-4a6b-8c7d-9e0f1a2b3c4d", "name": "Acme", "urlKey": "acme"}}}}),
            result_eq: ptrs([
                ("/email", json!("ada@acme.com")),
                ("/organization_name", json!("Acme")),
                ("/organization_url_key", json!("acme")),
                ("/admin", json!(true)),
            ]),
        },
        Case {
            tool: "LINEAR_LIST_TEAMS",
            args: json!({"limit": 2, "include_archived": true}),
            method: "POST",
            url_contains: URL,
            body_eq: ptrs([
                ("/variables/first", json!(2)),
                ("/variables/includeArchived", json!(true)),
            ]),
            response_status: 200,
            response: json!({"data": {"teams": connection(json!([
                {"id": TEAM, "key": "ENG", "name": "Engineering", "description": "Product engineering",
                 "icon": "hexagon", "color": "#5E6AD2", "timezone": "Europe/Berlin", "cyclesEnabled": true,
                 "triageEnabled": false, "defaultIssueEstimate": 1, "issueEstimationType": "points",
                 "createdAt": "2024-02-01T09:00:00.000Z", "updatedAt": "2026-09-01T09:00:00.000Z",
                 "archivedAt": null}]))}}),
            result_eq: ptrs([
                ("/count", json!(1)),
                ("/items/0/key", json!("ENG")),
                ("/items/0/cycles_enabled", json!(true)),
                ("/has_next_page", json!(false)),
            ]),
        },
        Case {
            tool: "LINEAR_GET_TEAM",
            args: json!({"team_id": TEAM}),
            method: "POST",
            url_contains: URL,
            body_eq: ptrs([("/variables/id", json!(TEAM))]),
            response_status: 200,
            response: json!({"data": {"team": {
                "id": TEAM, "key": "ENG", "name": "Engineering", "timezone": "Europe/Berlin",
                "cyclesEnabled": true, "triageEnabled": false, "defaultIssueEstimate": 1,
                "issueEstimationType": "points", "createdAt": "2024-02-01T09:00:00.000Z",
                "updatedAt": "2026-09-01T09:00:00.000Z", "archivedAt": null,
                "issueCount": 412,
                "defaultIssueState": {"id": STATE, "name": "Todo", "type": "unstarted"}}}}),
            result_eq: ptrs([
                ("/name", json!("Engineering")),
                ("/issue_count", json!(412)),
                ("/default_issue_state_type", json!("unstarted")),
            ]),
        },
        Case {
            tool: "LINEAR_LIST_USERS",
            args: json!({"email": "ada@acme.com", "limit": 1}),
            method: "POST",
            url_contains: URL,
            body_eq: ptrs([
                ("/variables/filter/email/eqIgnoreCase", json!("ada@acme.com")),
                ("/variables/first", json!(1)),
                ("/variables/includeDisabled", json!(false)),
            ]),
            response_status: 200,
            response: json!({"data": {"users": connection(json!([
                {"id": USER, "name": "Ada Lovelace", "displayName": "ada", "email": "ada@acme.com",
                 "active": true, "admin": true, "guest": false, "url": "https://linear.app/acme/u/ada",
                 "avatarUrl": null, "timezone": "Europe/Berlin", "createdAt": "2024-02-01T09:00:00.000Z"}]))}}),
            result_eq: ptrs([("/items/0/id", json!(USER)), ("/items/0/name", json!("Ada Lovelace"))]),
        },
        Case {
            tool: "LINEAR_LIST_WORKFLOW_STATES",
            args: json!({"team_id": TEAM}),
            method: "POST",
            url_contains: URL,
            body_eq: ptrs([("/variables/filter/team/id/eq", json!(TEAM)), ("/variables/first", json!(100))]),
            response_status: 200,
            response: json!({"data": {"workflowStates": connection(json!([
                {"id": STATE, "name": "In Progress", "type": "started", "color": "#f2c94c",
                 "position": 2, "description": "Work in progress",
                 "team": {"id": TEAM, "key": "ENG", "name": "Engineering"}}]))}}),
            result_eq: ptrs([("/items/0/name", json!("In Progress")), ("/items/0/type", json!("started"))]),
        },
        Case {
            tool: "LINEAR_LIST_CYCLES",
            args: json!({"team_id": TEAM, "which": "isActive"}),
            method: "POST",
            url_contains: URL,
            body_eq: ptrs([
                ("/variables/filter/team/id/eq", json!(TEAM)),
                ("/variables/filter/isActive/eq", json!(true)),
            ]),
            response_status: 200,
            response: json!({"data": {"cycles": connection(json!([
                {"id": CYCLE, "number": 7, "name": "Q3 hardening", "startsAt": "2026-09-01T00:00:00.000Z",
                 "endsAt": "2026-09-14T00:00:00.000Z", "completedAt": null, "progress": 0.6,
                 "isActive": true, "isNext": false, "isPrevious": false,
                 "team": {"id": TEAM, "key": "ENG", "name": "Engineering"}}]))}}),
            result_eq: ptrs([("/items/0/number", json!(7)), ("/items/0/is_active", json!(true))]),
        },
        Case {
            tool: "LINEAR_LIST_ISSUE_LABELS",
            args: json!({"team_id": TEAM, "name_contains": "bug"}),
            method: "POST",
            url_contains: URL,
            body_eq: ptrs([
                ("/variables/filter/name/containsIgnoreCase", json!("bug")),
                ("/variables/filter/or/0/team/id/eq", json!(TEAM)),
                ("/variables/filter/or/1/team/null", json!(true)),
            ]),
            response_status: 200,
            response: json!({"data": {"issueLabels": connection(json!([
                {"id": LABEL, "name": "Bug", "color": "#eb5757", "description": "Something is broken",
                 "isGroup": false, "createdAt": "2024-02-01T09:00:00.000Z",
                 "team": {"id": TEAM, "key": "ENG", "name": "Engineering"}, "parent": null}]))}}),
            result_eq: ptrs([("/items/0/name", json!("Bug")), ("/items/0/is_workspace_label", json!(false))]),
        },
        Case {
            tool: "LINEAR_GET_ISSUE",
            args: json!({"issue_id": "ENG-123"}),
            method: "POST",
            url_contains: URL,
            body_eq: ptrs([("/variables/id", json!("ENG-123"))]),
            response_status: 200,
            response: json!({"data": {"issue": issue(ISSUE, "ENG-123")}}),
            result_eq: ptrs([
                ("/identifier", json!("ENG-123")),
                ("/title", json!("Login page times out on Safari")),
                ("/state_name", json!("In Progress")),
                ("/assignee_email", json!("ada@acme.com")),
                ("/label_names", json!("Bug")),
                ("/due_date", json!("2026-10-15")),
                ("/labels_complete", json!(true)),
            ]),
        },
        Case {
            tool: "LINEAR_LIST_ISSUES",
            args: json!({"team_id": TEAM, "state_type": "started", "label_id": LABEL,
                         "priority": 2, "order_by": "updatedAt", "include_archived": false}),
            method: "POST",
            url_contains: URL,
            body_eq: ptrs([
                ("/variables/filter/team/id/eq", json!(TEAM)),
                ("/variables/filter/state/type/eq", json!("started")),
                ("/variables/filter/labels/some/id/eq", json!(LABEL)),
                ("/variables/filter/priority/eq", json!(2)),
                ("/variables/orderBy", json!("updatedAt")),
                ("/variables/first", json!(50)),
            ]),
            response_status: 200,
            response: json!({"data": {"issues": connection(json!([issue(ISSUE, "ENG-123")]))}}),
            result_eq: ptrs([("/count", json!(1)), ("/items/0/identifier", json!("ENG-123"))]),
        },
        Case {
            tool: "LINEAR_SEARCH_ISSUES",
            args: json!({"term": "login timeout safari", "team_id": TEAM, "limit": 25}),
            method: "POST",
            url_contains: URL,
            body_eq: ptrs([
                ("/variables/term", json!("login timeout safari")),
                ("/variables/filter/team/id/eq", json!(TEAM)),
                ("/variables/includeComments", json!(false)),
                ("/variables/first", json!(25)),
            ]),
            response_status: 200,
            response: json!({"data": {"searchIssues": {
                "totalCount": 1,
                "pageInfo": {"hasNextPage": false, "endCursor": null},
                "nodes": [issue(ISSUE, "ENG-123")]}}}),
            result_eq: ptrs([("/total_count", json!(1)), ("/items/0/title", json!("Login page times out on Safari"))]),
        },
        Case {
            tool: "LINEAR_GET_COMMENT",
            args: json!({"comment_id": COMMENT}),
            method: "POST",
            url_contains: URL,
            body_eq: ptrs([("/variables/id", json!(COMMENT))]),
            response_status: 200,
            response: json!({"data": {"comment": comment(COMMENT, "Reproduced on Safari 17.")}}),
            result_eq: ptrs([("/body", json!("Reproduced on Safari 17.")), ("/user_name", json!("Ada Lovelace"))]),
        },
        Case {
            tool: "LINEAR_LIST_COMMENTS",
            args: json!({"issue_id": ISSUE, "limit": 50}),
            method: "POST",
            url_contains: URL,
            body_eq: ptrs([
                ("/variables/filter/issue/id/eq", json!(ISSUE)),
                ("/variables/first", json!(50)),
            ]),
            response_status: 200,
            response: json!({"data": {"comments": connection(json!([
                comment(COMMENT, "Reproduced on Safari 17."),
                comment(REPLY, "Fix is in review.")])).as_object()
                .map(|_| connection(json!([
                    comment(COMMENT, "Reproduced on Safari 17."),
                    comment(REPLY, "Fix is in review.")])))
                .unwrap()}}),
            result_eq: ptrs([("/count", json!(2)), ("/items/1/body", json!("Fix is in review."))]),
        },
        Case {
            tool: "LINEAR_GET_PROJECT",
            args: json!({"project_id": PROJECT}),
            method: "POST",
            url_contains: URL,
            body_eq: ptrs([("/variables/id", json!(PROJECT))]),
            response_status: 200,
            response: json!({"data": {"project": {
                "id": PROJECT, "name": "Platform hardening", "description": "Make the platform boring.",
                "url": "https://linear.app/acme/project/platform-hardening", "icon": "shield",
                "color": "#5E6AD2", "priority": 2, "priorityLabel": "High", "progress": 0.42,
                "health": "atRisk", "startDate": "2026-09-01", "targetDate": "2026-11-30",
                "createdAt": "2026-08-01T09:00:00.000Z", "updatedAt": "2026-09-20T09:00:00.000Z",
                "completedAt": null, "canceledAt": null, "archivedAt": null, "trashed": false,
                "status": {"id": "d0e1f2a3-b4c5-4d6e-8f7a-9b8c7d6e5f4a", "name": "In Progress", "type": "started"},
                "lead": {"id": USER, "name": "Ada Lovelace", "email": "ada@acme.com"},
                "creator": {"id": USER, "name": "Ada Lovelace"},
                "teams": {"pageInfo": {"hasNextPage": false, "endCursor": null},
                          "nodes": [{"id": TEAM, "key": "ENG", "name": "Engineering"}]},
                "projectMilestones": {"pageInfo": {"hasNextPage": false, "endCursor": null},
                    "nodes": [{"id": "e1f2a3b4-c5d6-4e7f-8a9b-0c8d7e6f5a4b", "name": "Beta",
                               "targetDate": "2026-11-30", "status": "next"}]}}}}),
            result_eq: ptrs([
                ("/name", json!("Platform hardening")),
                ("/health", json!("atRisk")),
                ("/team_names", json!("Engineering")),
                ("/milestones/0/name", json!("Beta")),
                ("/milestones_complete", json!(true)),
            ]),
        },
        Case {
            tool: "LINEAR_LIST_PROJECTS",
            args: json!({"team_id": TEAM, "status_type": "started"}),
            method: "POST",
            url_contains: URL,
            body_eq: ptrs([
                ("/variables/filter/accessibleTeams/some/id/eq", json!(TEAM)),
                ("/variables/filter/status/type/eq", json!("started")),
            ]),
            response_status: 200,
            response: json!({"data": {"projects": connection(json!([
                {"id": PROJECT, "name": "Platform hardening", "url": "https://linear.app/acme/project/platform-hardening",
                 "priority": 2, "priorityLabel": "High", "progress": 0.42, "health": "atRisk",
                 "createdAt": "2026-08-01T09:00:00.000Z", "updatedAt": "2026-09-20T09:00:00.000Z",
                 "trashed": false,
                 "status": {"id": "d0e1f2a3-b4c5-4d6e-8f7a-9b8c7d6e5f4a", "name": "In Progress", "type": "started"},
                 "teams": {"pageInfo": {"hasNextPage": false, "endCursor": null},
                           "nodes": [{"id": TEAM, "key": "ENG", "name": "Engineering"}]}}]))}}),
            result_eq: ptrs([("/count", json!(1)), ("/items/0/health", json!("atRisk"))]),
        },
        Case {
            tool: "LINEAR_GET_PROJECT_STATUS_UPDATE",
            args: json!({"status_update_id": UPDATE}),
            method: "POST",
            url_contains: URL,
            body_eq: ptrs([("/variables/id", json!(UPDATE))]),
            response_status: 200,
            response: json!({"data": {"projectUpdate": {
                "id": UPDATE, "body": "Beta is on track.", "health": "onTrack",
                "url": "https://linear.app/acme/project/platform-hardening/update/1", "isDiffHidden": false,
                "createdAt": "2026-09-20T10:00:00.000Z", "updatedAt": "2026-09-20T10:00:00.000Z",
                "editedAt": null, "archivedAt": null,
                "user": {"id": USER, "name": "Ada Lovelace", "email": "ada@acme.com"},
                "project": {"id": PROJECT, "name": "Platform hardening"}}}}),
            result_eq: ptrs([("/body", json!("Beta is on track.")), ("/health", json!("onTrack"))]),
        },
        Case {
            tool: "LINEAR_LIST_PROJECT_STATUS_UPDATES",
            args: json!({"project_id": PROJECT, "limit": 25}),
            method: "POST",
            url_contains: URL,
            body_eq: ptrs([("/variables/filter/project/id/eq", json!(PROJECT)), ("/variables/first", json!(25))]),
            response_status: 200,
            response: json!({"data": {"projectUpdates": connection(json!([
                {"id": UPDATE, "body": "Beta is on track.", "health": "onTrack",
                 "url": "https://linear.app/acme/project/platform-hardening/update/1", "isDiffHidden": false,
                 "createdAt": "2026-09-20T10:00:00.000Z", "updatedAt": "2026-09-20T10:00:00.000Z",
                 "editedAt": null, "archivedAt": null,
                 "user": {"id": USER, "name": "Ada Lovelace", "email": "ada@acme.com"},
                 "project": {"id": PROJECT, "name": "Platform hardening"}}]))}}),
            result_eq: ptrs([("/count", json!(1)), ("/items/0/project_name", json!("Platform hardening"))]),
        },
        Case {
            tool: "LINEAR_GET_ATTACHMENT",
            args: json!({"attachment_id": LABEL}),
            method: "POST",
            url_contains: URL,
            body_eq: ptrs([("/variables/id", json!(LABEL))]),
            response_status: 200,
            response: json!({"data": {"attachment": {
                "id": LABEL, "title": "safari-trace.zip", "subtitle": "2.1 MB",
                "url": "https://files.linear.app/safari-trace.zip", "sourceType": "upload",
                "createdAt": "2026-09-21T08:00:00.000Z", "updatedAt": "2026-09-21T08:00:00.000Z",
                "issue": {"id": ISSUE, "identifier": "ENG-123", "title": "Login page times out on Safari"},
                "creator": {"id": USER, "name": "Ada Lovelace"}}}}),
            result_eq: ptrs([
                ("/title", json!("safari-trace.zip")),
                ("/issue_identifier", json!("ENG-123")),
                ("/source_type", json!("upload")),
            ]),
        },
        Case {
            tool: "LINEAR_CREATE_ISSUE",
            args: json!({"team_id": TEAM, "title": "Login page times out on Safari",
                         "description": "Spinner never stops.", "priority": 2,
                         "label_ids": [LABEL], "due_date": "2026-10-15", "estimate": 3}),
            method: "POST",
            url_contains: URL,
            body_eq: ptrs([
                ("/variables/input/teamId", json!(TEAM)),
                ("/variables/input/title", json!("Login page times out on Safari")),
                ("/variables/input/priority", json!(2)),
                ("/variables/input/labelIds", json!([LABEL])),
                ("/variables/input/dueDate", json!("2026-10-15")),
                ("/variables/input/estimate", json!(3)),
            ]),
            response_status: 200,
            response: json!({"data": {"issueCreate": {"success": true, "issue": issue(ISSUE, "ENG-123")}}}),
            result_eq: ptrs([("/identifier", json!("ENG-123")), ("/title", json!("Login page times out on Safari"))]),
        },
        Case {
            tool: "LINEAR_UPDATE_ISSUE",
            args: json!({"issue_id": ISSUE, "state_id": STATE, "add_label_ids": [LABEL],
                         "clear_fields": ["dueDate"]}),
            method: "POST",
            url_contains: URL,
            body_eq: ptrs([
                ("/variables/id", json!(ISSUE)),
                ("/variables/input/stateId", json!(STATE)),
                ("/variables/input/addedLabelIds", json!([LABEL])),
                ("/variables/input/dueDate", Value::Null),
            ]),
            response_status: 200,
            response: json!({"data": {"issueUpdate": {"success": true, "issue": issue(ISSUE, "ENG-123")}}}),
            result_eq: ptrs([("/state_id", json!(STATE)), ("/label_ids", json!([LABEL]))]),
        },
        Case {
            tool: "LINEAR_ARCHIVE_ISSUE",
            args: json!({"issue_id": "ENG-123"}),
            method: "POST",
            url_contains: URL,
            body_eq: ptrs([("/variables/id", json!("ENG-123"))]),
            response_status: 200,
            response: json!({"data": {"issueArchive": {"success": true, "entity": {
                "id": ISSUE, "identifier": "ENG-123", "title": "Login page times out on Safari",
                "url": "https://linear.app/acme/issue/ENG-123", "trashed": false,
                "archivedAt": "2026-09-22T09:00:00.000Z"}}}}),
            result_eq: ptrs([("/success", json!(true)), ("/archived_at", json!("2026-09-22T09:00:00.000Z"))]),
        },
        Case {
            tool: "LINEAR_UNARCHIVE_ISSUE",
            args: json!({"issue_id": ISSUE}),
            method: "POST",
            url_contains: URL,
            body_eq: ptrs([("/variables/id", json!(ISSUE))]),
            response_status: 200,
            response: json!({"data": {"issueUnarchive": {"success": true, "entity": {
                "id": ISSUE, "identifier": "ENG-123", "title": "Login page times out on Safari",
                "url": "https://linear.app/acme/issue/ENG-123", "trashed": false, "archivedAt": null}}}}),
            result_eq: ptrs([("/success", json!(true)), ("/archived_at", Value::Null)]),
        },
        Case {
            tool: "LINEAR_DELETE_ISSUE",
            args: json!({"issue_id": ISSUE}),
            method: "POST",
            url_contains: URL,
            body_eq: ptrs([("/variables/id", json!(ISSUE))]),
            response_status: 200,
            response: json!({"data": {"issueDelete": {"success": true, "entity": {
                "id": ISSUE, "identifier": "ENG-123", "title": "Login page times out on Safari",
                "url": "https://linear.app/acme/issue/ENG-123", "trashed": true,
                "archivedAt": "2026-09-22T09:00:00.000Z"}}}}),
            result_eq: ptrs([("/success", json!(true)), ("/trashed", json!(true))]),
        },
        Case {
            tool: "LINEAR_ADD_ISSUE_LABEL",
            args: json!({"issue_id": "eng-123", "label_id": LABEL}),
            method: "POST",
            url_contains: URL,
            body_eq: ptrs([("/variables/id", json!("ENG-123")), ("/variables/labelId", json!(LABEL))]),
            response_status: 200,
            response: json!({"data": {"issueAddLabel": {"success": true, "issue": issue(ISSUE, "ENG-123")}}}),
            result_eq: ptrs([("/label_ids", json!([LABEL]))]),
        },
        Case {
            tool: "LINEAR_REMOVE_ISSUE_LABEL",
            args: json!({"issue_id": ISSUE, "label_id": LABEL}),
            method: "POST",
            url_contains: URL,
            body_eq: ptrs([("/variables/id", json!(ISSUE)), ("/variables/labelId", json!(LABEL))]),
            response_status: 200,
            response: json!({"data": {"issueRemoveLabel": {"success": true, "issue": {
                "id": ISSUE, "identifier": "ENG-123", "title": "Login page times out on Safari",
                "url": "https://linear.app/acme/issue/ENG-123", "priority": 0, "priorityLabel": "No priority",
                "labels": {"pageInfo": {"hasNextPage": false, "endCursor": null}, "nodes": []}}}}}),
            result_eq: ptrs([("/label_ids", json!([])), ("/label_names", json!(""))]),
        },
        Case {
            tool: "LINEAR_CREATE_ISSUE_RELATION",
            args: json!({"issue_id": ISSUE, "related_issue_id": "ENG-124", "type": "blocks"}),
            method: "POST",
            url_contains: URL,
            body_eq: ptrs([("/variables/input", json!({"issueId": ISSUE, "relatedIssueId": "ENG-124", "type": "blocks"}))]),
            response_status: 200,
            response: {
                let relation = json!({
                    "id": "f2a3b4c5-d6e7-4a8b-9c0d-1e2f3a4b5c6d",
                    "type": "blocks",
                    "createdAt": "2026-09-22T09:00:00.000Z",
                    "issue": {"id": ISSUE, "identifier": "ENG-123", "title": "Login page times out on Safari"},
                    "relatedIssue": {"id": ISSUE2, "identifier": "ENG-124", "title": "Release 4.2"}
                });
                json!({"data": {"issueRelationCreate": {"success": true, "issueRelation": relation}}})
            },
            result_eq: ptrs([("/type", json!("blocks")), ("/related_issue_identifier", json!("ENG-124"))]),
        },
        Case {
            tool: "LINEAR_CREATE_COMMENT",
            args: json!({"issue_id": "ENG-123", "body": "Reproduced on Safari 17."}),
            method: "POST",
            url_contains: URL,
            body_eq: ptrs([
                ("/variables/input/issueId", json!("ENG-123")),
                ("/variables/input/body", json!("Reproduced on Safari 17.")),
            ]),
            response_status: 200,
            response: json!({"data": {"commentCreate": {"success": true,
                "comment": comment(COMMENT, "Reproduced on Safari 17.")}}}),
            result_eq: ptrs([("/body", json!("Reproduced on Safari 17.")), ("/issue_identifier", json!("ENG-123"))]),
        },
        Case {
            tool: "LINEAR_UPDATE_COMMENT",
            args: json!({"comment_id": COMMENT, "body": "Fix is in review."}),
            method: "POST",
            url_contains: URL,
            body_eq: ptrs([
                ("/variables/id", json!(COMMENT)),
                ("/variables/input/body", json!("Fix is in review.")),
            ]),
            response_status: 200,
            response: json!({"data": {"commentUpdate": {"success": true,
                "comment": comment(COMMENT, "Fix is in review.")}}}),
            result_eq: ptrs([("/body", json!("Fix is in review."))]),
        },
        Case {
            tool: "LINEAR_RESOLVE_COMMENT",
            args: json!({"comment_id": COMMENT, "resolving_comment_id": REPLY}),
            method: "POST",
            url_contains: URL,
            body_eq: ptrs([
                ("/variables/id", json!(COMMENT)),
                ("/variables/resolvingCommentId", json!(REPLY)),
            ]),
            response_status: 200,
            response: json!({"data": {"commentResolve": {"success": true, "comment": {
                "id": COMMENT, "body": "Reproduced on Safari 17.", "url": "https://linear.app/acme/comment/1",
                "createdAt": "2026-09-21T09:00:00.000Z", "updatedAt": "2026-09-22T09:00:00.000Z",
                "resolvedAt": "2026-09-22T09:00:00.000Z",
                "resolvingUser": {"id": USER, "name": "Ada Lovelace"},
                "issue": {"id": ISSUE, "identifier": "ENG-123", "title": "Login page times out on Safari"},
                "parent": null}}}}),
            result_eq: ptrs([("/resolved_at", json!("2026-09-22T09:00:00.000Z"))]),
        },
        Case {
            tool: "LINEAR_UNRESOLVE_COMMENT",
            args: json!({"comment_id": COMMENT}),
            method: "POST",
            url_contains: URL,
            body_eq: ptrs([("/variables/id", json!(COMMENT))]),
            response_status: 200,
            response: json!({"data": {"commentUnresolve": {"success": true,
                "comment": comment(COMMENT, "Reproduced on Safari 17.")}}}),
            result_eq: ptrs([("/id", json!(COMMENT)), ("/resolved_at", Value::Null)]),
        },
        Case {
            tool: "LINEAR_CREATE_TEAM",
            args: json!({"name": "Platform", "key": "PLT", "cycles_enabled": "true"}),
            method: "POST",
            url_contains: URL,
            body_eq: ptrs([
                ("/variables/input/name", json!("Platform")),
                ("/variables/input/key", json!("PLT")),
                ("/variables/input/cyclesEnabled", json!(true)),
            ]),
            response_status: 200,
            response: json!({"data": {"teamCreate": {"success": true, "team": {
                "id": "a1b2c3d4-e5f6-4a7b-8c9d-0e1f2a3b4c5d", "key": "PLT", "name": "Platform",
                "description": null, "timezone": "Etc/UTC", "cyclesEnabled": true, "triageEnabled": false,
                "defaultIssueEstimate": 0, "issueEstimationType": "points",
                "createdAt": "2026-09-22T09:00:00.000Z", "updatedAt": "2026-09-22T09:00:00.000Z",
                "archivedAt": null}}}}),
            result_eq: ptrs([("/key", json!("PLT")), ("/cycles_enabled", json!(true))]),
        },
        Case {
            tool: "LINEAR_UPDATE_TEAM",
            args: json!({"team_id": TEAM, "description": "Product engineering", "triage_enabled": "true"}),
            method: "POST",
            url_contains: URL,
            body_eq: ptrs([
                ("/variables/id", json!(TEAM)),
                ("/variables/input/description", json!("Product engineering")),
                ("/variables/input/triageEnabled", json!(true)),
            ]),
            response_status: 200,
            response: json!({"data": {"teamUpdate": {"success": true, "team": {
                "id": TEAM, "key": "ENG", "name": "Engineering", "description": "Product engineering",
                "timezone": "Europe/Berlin", "cyclesEnabled": true, "triageEnabled": true,
                "createdAt": "2024-02-01T09:00:00.000Z", "updatedAt": "2026-09-22T09:00:00.000Z",
                "archivedAt": null}}}}),
            result_eq: ptrs([("/triage_enabled", json!(true))]),
        },
        Case {
            tool: "LINEAR_CREATE_PROJECT",
            args: json!({"team_ids": [TEAM], "name": "Platform hardening", "target_date": "2026-11-30",
                         "priority": 2}),
            method: "POST",
            url_contains: URL,
            body_eq: ptrs([
                ("/variables/input/teamIds", json!([TEAM])),
                ("/variables/input/name", json!("Platform hardening")),
                ("/variables/input/targetDate", json!("2026-11-30")),
                ("/variables/input/priority", json!(2)),
            ]),
            response_status: 200,
            response: {
                let project = json!({
                    "id": PROJECT, "name": "Platform hardening",
                    "url": "https://linear.app/acme/project/platform-hardening",
                    "priority": 2, "priorityLabel": "High", "progress": 0.0, "targetDate": "2026-11-30",
                    "createdAt": "2026-09-22T09:00:00.000Z", "updatedAt": "2026-09-22T09:00:00.000Z",
                    "teams": connection(json!([{"id": TEAM, "key": "ENG", "name": "Engineering"}]))
                });
                json!({"data": {"projectCreate": {"success": true, "project": project}}})
            },
            result_eq: ptrs([("/name", json!("Platform hardening")), ("/team_ids", json!([TEAM]))]),
        },
        Case {
            tool: "LINEAR_UPDATE_PROJECT",
            args: json!({"project_id": PROJECT, "status_id": STATE, "clear_fields": ["icon"]}),
            method: "POST",
            url_contains: URL,
            body_eq: ptrs([
                ("/variables/id", json!(PROJECT)),
                ("/variables/input/statusId", json!(STATE)),
                ("/variables/input/icon", Value::Null),
            ]),
            response_status: 200,
            response: {
                let project = json!({
                    "id": PROJECT, "name": "Platform hardening",
                    "url": "https://linear.app/acme/project/platform-hardening",
                    "icon": null, "priority": 2, "priorityLabel": "High", "progress": 0.42,
                    "status": {"id": STATE, "name": "In Progress", "type": "started"},
                    "createdAt": "2026-08-01T09:00:00.000Z", "updatedAt": "2026-09-22T09:00:00.000Z",
                    "teams": connection(json!([{"id": TEAM, "key": "ENG", "name": "Engineering"}]))
                });
                json!({"data": {"projectUpdate": {"success": true, "project": project}}})
            },
            result_eq: ptrs([("/icon", Value::Null), ("/status_name", json!("In Progress"))]),
        },
        Case {
            tool: "LINEAR_CREATE_PROJECT_STATUS_UPDATE",
            args: json!({"project_id": PROJECT, "body": "Beta is on track.", "health": "onTrack"}),
            method: "POST",
            url_contains: URL,
            body_eq: ptrs([
                ("/variables/input/projectId", json!(PROJECT)),
                ("/variables/input/body", json!("Beta is on track.")),
                ("/variables/input/health", json!("onTrack")),
            ]),
            response_status: 200,
            response: json!({"data": {"projectUpdateCreate": {"success": true, "projectUpdate": {
                "id": UPDATE, "body": "Beta is on track.", "health": "onTrack",
                "url": "https://linear.app/acme/project/platform-hardening/update/1", "isDiffHidden": false,
                "createdAt": "2026-09-22T10:00:00.000Z", "updatedAt": "2026-09-22T10:00:00.000Z",
                "editedAt": null, "archivedAt": null,
                "user": {"id": USER, "name": "Ada Lovelace", "email": "ada@acme.com"},
                "project": {"id": PROJECT, "name": "Platform hardening"}}}}}),
            result_eq: ptrs([("/body", json!("Beta is on track.")), ("/health", json!("onTrack"))]),
        },
        Case {
            tool: "LINEAR_CREATE_PROJECT_MILESTONE",
            args: json!({"project_id": PROJECT, "name": "Beta", "target_date": "2026-11-30"}),
            method: "POST",
            url_contains: URL,
            body_eq: ptrs([
                ("/variables/input/projectId", json!(PROJECT)),
                ("/variables/input/name", json!("Beta")),
                ("/variables/input/targetDate", json!("2026-11-30")),
            ]),
            response_status: 200,
            response: json!({"data": {"projectMilestoneCreate": {"success": true, "projectMilestone": {
                "id": "e1f2a3b4-c5d6-4e7f-8a9b-0c8d7e6f5a4b", "name": "Beta", "description": null,
                "targetDate": "2026-11-30", "sortOrder": 1, "status": "next", "progress": 0,
                "createdAt": "2026-09-22T09:00:00.000Z",
                "project": {"id": PROJECT, "name": "Platform hardening"}}}}}),
            result_eq: ptrs([("/name", json!("Beta")), ("/target_date", json!("2026-11-30"))]),
        },
    ];
    support::assert_full_coverage(&a, &cases);
    for c in &cases {
        run(&a, c);
    }
}

#[test]
fn graphql_errors_surface_as_tool_errors() {
    use ctox_toolhub::http::HttpResponse;
    use ctox_toolhub::toolkits::ToolkitAdapter;

    let a = adapter();
    let resp = HttpResponse {
        status: 200,
        body: serde_json::to_vec(
            &json!({"errors": [{"message": "Entity not found: Issue"}]}),
        )
        .unwrap(),
        session_id: None,
    };
    let err = a.parse_response("LINEAR_GET_ISSUE", &resp).unwrap_err();
    assert!(err.to_string().contains("Entity not found: Issue"), "{err}");
}

/// Live read-only calls through the hub exec path (bare API key header).
/// Full live coverage lives in tests/live_linear.rs.
#[test]
#[ignore]
fn live_read_only_actions() {
    let a = adapter();
    for (tool, args) in [
        ("LINEAR_GET_VIEWER", json!({})),
        ("LINEAR_LIST_TEAMS", json!({"limit": 1})),
        ("LINEAR_LIST_ISSUES", json!({"limit": 1})),
    ] {
        let Some(result) = live(&a, tool, args) else { return };
        let v = result.unwrap_or_else(|e| panic!("{tool}: {e}"));
        assert!(v.is_object(), "{tool}: expected an object, got {v}");
    }
}
