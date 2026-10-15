//! GitLab port: one recorded case per action, expectations derived from the
//! Activepieces `run()` code in `src/lib/actions/**` (every call is a single
//! `sendGitlabRequest` against `{GITLAB_API_BASE}{path}` with `page`/`per_page`
//! added by `gitlabListPage`).
mod support;

use ctox_toolhub::toolkits::gitlab::GitLab;
use serde_json::{json, Value};
use support::{assert_full_coverage, live, run, Case};

const BASE: &str = "https://gitlab.test/api/v4";

fn adapter() -> GitLab {
    GitLab::with_api_base(BASE)
}

/// `&[(ptr, value)]` for a `Case` field: leaked so it lives as long as the case.
fn ptrs<const N: usize>(pairs: [(&'static str, Value); N]) -> &'static [(&'static str, Value)] {
    Box::leak(Box::new(pairs))
}

fn issue() -> Value {
    json!({
        "id": 84301, "iid": 12, "project_id": 4711,
        "title": "Login page times out on Safari",
        "description": "Safari shows a spinner forever.",
        "state": "opened", "web_url": "https://gitlab.com/acme/toolhub/-/issues/12",
        "created_at": "2026-09-01T10:00:00.000Z", "updated_at": "2026-09-20T08:30:00.000Z",
        "author": {"id": 22, "username": "ada"},
        "assignees": [{"id": 22, "username": "ada"}],
        "labels": ["bug", "frontend"],
        "milestone": {"id": 9, "iid": 3, "title": "Q3 hardening"},
        "confidential": false, "discussion_locked": false,
        "weight": null, "time_stats": {"time_estimate": 7200, "total_time_spent": 3600}
    })
}

fn merge_request() -> Value {
    json!({
        "id": 931, "iid": 7, "project_id": 4711,
        "title": "Add retry to the export job", "description": "Closes #12",
        "state": "opened", "draft": false, "work_in_progress": false,
        "source_branch": "feat/export-retry", "target_branch": "main",
        "web_url": "https://gitlab.com/acme/toolhub/-/merge_requests/7",
        "merge_status": "can_be_merged", "has_conflicts": false,
        "author": {"id": 22, "username": "ada"},
        "assignees": [{"id": 22, "username": "ada"}],
        "reviewers": [{"id": 31, "username": "grace"}],
        "labels": ["backend"], "created_at": "2026-09-21T09:00:00.000Z"
    })
}

fn note() -> Value {
    json!({
        "id": 5501, "type": null, "body": "Reproduced on Safari 18.",
        "author": {"id": 22, "username": "ada"},
        "created_at": "2026-09-21T10:15:00.000Z", "updated_at": "2026-09-21T10:15:00.000Z",
        "system": false, "internal": false, "confidential": false,
        "noteable_id": 84301, "noteable_iid": 12, "noteable_type": "Issue"
    })
}

fn label() -> Value {
    json!({
        "id": 61, "name": "bug", "color": "#FF0000", "text_color": "#FFFFFF",
        "description": "Something is broken", "priority": 2,
        "open_issues_count": 4, "closed_issues_count": 1
    })
}

#[test]
fn gitlab_actions_are_ported() {
    let a = adapter();
    let cases: Vec<Case> = vec![
        // --- users -------------------------------------------------------------
        Case {
            tool: "GITLAB_GET_CURRENT_USER",
            args: json!({}),
            method: "GET",
            url_contains: &[BASE, "/user"],
            body_eq: ptrs([]),
            response_status: 200,
            response: json!({
                "id": 22, "username": "ada", "name": "Ada Lovelace", "email": "ada@acme.com",
                "state": "active", "web_url": "https://gitlab.com/ada", "is_admin": true
            }),
            result_eq: ptrs([("/username", json!("ada")), ("/email", json!("ada@acme.com"))]),
        },
        Case {
            tool: "GITLAB_LIST_USERS",
            args: json!({"username": "@ada", "page": 1, "per_page": 10}),
            method: "GET",
            url_contains: &[BASE, "/users?", "username=ada", "page=1", "per_page=10"],
            body_eq: ptrs([]),
            response_status: 200,
            response: json!([{"id": 22, "username": "ada", "name": "Ada Lovelace"}]),
            result_eq: ptrs([("/count", json!(1)), ("/items/0/username", json!("ada"))]),
        },
        Case {
            tool: "GITLAB_GET_USER_STATUS",
            args: json!({"user": "@grace"}),
            method: "GET",
            url_contains: &[BASE, "/users/grace/status"],
            body_eq: ptrs([]),
            response_status: 200,
            response: json!({"emoji": "coffee", "message": "Back at 14:00", "availability": "busy"}),
            result_eq: ptrs([("/availability", json!("busy")), ("/emoji", json!("coffee"))]),
        },
        // --- issues ------------------------------------------------------------
        Case {
            tool: "GITLAB_LIST_ISSUES",
            args: json!({"project_id": "acme/toolhub", "state": "opened",
                         "labels": ["bug", "frontend"], "search": "login timeout",
                         "order_by": "updated_at", "sort": "desc", "page": 2, "per_page": 25}),
            method: "GET",
            url_contains: &[
                BASE, "/projects/acme%2Ftoolhub/issues?", "state=opened",
                "labels=bug%2Cfrontend", "search=login+timeout", "order_by=updated_at",
                "sort=desc", "page=2", "per_page=25",
            ],
            body_eq: ptrs([]),
            response_status: 200,
            response: json!([issue()]),
            result_eq: ptrs([("/items/0/iid", json!(12)), ("/items/0/labels/0", json!("bug"))]),
        },
        Case {
            tool: "GITLAB_GET_ISSUE",
            args: json!({"project_id": "4711", "issue_iid": 12}),
            method: "GET",
            url_contains: &[BASE, "/projects/4711/issues/12"],
            body_eq: ptrs([]),
            response_status: 200,
            response: issue(),
            result_eq: ptrs([("/iid", json!(12)), ("/title", json!("Login page times out on Safari"))]),
        },
        Case {
            tool: "GITLAB_CREATE_ISSUE",
            args: json!({"project_id": "4711", "title": "  Export job stalls  ",
                         "description": "Steps to reproduce", "labels": ["bug"],
                         "assignee_ids": ["22", "31"], "milestone_id": 9,
                         "due_date": "2026-10-11", "confidential": "false"}),
            method: "POST",
            url_contains: &[BASE, "/projects/4711/issues"],
            body_eq: ptrs([
                ("/title", json!("Export job stalls")),
                ("/description", json!("Steps to reproduce")),
                ("/labels", json!("bug")),
                ("/assignee_ids", json!([22, 31])),
                ("/milestone_id", json!(9)),
                ("/due_date", json!("2026-10-11")),
                ("/confidential", json!(false)),
            ]),
            response_status: 201,
            response: issue(),
            result_eq: ptrs([("/iid", json!(12))]),
        },
        Case {
            tool: "GITLAB_UPDATE_ISSUE",
            args: json!({"project_id": "4711", "issue_iid": 12, "state_event": "close",
                         "add_labels": ["p1"], "clear_assignees": true, "clear_milestone": true}),
            method: "PUT",
            url_contains: &[BASE, "/projects/4711/issues/12"],
            body_eq: ptrs([
                ("/state_event", json!("close")),
                ("/add_labels", json!("p1")),
                ("/assignee_ids", json!([0])),
                ("/milestone_id", json!(0)),
            ]),
            response_status: 200,
            response: issue(),
            result_eq: ptrs([("/state", json!("opened"))]),
        },
        Case {
            tool: "GITLAB_LIST_ISSUE_NOTES",
            args: json!({"project_id": "4711", "issue_iid": 12, "order_by": "created_at"}),
            method: "GET",
            url_contains: &[BASE, "/projects/4711/issues/12/notes?", "order_by=created_at",
                            "page=1", "per_page=20"],
            body_eq: ptrs([]),
            response_status: 200,
            response: json!([note()]),
            result_eq: ptrs([("/count", json!(1)), ("/items/0/id", json!(5501))]),
        },
        Case {
            tool: "GITLAB_GET_ISSUE_NOTE",
            args: json!({"project_id": "4711", "issue_iid": 12, "note_id": 5501}),
            method: "GET",
            url_contains: &[BASE, "/projects/4711/issues/12/notes/5501"],
            body_eq: ptrs([]),
            response_status: 200,
            response: note(),
            result_eq: ptrs([("/body", json!("Reproduced on Safari 18."))]),
        },
        Case {
            tool: "GITLAB_CREATE_ISSUE_NOTE",
            args: json!({"project_id": "4711", "issue_iid": 12, "body": "Thanks, taking a look.",
                         "internal": "true"}),
            method: "POST",
            url_contains: &[BASE, "/projects/4711/issues/12/notes"],
            body_eq: ptrs([("/body", json!("Thanks, taking a look.")), ("/internal", json!(true))]),
            response_status: 201,
            response: json!({"id": 5502, "body": "Thanks, taking a look.", "internal": true,
                             "system": false, "author": {"id": 22, "username": "ada"},
                             "noteable_iid": 12, "noteable_type": "Issue"}),
            result_eq: ptrs([("/id", json!(5502)), ("/internal", json!(true))]),
        },
        Case {
            tool: "GITLAB_DELETE_ISSUE_NOTE",
            args: json!({"project_id": "4711", "issue_iid": 12, "note_id": 5501}),
            method: "DELETE",
            url_contains: &[BASE, "/projects/4711/issues/12/notes/5501"],
            body_eq: ptrs([]),
            response_status: 204,
            response: json!(null),
            result_eq: ptrs([("/deleted", json!(true))]),
        },
        Case {
            tool: "GITLAB_GET_ISSUE_TIME_STATS",
            args: json!({"project_id": "4711", "issue_iid": 12}),
            method: "GET",
            url_contains: &[BASE, "/projects/4711/issues/12/time_stats"],
            body_eq: ptrs([]),
            response_status: 200,
            response: json!({"time_estimate": 7200, "total_time_spent": 3600, "human_time_estimate": "2h",
                             "human_total_time_spent": "1h"}),
            result_eq: ptrs([("/time_estimate", json!(7200)), ("/total_time_spent", json!(3600))]),
        },
        // --- merge requests ----------------------------------------------------
        Case {
            tool: "GITLAB_LIST_MERGE_REQUESTS",
            args: json!({"project_id": "4711", "state": "opened", "reviewer_id": 31,
                         "labels": ["backend"], "page": 1, "per_page": 20}),
            method: "GET",
            url_contains: &[BASE, "/projects/4711/merge_requests?", "state=opened",
                            "reviewer_id=31", "labels=backend"],
            body_eq: ptrs([]),
            response_status: 200,
            response: json!([merge_request()]),
            result_eq: ptrs([("/items/0/iid", json!(7)), ("/items/0/source_branch", json!("feat/export-retry"))]),
        },
        Case {
            tool: "GITLAB_GET_MERGE_REQUEST",
            args: json!({"project_id": "4711", "merge_request_iid": 7}),
            method: "GET",
            url_contains: &[BASE, "/projects/4711/merge_requests/7"],
            body_eq: ptrs([]),
            response_status: 200,
            response: merge_request(),
            result_eq: ptrs([("/iid", json!(7)), ("/target_branch", json!("main"))]),
        },
        Case {
            tool: "GITLAB_GET_MERGE_REQUEST_APPROVALS",
            args: json!({"project_id": "4711", "merge_request_iid": 7}),
            method: "GET",
            url_contains: &[BASE, "/projects/4711/merge_requests/7/approvals"],
            body_eq: ptrs([]),
            response_status: 200,
            response: json!({"id": 7, "iid": 7, "approved": false,
                             "approvals_required": 2, "approvals_left": 2,
                             "approved_by": []}),
            result_eq: ptrs([("/approved", json!(false)), ("/approvals_required", json!(2))]),
        },
        Case {
            tool: "GITLAB_LIST_MERGE_REQUEST_COMMITS",
            args: json!({"project_id": "4711", "merge_request_iid": 7}),
            method: "GET",
            url_contains: &[BASE, "/projects/4711/merge_requests/7/commits?"],
            body_eq: ptrs([]),
            response_status: 200,
            response: json!([{"id": "a1b2c3d4", "short_id": "a1b2c3d",
                              "title": "Retry the export three times", "author_name": "Ada Lovelace"}]),
            result_eq: ptrs([("/items/0/short_id", json!("a1b2c3d"))]),
        },
        Case {
            tool: "GITLAB_CREATE_MERGE_REQUEST",
            args: json!({"project_id": "4711", "source_branch": "feat/export-retry",
                         "target_branch": "main", "title": "Add retry to the export job",
                         "labels": ["backend"], "assignee_ids": ["22"], "reviewer_ids": ["31"],
                         "milestone_id": 9, "squash": "true"}),
            method: "POST",
            url_contains: &[BASE, "/projects/4711/merge_requests"],
            body_eq: ptrs([
                ("/source_branch", json!("feat/export-retry")),
                ("/target_branch", json!("main")),
                ("/title", json!("Add retry to the export job")),
                ("/labels", json!("backend")),
                ("/assignee_ids", json!([22])),
                ("/reviewer_ids", json!([31])),
                ("/milestone_id", json!(9)),
                ("/squash", json!(true)),
            ]),
            response_status: 201,
            response: merge_request(),
            result_eq: ptrs([("/iid", json!(7))]),
        },
        Case {
            tool: "GITLAB_UPDATE_MERGE_REQUEST",
            args: json!({"project_id": "4711", "merge_request_iid": 7, "state_event": "close",
                         "remove_labels": ["backend"], "clear_reviewers": true,
                         "clear_description": true}),
            method: "PUT",
            url_contains: &[BASE, "/projects/4711/merge_requests/7"],
            body_eq: ptrs([
                ("/state_event", json!("close")),
                ("/remove_labels", json!("backend")),
                ("/reviewer_ids", json!([0])),
                ("/description", json!("")),
            ]),
            response_status: 200,
            response: merge_request(),
            result_eq: ptrs([("/state", json!("opened"))]),
        },
        // --- projects ----------------------------------------------------------
        Case {
            tool: "GITLAB_LIST_PROJECTS",
            args: json!({"search": "toolhub", "order_by": "last_activity_at", "sort": "desc",
                         "page": 1, "per_page": 20}),
            method: "GET",
            url_contains: &[BASE, "/projects?", "membership=true", "search=toolhub",
                            "order_by=last_activity_at", "simple=true"],
            body_eq: ptrs([]),
            response_status: 200,
            response: json!([{"id": 4711, "name": "toolhub", "path": "toolhub",
                              "path_with_namespace": "acme/toolhub",
                              "name_with_namespace": "Acme / toolhub",
                              "default_branch": "main", "visibility": "private",
                              "star_count": 3, "last_activity_at": "2026-09-21T10:00:00.000Z"}]),
            result_eq: ptrs([("/count", json!(1)), ("/items/0/path_with_namespace", json!("acme/toolhub"))]),
        },
        Case {
            tool: "GITLAB_GET_PROJECT",
            args: json!({"project_id": "acme/toolhub"}),
            method: "GET",
            url_contains: &[BASE, "/projects/acme%2Ftoolhub"],
            body_eq: ptrs([]),
            response_status: 200,
            response: json!({"id": 4711, "name": "toolhub", "path_with_namespace": "acme/toolhub",
                             "default_branch": "main", "visibility": "private", "star_count": 3,
                             "web_url": "https://gitlab.com/acme/toolhub"}),
            result_eq: ptrs([("/id", json!(4711)), ("/default_branch", json!("main"))]),
        },
        Case {
            tool: "GITLAB_LIST_PROJECT_MEMBERS",
            args: json!({"project_id": "4711", "query": "ada"}),
            method: "GET",
            url_contains: &[BASE, "/projects/4711/members/all?", "query=ada"],
            body_eq: ptrs([]),
            response_status: 200,
            response: json!([{"id": 22, "username": "ada", "name": "Ada Lovelace",
                              "access_level": 30}]),
            result_eq: ptrs([("/items/0/access_level", json!(30))]),
        },
        // --- groups ------------------------------------------------------------
        Case {
            tool: "GITLAB_LIST_GROUPS",
            args: json!({"search": "acme", "top_level_only": "true", "order_by": "path"}),
            method: "GET",
            url_contains: &[BASE, "/groups?", "search=acme", "top_level_only=true", "order_by=path"],
            body_eq: ptrs([]),
            response_status: 200,
            response: json!([{"id": 77, "name": "Acme", "full_path": "acme", "visibility": "private"}]),
            result_eq: ptrs([("/items/0/full_path", json!("acme"))]),
        },
        Case {
            tool: "GITLAB_GET_GROUP",
            args: json!({"group_id": "acme/platform"}),
            method: "GET",
            url_contains: &[BASE, "/groups/acme%2Fplatform?", "with_projects=false"],
            body_eq: ptrs([]),
            response_status: 200,
            response: json!({"id": 78, "name": "Platform", "full_path": "acme/platform",
                             "visibility": "private", "project_creation_level": "developer"}),
            result_eq: ptrs([("/full_path", json!("acme/platform"))]),
        },
        // --- labels and milestones --------------------------------------------
        Case {
            tool: "GITLAB_LIST_LABELS",
            args: json!({"project_id": "4711", "search": "bug", "include_ancestor_groups": "false"}),
            method: "GET",
            url_contains: &[BASE, "/projects/4711/labels?", "search=bug", "include_ancestor_groups=false"],
            body_eq: ptrs([]),
            response_status: 200,
            response: json!([label()]),
            result_eq: ptrs([("/items/0/name", json!("bug"))]),
        },
        Case {
            tool: "GITLAB_GET_LABEL",
            args: json!({"project_id": "4711", "label_id": "bug :: urgent",
                         "include_ancestor_groups": "true"}),
            method: "GET",
            url_contains: &[BASE, "/projects/4711/labels/bug%20%3A%3A%20urgent?",
                            "include_ancestor_groups=true"],
            body_eq: ptrs([]),
            response_status: 200,
            response: label(),
            result_eq: ptrs([("/color", json!("#FF0000"))]),
        },
        Case {
            tool: "GITLAB_CREATE_LABEL",
            args: json!({"project_id": "4711", "name": "backend", "color": "#00FF00",
                         "description": "Server side", "priority": 3}),
            method: "POST",
            url_contains: &[BASE, "/projects/4711/labels"],
            body_eq: ptrs([
                ("/name", json!("backend")),
                ("/color", json!("#00FF00")),
                ("/description", json!("Server side")),
                ("/priority", json!(3)),
            ]),
            response_status: 201,
            response: json!({"id": 62, "name": "backend", "color": "#00FF00", "priority": 3}),
            result_eq: ptrs([("/id", json!(62))]),
        },
        Case {
            tool: "GITLAB_LIST_MILESTONES",
            args: json!({"project_id": "4711", "state": "active", "title": "Q3 hardening"}),
            method: "GET",
            url_contains: &[BASE, "/projects/4711/milestones?", "state=active", "title=Q3+hardening"],
            body_eq: ptrs([]),
            response_status: 200,
            response: json!([{"id": 9, "iid": 3, "title": "Q3 hardening", "state": "active",
                              "due_date": "2026-09-30", "issue_count": 4}]),
            result_eq: ptrs([("/items/0/id", json!(9)), ("/items/0/title", json!("Q3 hardening"))]),
        },
        // --- repository --------------------------------------------------------
        Case {
            tool: "GITLAB_LIST_BRANCHES",
            args: json!({"project_id": "4711", "search": "feat"}),
            method: "GET",
            url_contains: &[BASE, "/projects/4711/repository/branches?", "search=feat"],
            body_eq: ptrs([]),
            response_status: 200,
            response: json!([{"name": "feat/export-retry", "merged": false, "protected": false,
                              "default": false, "commit": {"id": "a1b2c3d4", "title": "Retry the export"}}]),
            result_eq: ptrs([("/items/0/name", json!("feat/export-retry"))]),
        },
        Case {
            tool: "GITLAB_LIST_COMMITS",
            args: json!({"project_id": "4711", "ref_name": "main", "path": "src/export.ts",
                         "since": "2026-09-01", "until": "2026-09-30T23:59:59Z"}),
            method: "GET",
            url_contains: &[BASE, "/projects/4711/repository/commits?", "ref_name=main",
                            "path=src%2Fexport.ts", "since=2026-09-01",
                            "until=2026-09-30T23%3A59%3A59Z"],
            body_eq: ptrs([]),
            response_status: 200,
            response: json!([{"id": "a1b2c3d4", "short_id": "a1b2c3d",
                              "title": "Retry the export three times",
                              "author_name": "Ada Lovelace", "created_at": "2026-09-21T09:00:00.000Z"}]),
            result_eq: ptrs([("/items/0/author_name", json!("Ada Lovelace"))]),
        },
        // --- ci ----------------------------------------------------------------
        Case {
            tool: "GITLAB_LIST_PIPELINES",
            args: json!({"project_id": "4711", "status": "failed", "ref": "main",
                         "username": "@ada", "order_by": "updated_at"}),
            method: "GET",
            url_contains: &[BASE, "/projects/4711/pipelines?", "status=failed", "ref=main",
                            "username=ada", "order_by=updated_at"],
            body_eq: ptrs([]),
            response_status: 200,
            response: json!([{"id": 880, "iid": 42, "ref": "main", "sha": "a1b2c3d4",
                              "status": "failed", "source": "push"}]),
            result_eq: ptrs([("/items/0/id", json!(880)), ("/items/0/status", json!("failed"))]),
        },
    ];
    assert_full_coverage(&a, &cases);
    for c in &cases {
        run(&a, c);
    }
}

/// Live read-only calls against the real GitLab API.
/// `CTOX_TOOLHUB_LIVE_GITLAB_TOKEN=… cargo test --test port_gitlab -- --ignored`.
#[test]
#[ignore]
fn live_read_only_calls() {
    let adapter = GitLab::default();
    let probes: [(&str, Value); 3] = [
        ("GITLAB_GET_CURRENT_USER", json!({})),
        ("GITLAB_LIST_USERS", json!({"search": "ada", "per_page": 3})),
        ("GITLAB_LIST_PROJECTS", json!({"per_page": 3, "order_by": "last_activity_at"})),
    ];
    let mut ran = 0;
    for (tool, args) in &probes {
        let Some(outcome) = live(&adapter, tool, args.clone()) else {
            return; // no token configured -> skip silently
        };
        ran += 1;
        outcome.unwrap_or_else(|e| panic!("{tool} failed live: {e}"));
    }
    assert_eq!(ran, probes.len());
}