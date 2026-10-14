//! GitHub port: one recorded case per ported action. Every expected URL is the
//! `resourceUri` of the Activepieces `run()` / dropdown loader it was ported
//! from (`packages/pieces/community/github/src`), e.g.
//! `GET /repos/{owner}/{repo}/issues/{number}`.
//! DE: Expectations come from the TypeScript source, not from our own code.
mod support;

use ctox_toolhub::toolkits::github::GitHub;
use serde_json::{json, Value};
use support::{live, run, Case};

const BASE: &str = "https://github.test";

/// `DISCUSSION_LOOKUP_QUERY` / `ADD_DISCUSSION_COMMENT_MUTATION` verbatim from
/// `src/lib/actions/create-discussion-comment.ts`.
const DISCUSSION_LOOKUP: &str = "query($owner: String!, $name: String!, $number: Int!) {\n  repository(owner: $owner, name: $name) {\n    discussion(number: $number) {\n      id\n    }\n  }\n}";
const ADD_DISCUSSION_COMMENT: &str = "mutation($discussionId: ID!, $body: String!) {\n  addDiscussionComment(input: { discussionId: $discussionId, body: $body }) {\n    comment {\n      id\n      body\n      url\n    }\n  }\n}";

fn adapter() -> GitHub {
    GitHub::with_api_base(BASE)
}

/// `&[(ptr, value)]` for a `Case` field: leaked so it lives as long as the case.
fn ptrs<const N: usize>(pairs: [(&'static str, Value); N]) -> &'static [(&'static str, Value)] {
    Box::leak(Box::new(pairs))
}

/// `GET /repos/{owner}/{repo}/issues/{n}` as GitHub returns it.
fn issue(number: i64, title: &str) -> Value {
    json!({
        "id": 1_234_567_890_u64 + number as u64,
        "node_id": "I_kwDOA1b2c4AbCdEf",
        "number": number, "title": title,
        "body": "Steps to reproduce:\n\n1. open the page",
        "state": "open", "state_reason": null, "locked": false, "comments": 2,
        "html_url": format!("https://github.com/octocat/hello-world/issues/{number}"),
        "user": {"login": "octocat", "html_url": "https://github.com/octocat"},
        "assignees": [{"login": "ada", "id": 9, "html_url": "https://github.com/ada",
                       "avatar_url": "https://avatars.githubusercontent.com/u/9", "type": "User"}],
        "labels": [{"id": 5, "name": "bug", "color": "d73a4a"}],
        "milestone": {"number": 2, "title": "v1.0"},
        "pull_request": null,
        "created_at": "2026-01-04T09:12:33Z", "updated_at": "2026-02-11T17:02:01Z",
        "closed_at": null, "closed_by": null,
    })
}

fn comment(id: i64) -> Value {
    json!({
        "id": id, "node_id": "IC_kwDOA1b2c4AbCdEf",
        "body": "Thanks for the report!", "path": "src/index.ts", "position": 12,
        "html_url": "https://github.com/octocat/hello-world/pull/7#discussion_r1",
        "user": {"login": "ada"}, "commit_id": "6dcb09b5b57875f334f61aebed695e2e4193db5e",
        "pull_request_review_id": null, "in_reply_to_id": null,
        "created_at": "2026-02-11T17:02:01Z", "updated_at": "2026-02-11T17:02:01Z",
    })
}

#[test]
fn github_actions_are_ported() {
    let a = adapter();
    let cases: Vec<Case> = vec![
        // ------------------------------------------------------------ reads
        Case {
            tool: "GITHUB_GET_REPO",
            args: json!({"owner": "octocat", "repo": "hello-world"}),
            method: "GET",
            url_contains: &[BASE, "/repos/octocat/hello-world"],
            body_eq: ptrs([]),
            response_status: 200,
            response: json!({
                "id": 1296269, "node_id": "MDEwOlJlcG9zaXRvcnkxMjk2MjY5",
                "name": "hello-world", "full_name": "octocat/hello-world",
                "description": "My first repository on GitHub!",
                "private": false, "fork": false, "archived": false, "language": "TypeScript",
                "default_branch": "main", "html_url": "https://github.com/octocat/hello-world",
                "topics": ["octocat", "sample"], "license": {"spdx_id": "MIT"},
                "stargazers_count": 1421, "forks_count": 35, "open_issues_count": 4,
                "owner": {"login": "octocat"},
                "created_at": "2011-01-26T19:01:12Z", "updated_at": "2026-02-11T16:59:31Z",
                "pushed_at": "2026-02-11T16:58:03Z"}),
            result_eq: ptrs([
                ("/full_name", json!("octocat/hello-world")),
                ("/default_branch", json!("main")),
                ("/private", json!(false)),
                ("/owner_login", json!("octocat")),
                ("/license", json!("MIT")),
                ("/stargazers_count", json!(1421)),
            ]),
        },
        Case {
            tool: "GITHUB_GET_AUTHENTICATED_USER",
            args: json!({}),
            method: "GET",
            url_contains: &[BASE, "/user"],
            body_eq: ptrs([]),
            response_status: 200,
            response: json!({
                "login": "ada", "id": 9, "node_id": "MDQ6VXNlcjE=",
                "name": "Ada Lovelace", "email": "ada@example.com",
                "bio": "Analytical engines.", "company": "Acme", "blog": "https://ada.example.com",
                "location": "London", "type": "User", "site_admin": false,
                "html_url": "https://github.com/ada", "avatar_url": "https://avatars.githubusercontent.com/u/9",
                "public_repos": 42, "public_gists": 7, "followers": 320, "following": 9,
                "created_at": "2011-01-25T18:44:36Z", "updated_at": "2026-02-11T09:00:00Z"}),
            result_eq: ptrs([
                ("/login", json!("ada")),
                ("/id", json!(9)),
                ("/email", json!("ada@example.com")),
                ("/type", json!("User")),
                ("/public_repos", json!(42)),
            ]),
        },
        Case {
            tool: "GITHUB_LIST_MY_REPOSITORIES",
            args: json!({"page": 2, "per_page": 50}),
            method: "GET",
            url_contains: &[BASE, "/user/repos", "page=2", "per_page=50"],
            body_eq: ptrs([]),
            response_status: 200,
            response: json!([
                {"id": 1296269, "name": "hello-world", "full_name": "octocat/hello-world",
                 "description": "My first repository on GitHub!", "private": true, "fork": false,
                 "archived": false, "language": "TypeScript", "default_branch": "main",
                 "html_url": "https://github.com/octocat/hello-world", "topics": [],
                 "license": {"spdx_id": "MIT"}, "stargazers_count": 3, "forks_count": 1,
                 "open_issues_count": 0, "owner": {"login": "octocat"},
                 "created_at": "2011-01-26T19:01:12Z", "updated_at": "2026-02-11T16:59:31Z",
                 "pushed_at": "2026-02-11T16:58:03Z"},
                {"id": 1300192, "name": " Spoon-Knife", "full_name": "octocat/Spoon-Knife",
                 "description": null, "private": false, "fork": false, "archived": false,
                 "language": null, "default_branch": "main",
                 "html_url": "https://github.com/octocat/Spoon-Knife", "topics": [],
                 "license": {"spdx_id": "MIT"}, "stargazers_count": 1, "forks_count": 0,
                 "open_issues_count": 0, "owner": {"login": "octocat"},
                 "created_at": "2011-01-26T19:04:39Z", "updated_at": "2026-01-04T11:11:22Z",
                 "pushed_at": "2026-01-04T11:10:00Z"}]),
            result_eq: ptrs([
                ("/0/full_name", json!("octocat/hello-world")),
                ("/0/private", json!(true)),
                ("/0/default_branch", json!("main")),
                ("/1/name", json!(" Spoon-Knife")),
                ("/1/language", Value::Null),
            ]),
        },
        Case {
            tool: "GITHUB_FIND_USER",
            args: json!({"username": "octocat"}),
            method: "GET",
            url_contains: &[BASE, "/users/octocat"],
            body_eq: ptrs([]),
            response_status: 200,
            response: json!({
                "login": "octocat", "id": 1, "name": "The Octocat", "email": null,
                "bio": null, "company": "@github", "blog": "https://github.blog",
                "location": "San Francisco", "type": "User", "site_admin": false,
                "html_url": "https://github.com/octocat",
                "avatar_url": "https://avatars.githubusercontent.com/u/1",
                "public_repos": 8, "public_gists": 8, "followers": 9000, "following": 9,
                "created_at": "2011-01-25T18:44:36Z", "updated_at": "2026-02-11T09:00:00Z"}),
            result_eq: ptrs([
                ("/found", json!(true)),
                ("/result/login", json!("octocat")),
                ("/result/company", json!("@github")),
                ("/result/followers", json!(9000)),
            ]),
        },
        Case {
            tool: "GITHUB_FIND_BRANCH",
            args: json!({"owner": "octocat", "repo": "hello-world", "branch": "feature/login"}),
            method: "GET",
            url_contains: &[BASE, "/repos/octocat/hello-world/branches/feature/login"],
            body_eq: ptrs([]),
            response_status: 200,
            response: json!({
                "name": "feature/login",
                "commit": {"sha": "6dcb09b5b57875f334f61aebed695e2e4193db5e",
                           "url": "https://api.github.com/repos/octocat/hello-world/commits/6dcb09b"},
                "protected": false,
                "protection": {"enabled": false,
                               "required_status_checks": {"contexts": []}}}),
            result_eq: ptrs([
                ("/found", json!(true)),
                ("/result/name", json!("feature/login")),
                ("/result/commit_sha", json!("6dcb09b5b57875f334f61aebed695e2e4193db5e")),
                ("/result/protected", json!(false)),
            ]),
        },
        Case {
            tool: "GITHUB_LIST_BRANCHES",
            args: json!({"owner": "octocat", "repo": "hello-world", "page": 1, "per_page": 100}),
            method: "GET",
            url_contains: &[BASE, "/repos/octocat/hello-world/branches", "page=1", "per_page=100"],
            body_eq: ptrs([]),
            response_status: 200,
            response: json!([
                {"name": "main", "commit": {"sha": "6dcb09b5b57875f334f61aebed695e2e4193db5e",
                                           "url": "https://api.github.com/repos/o/c/commits/6dcb09b"},
                 "protected": true, "protection": {"enabled": true,
                                                    "required_status_checks": {"contexts": ["ci"]}}},
                {"name": "feature/login", "commit": {"sha": "aa218f56b14c9653891f9e74264a383fa43fefbd",
                                                     "url": "https://api.github.com/repos/o/c/commits/aa218f"},
                 "protected": false, "protection": {"enabled": false,
                                                    "required_status_checks": {"contexts": []}}}]),
            result_eq: ptrs([
                ("/0/name", json!("main")),
                ("/0/commit_sha", json!("6dcb09b5b57875f334f61aebed695e2e4193db5e")),
                ("/0/protected", json!(true)),
                ("/1/name", json!("feature/login")),
            ]),
        },
        Case {
            tool: "GITHUB_FIND_ISSUE",
            args: json!({"owner": "octocat", "repo": "hello-world", "title": "Login error", "state": "open"}),
            method: "GET",
            url_contains: &[
                BASE,
                "/search/issues",
                "q=repo%3Aoctocat%2Fhello-world+is%3Aissue+in%3Atitle+%22Login+error%22+state%3Aopen",
                "per_page=1",
            ],
            body_eq: ptrs([]),
            response_status: 200,
            response: json!({
                "total_count": 1,
                "incomplete_results": false,
                "items": [{
                    "id": 1234567891_u64, "number": 42, "title": "Login error on Safari",
                    "body": "It fails on Safari only.", "state": "open", "state_reason": null,
                    "locked": false, "comments": 1,
                    "html_url": "https://github.com/octocat/hello-world/issues/42",
                    "user": {"login": "ada", "html_url": "https://github.com/ada"},
                    "assignees": [], "labels": [{"id": 5, "name": "bug"}],
                    "milestone": null, "pull_request": null,
                    "created_at": "2026-02-01T10:00:00Z", "updated_at": "2026-02-02T10:00:00Z",
                    "closed_at": null, "closed_by": null}]}),
            result_eq: ptrs([
                ("/found", json!(true)),
                ("/total_count", json!(1)),
                ("/result/0/number", json!(42)),
                ("/result/0/title", json!("Login error on Safari")),
                ("/result/0/author", json!("ada")),
                ("/result/0/is_pull_request", json!(false)),
            ]),
        },
        Case {
            tool: "GITHUB_GET_ISSUE",
            args: json!({"owner": "octocat", "repo": "hello-world", "issue_number": 42}),
            method: "GET",
            url_contains: &[BASE, "/repos/octocat/hello-world/issues/42"],
            body_eq: ptrs([]),
            response_status: 200,
            response: issue(42, "Login error on Safari"),
            result_eq: ptrs([
                ("/number", json!(42)),
                ("/title", json!("Login error on Safari")),
                ("/state", json!("open")),
                ("/label_names", json!("bug")),
                ("/assignee_logins", json!("ada")),
                ("/milestone_number", json!(2)),
                ("/is_pull_request", json!(false)),
            ]),
        },
        Case {
            tool: "GITHUB_LIST_ISSUE_ASSIGNEES",
            args: json!({"owner": "octocat", "repo": "hello-world"}),
            method: "GET",
            url_contains: &[BASE, "/repos/octocat/hello-world/assignees", "page=1", "per_page=100"],
            body_eq: ptrs([]),
            response_status: 200,
            response: json!([
                {"id": 1, "login": "octocat", "type": "User",
                 "html_url": "https://github.com/octocat",
                 "avatar_url": "https://avatars.githubusercontent.com/u/1"},
                {"id": 9, "login": "ada", "type": "User",
                 "html_url": "https://github.com/ada",
                 "avatar_url": "https://avatars.githubusercontent.com/u/9"}]),
            result_eq: ptrs([
                ("/0/login", json!("octocat")),
                ("/1/login", json!("ada")),
                ("/1/type", json!("User")),
            ]),
        },
        Case {
            tool: "GITHUB_LIST_REPOSITORY_LABELS",
            args: json!({"owner": "octocat", "repo": "hello-world"}),
            method: "GET",
            url_contains: &[BASE, "/repos/octocat/hello-world/labels", "per_page=100"],
            body_eq: ptrs([]),
            response_status: 200,
            response: json!([
                {"id": 5, "name": "bug", "color": "d73a4a", "description": "Something isn't working",
                 "default": true},
                {"id": 6, "name": "enhancement", "color": "a2eeef", "description": null,
                 "default": false}]),
            result_eq: ptrs([
                ("/0/name", json!("bug")),
                ("/0/color", json!("d73a4a")),
                ("/1/default", json!(false)),
            ]),
        },
        Case {
            tool: "GITHUB_LIST_REPOSITORY_MILESTONES",
            args: json!({"owner": "octocat", "repo": "hello-world", "page": 1}),
            method: "GET",
            url_contains: &[BASE, "/repos/octocat/hello-world/milestones", "page=1", "per_page=100"],
            body_eq: ptrs([]),
            response_status: 200,
            response: json!([
                {"number": 2, "title": "v1.0", "description": "First stable release", "state": "open",
                 "open_issues": 3, "closed_issues": 7, "due_on": "2026-06-01T00:00:00Z",
                 "html_url": "https://github.com/octocat/hello-world/milestone/2",
                 "created_at": "2026-01-01T10:00:00Z", "updated_at": "2026-02-11T10:00:00Z"},
                {"number": 1, "title": "v0.9", "description": null, "state": "closed",
                 "open_issues": 0, "closed_issues": 12, "due_on": null,
                 "html_url": "https://github.com/octocat/hello-world/milestone/1",
                 "created_at": "2025-11-01T10:00:00Z", "updated_at": "2026-01-05T10:00:00Z"}]),
            result_eq: ptrs([
                ("/0/number", json!(2)),
                ("/0/state", json!("open")),
                ("/0/open_issues", json!(3)),
                ("/1/state", json!("closed")),
                ("/1/due_on", Value::Null),
            ]),
        },
        Case {
            tool: "GITHUB_LIST_WORKFLOWS",
            args: json!({"owner": "octocat", "repo": "hello-world"}),
            method: "GET",
            url_contains: &[BASE, "/repos/octocat/hello-world/actions/workflows", "per_page=100"],
            body_eq: ptrs([]),
            response_status: 200,
            response: json!({
                "total_count": 1,
                "workflows": [{"id": 16135, "node_id": "MDg6V29ya2Zsb3cxNjEzNQ==",
                               "name": "CI", "path": ".github/workflows/blank.yml", "state": "active"}]}),
            result_eq: ptrs([
                ("/0/id", json!(16135)),
                ("/0/name", json!("CI")),
                ("/0/path", json!(".github/workflows/blank.yml")),
                ("/0/state", json!("active")),
            ]),
        },
        Case {
            tool: "GITHUB_LIST_ENVIRONMENTS",
            args: json!({"owner": "octocat", "repo": "hello-world"}),
            method: "GET",
            url_contains: &[BASE, "/repos/octocat/hello-world/environments", "page=1", "per_page=100"],
            body_eq: ptrs([]),
            response_status: 200,
            response: json!({
                "total_count": 1,
                "environments": [{"id": 1, "node_id": "MDExOkVudmlyb25tZW50MQ==", "name": "staging",
                                  "html_url": "https://github.com/octocat/hello-world/deployments/activity_log?environments_filter=staging",
                                  "protection_rules": [], "created_at": "2026-01-20T10:00:00Z",
                                  "updated_at": "2026-02-01T10:00:00Z"}]}),
            result_eq: ptrs([("/0/id", json!(1)), ("/0/name", json!("staging"))]),
        },
        Case {
            tool: "GITHUB_GET_FILE_CONTENT",
            args: json!({"owner": "octocat", "repo": "hello-world", "path": "README.md", "ref": "main"}),
            method: "GET",
            url_contains: &[BASE, "/repos/octocat/hello-world/contents/README.md", "ref=main"],
            body_eq: ptrs([]),
            response_status: 200,
            response: json!({
                "name": "README", "path": "README.md", "sha": "3d21ec53a331a6f037a91c368710b99387d012c1",
                "size": 14, "url": "https://api.github.com/repos/octocat/hello-world/contents/README.md",
                "html_url": "https://github.com/octocat/hello-world/blob/main/README.md",
                "git_url": "https://api.github.com/repos/octocat/hello-world/git/blobs/3d21ec5",
                "download_url": "https://raw.githubusercontent.com/octocat/hello-world/main/README.md",
                "type": "file", "content": "IyBoZWxsby13b3JsZAo=", "encoding": "base64"}),
            result_eq: ptrs([
                ("/path", json!("README.md")),
                ("/encoding", json!("base64")),
                ("/content", json!("# hello-world\n")),
                ("/size", json!(14)),
            ]),
        },
        Case {
            tool: "GITHUB_GET_WORKFLOW_RUN",
            args: json!({"owner": "octocat", "repo": "hello-world", "run_id": 29679449}),
            method: "GET",
            url_contains: &[BASE, "/repos/octocat/hello-world/actions/runs/29679449"],
            body_eq: ptrs([]),
            response_status: 200,
            response: json!({
                "id": 29679449, "name": "CI", "node_id": "MDEyOldvcmtmbG93IFJ1bjI5Njc5NDQ5",
                "head_branch": "main", "head_sha": "aa218f56b14c9653891f9e74264a383fa43fefbd",
                "run_number": 7, "event": "push", "status": "completed", "conclusion": "success",
                "workflow_id": 16135, "check_suite_id": 414944374,
                "html_url": "https://github.com/octocat/hello-world/actions/runs/29679449",
                "url": "https://api.github.com/repos/octocat/hello-world/actions/runs/29679449",
                "created_at": "2026-02-11T16:59:31Z", "updated_at": "2026-02-11T17:02:01Z",
                "run_attempt": 1}),
            result_eq: ptrs([
                ("/id", json!(29679449)),
                ("/status", json!("completed")),
                ("/conclusion", json!("success")),
                ("/head_branch", json!("main")),
                ("/run_number", json!(7)),
            ]),
        },
        Case {
            tool: "GITHUB_FIND_DISCUSSION",
            args: json!({"owner": "octocat", "repo": "hello-world", "discussion_number": 3}),
            method: "POST",
            url_contains: &[BASE, "/graphql"],
            body_eq: ptrs([
                ("/query", json!(DISCUSSION_LOOKUP)),
                ("/variables/owner", json!("octocat")),
                ("/variables/name", json!("hello-world")),
                ("/variables/number", json!(3)),
            ]),
            response_status: 200,
            response: json!({"data": {"repository": {"discussion": {
                "id": "D_kwDOA1b2c4AbCdEfgAAAAAAA"}}}}),
            result_eq: ptrs([
                ("/found", json!(true)),
                ("/discussion_id", json!("D_kwDOA1b2c4AbCdEfgAAAAAAA")),
            ]),
        },
        Case {
            tool: "GITHUB_LIST_ISSUES",
            args: json!({"owner": "octocat", "repo": "hello-world", "state": "open"}),
            method: "GET",
            url_contains: &[BASE, "/repos/octocat/hello-world/issues", "state=open", "per_page=100"],
            body_eq: ptrs([]),
            response_status: 200,
            response: json!([
                {"number": 42, "title": "Login error on Safari", "state": "open",
                 "html_url": "https://github.com/octocat/hello-world/issues/42",
                 "user": {"login": "ada"}, "labels": [{"name": "bug"}]},
                {"number": 43, "title": "Add dark mode", "state": "open",
                 "html_url": "https://github.com/octocat/hello-world/pull/43",
                 "user": {"login": "octocat"}, "labels": [],
                 "pull_request": {"url": "https://api.github.com/repos/octocat/hello-world/pulls/43"}}]),
            result_eq: ptrs([
                ("/0/number", json!(42)),
                ("/0/user", json!("ada")),
                ("/0/labels/0", json!("bug")),
            ]),
        },
        // ------------------------------------------------------------ writes
        Case {
            tool: "GITHUB_CREATE_ISSUE",
            args: json!({"owner": "octocat", "repo": "hello-world", "title": "Login page shows an error",
                         "description": "Steps to reproduce …", "labels": ["bug"],
                         "assignees": ["ada"]}),
            method: "POST",
            url_contains: &[BASE, "/repos/octocat/hello-world/issues"],
            body_eq: ptrs([
                ("/title", json!("Login page shows an error")),
                ("/body", json!("Steps to reproduce …")),
                ("/labels", json!(["bug"])),
                ("/assignees", json!(["ada"])),
            ]),
            response_status: 201,
            response: issue(42, "Login page shows an error"),
            result_eq: ptrs([
                ("/number", json!(42)),
                ("/title", json!("Login page shows an error")),
                ("/html_url", json!("https://github.com/octocat/hello-world/issues/42")),
            ]),
        },
        Case {
            tool: "GITHUB_UPDATE_ISSUE",
            args: json!({"owner": "octocat", "repo": "hello-world", "issue_number": 42,
                         "state": "closed", "state_reason": "completed", "milestone": 2}),
            method: "PATCH",
            url_contains: &[BASE, "/repos/octocat/hello-world/issues/42"],
            body_eq: ptrs([
                ("/state", json!("closed")),
                ("/state_reason", json!("completed")),
                ("/milestone", json!(2)),
            ]),
            response_status: 200,
            response: json!({
                "id": 1234567891_u64, "number": 42, "title": "Login error on Safari",
                "body": "Steps to reproduce …", "state": "closed", "state_reason": "completed",
                "locked": false, "comments": 2,
                "html_url": "https://github.com/octocat/hello-world/issues/42",
                "user": {"login": "ada", "html_url": "https://github.com/ada"},
                "assignees": [], "labels": [],
                "milestone": {"number": 2, "title": "v1.0"}, "pull_request": null,
                "created_at": "2026-02-01T10:00:00Z", "updated_at": "2026-02-11T17:02:01Z",
                "closed_at": "2026-02-11T17:02:01Z", "closed_by": {"login": "ada"}}),
            result_eq: ptrs([
                ("/state", json!("closed")),
                ("/state_reason", json!("completed")),
                ("/closed_by", json!("ada")),
            ]),
        },
        Case {
            tool: "GITHUB_ADD_LABELS_TO_ISSUE",
            args: json!({"owner": "octocat", "repo": "hello-world", "issue_number": 42,
                         "labels": ["bug", "priority: high"]}),
            method: "POST",
            url_contains: &[BASE, "/repos/octocat/hello-world/issues/42/labels"],
            body_eq: ptrs([("/labels", json!(["bug", "priority: high"]))]),
            response_status: 200,
            response: json!([
                {"id": 5, "name": "bug", "color": "d73a4a", "description": null, "default": true},
                {"id": 6, "name": "priority: high", "color": "b60205", "description": null,
                 "default": false}]),
            result_eq: ptrs([
                ("/count", json!(2)),
                ("/labels/0/name", json!("bug")),
                ("/labels/1/name", json!("priority: high")),
            ]),
        },
        Case {
            tool: "GITHUB_CREATE_ISSUE_COMMENT",
            args: json!({"owner": "octocat", "repo": "hello-world", "issue_number": 42,
                         "comment": "Reproduced on Safari 18."}),
            method: "POST",
            url_contains: &[BASE, "/repos/octocat/hello-world/issues/42/comments"],
            body_eq: ptrs([("/body", json!("Reproduced on Safari 18."))]),
            response_status: 201,
            response: comment(9_000_001),
            result_eq: ptrs([
                ("/id", json!(9_000_001)),
                ("/body", json!("Thanks for the report!")),
                ("/author", json!("ada")),
            ]),
        },
        Case {
            tool: "GITHUB_CREATE_BRANCH",
            args: json!({"owner": "octocat", "repo": "hello-world",
                         "source_sha": "aa218f56b14c9653891f9e74264a383fa43fefbd",
                         "new_branch_name": "feature/login"}),
            method: "POST",
            url_contains: &[BASE, "/repos/octocat/hello-world/git/refs"],
            body_eq: ptrs([
                ("/ref", json!("refs/heads/feature/login")),
                ("/sha", json!("aa218f56b14c9653891f9e74264a383fa43fefbd")),
            ]),
            response_status: 201,
            response: json!({
                "ref": "refs/heads/feature/login",
                "node_id": "MDM6UmVmMTI4NjQ4OTIyNTozOTUzMTQ1MzY=",
                "url": "https://api.github.com/repos/octocat/hello-world/git/refs/heads/feature/login",
                "object": {"type": "commit", "sha": "aa218f56b14c9653891f9e74264a383fa43fefbd",
                           "url": "https://api.github.com/repos/octocat/hello-world/git/commits/aa218f"}}),
            result_eq: ptrs([
                ("/ref", json!("refs/heads/feature/login")),
                ("/sha", json!("aa218f56b14c9653891f9e74264a383fa43fefbd")),
                ("/object_type", json!("commit")),
            ]),
        },
        Case {
            tool: "GITHUB_DELETE_BRANCH",
            args: json!({"owner": "octocat", "repo": "hello-world", "branch": "feature/login"}),
            method: "DELETE",
            url_contains: &[BASE, "/repos/octocat/hello-world/git/refs/heads/feature/login"],
            body_eq: ptrs([]),
            response_status: 204,
            response: json!({}),
            result_eq: ptrs([("/deleted", json!(true))]),
        },
        Case {
            tool: "GITHUB_LOCK_ISSUE",
            args: json!({"owner": "octocat", "repo": "hello-world", "issue_number": 42,
                         "lock_reason": "too heated"}),
            method: "PUT",
            url_contains: &[BASE, "/repos/octocat/hello-world/issues/42/lock"],
            body_eq: ptrs([("/lock_reason", json!("too heated"))]),
            response_status: 204,
            response: json!({}),
            result_eq: ptrs([("/locked", json!(true))]),
        },
        Case {
            tool: "GITHUB_UNLOCK_ISSUE",
            args: json!({"owner": "octocat", "repo": "hello-world", "issue_number": 42}),
            method: "DELETE",
            url_contains: &[BASE, "/repos/octocat/hello-world/issues/42/lock"],
            body_eq: ptrs([]),
            response_status: 204,
            response: json!({}),
            result_eq: ptrs([("/locked", json!(false))]),
        },
        Case {
            tool: "GITHUB_CREATE_GIST",
            args: json!({"description": "Release snippet", "public": false,
                         "filename": "release.sh", "content": "#!/bin/sh\necho release\n"}),
            method: "POST",
            url_contains: &[BASE, "/gists"],
            body_eq: ptrs([
                ("/description", json!("Release snippet")),
                ("/public", json!(false)),
                ("/files/release.sh/content", json!("#!/bin/sh\necho release\n")),
            ]),
            response_status: 201,
            response: json!({
                "id": "aa5a315d61ae9438b18d", "node_id": "MDExOkdpc3RhYWE1YTVhMzE1ZDYxYWU5NDM4YjE4ZA==",
                "description": "Release snippet", "public": false,
                "html_url": "https://gist.github.com/ada/aa5a315d61ae9438b18d",
                "git_pull_url": "https://gist.github.com/aa5a315d61ae9438b18d.git",
                "files": {"release.sh": {"filename": "release.sh", "type": "text/plain",
                                         "language": "Shell", "raw_url": "https://gist.githubusercontent.com/ada/aa5a31/raw/release.sh",
                                         "size": 28}},
                "created_at": "2026-02-11T18:00:00Z", "updated_at": "2026-02-11T18:00:00Z"}),
            result_eq: ptrs([
                ("/id", json!("aa5a315d61ae9438b18d")),
                ("/public", json!(false)),
                ("/files/0/filename", json!("release.sh")),
                ("/files/0/language", json!("Shell")),
            ]),
        },
        Case {
            tool: "GITHUB_CREATE_COMMIT_COMMENT",
            args: json!({"owner": "octocat", "repo": "hello-world",
                         "sha": "6dcb09b5b57875f334f61aebed695e2e4193db5e",
                         "body": "Nice refactor.", "path": "src/index.ts", "position": 12}),
            method: "POST",
            url_contains: &[BASE, "/repos/octocat/hello-world/commits/6dcb09b5b57875f334f61aebed695e2e4193db5e/comments"],
            body_eq: ptrs([
                ("/body", json!("Nice refactor.")),
                ("/path", json!("src/index.ts")),
                ("/position", json!(12)),
            ]),
            response_status: 201,
            response: json!({
                "id": 1, "node_id": "MDIzOkNvbW1pdENvbW1lbnQx",
                "body": "Nice refactor.", "path": "src/index.ts", "position": 12,
                "html_url": "https://github.com/octocat/hello-world/commit/6dcb09b#commitcomment-1",
                "user": {"login": "ada"}, "commit_id": "6dcb09b5b57875f334f61aebed695e2e4193db5e",
                "pull_request_review_id": null, "in_reply_to_id": null,
                "created_at": "2026-02-11T17:00:00Z", "updated_at": "2026-02-11T17:00:00Z"}),
            result_eq: ptrs([
                ("/id", json!(1)),
                ("/path", json!("src/index.ts")),
                ("/position", json!(12)),
                ("/author", json!("ada")),
            ]),
        },
        Case {
            tool: "GITHUB_CREATE_PULL_REQUEST_REVIEW_COMMENT",
            args: json!({"owner": "octocat", "repo": "hello-world", "pull_number": 7,
                         "commit_id": "6dcb09b5b57875f334f61aebed695e2e4193db5e",
                         "path": "src/index.ts", "body": "Rename this.", "position": 12}),
            method: "POST",
            url_contains: &[BASE, "/repos/octocat/hello-world/pulls/7/comments"],
            body_eq: ptrs([
                ("/commit_id", json!("6dcb09b5b57875f334f61aebed695e2e4193db5e")),
                ("/path", json!("src/index.ts")),
                ("/body", json!("Rename this.")),
                ("/position", json!(12)),
            ]),
            response_status: 201,
            response: comment(1_234_567),
            result_eq: ptrs([
                ("/id", json!(1_234_567)),
                ("/body", json!("Thanks for the report!")),
                ("/commit_id", json!("6dcb09b5b57875f334f61aebed695e2e4193db5e")),
                ("/pull_request_review_id", Value::Null),
            ]),
        },
        Case {
            tool: "GITHUB_CREATE_DISCUSSION_COMMENT",
            args: json!({"owner": "octocat", "repo": "hello-world",
                         "discussion_id": "D_kwDOA1b2c4AbCdEfgAAAAAAA",
                         "body": "Thanks for the discussion!"}),
            method: "POST",
            url_contains: &[BASE, "/graphql"],
            body_eq: ptrs([
                ("/query", json!(ADD_DISCUSSION_COMMENT)),
                ("/variables/discussionId", json!("D_kwDOA1b2c4AbCdEfgAAAAAAA")),
                ("/variables/body", json!("Thanks for the discussion!")),
            ]),
            response_status: 200,
            response: json!({"data": {"addDiscussionComment": {"comment": {
                "id": "DC_kwDOA1b2c4AbCdEfgAAAAAAA",
                "body": "Thanks for the discussion!",
                "url": "https://github.com/octocat/hello-world/discussions/3#discussioncomment-1"}}}}),
            result_eq: ptrs([
                ("/id", json!("DC_kwDOA1b2c4AbCdEfgAAAAAAA")),
                ("/body", json!("Thanks for the discussion!")),
                ("/url", json!("https://github.com/octocat/hello-world/discussions/3#discussioncomment-1")),
            ]),
        },
        Case {
            tool: "GITHUB_TRIGGER_WORKFLOW_DISPATCH",
            args: json!({"owner": "octocat", "repo": "hello-world", "workflow_id": 16135,
                         "ref": "main", "inputs": {"environment": "staging"}}),
            method: "POST",
            url_contains: &[BASE, "/repos/octocat/hello-world/actions/workflows/16135/dispatches"],
            body_eq: ptrs([
                ("/ref", json!("main")),
                ("/inputs/environment", json!("staging")),
                ("/return_run_details", json!(true)),
            ]),
            response_status: 200,
            response: json!({"workflow_run_id": 29679450}),
            result_eq: ptrs([
                ("/dispatched", json!(true)),
                ("/workflow_run_id", json!(29679450)),
            ]),
        },
    ];
    support::assert_full_coverage(&a, &cases);
    for c in &cases {
        run(&a, c);
    }
}

/// Live read-only calls through the hub exec path.
/// `CTOX_TOOLHUB_LIVE_GITHUB_TOKEN=<token> cargo test --test port_github -- --ignored`
#[test]
#[ignore]
fn live_read_only_actions() {
    let a = adapter();
    for (tool, args) in [
        ("GITHUB_GET_AUTHENTICATED_USER", json!({})),
        ("GITHUB_LIST_MY_REPOSITORIES", json!({"per_page": 1})),
        ("GITHUB_FIND_USER", json!({"username": "octocat"})),
    ] {
        let Some(result) = live(&a, tool, args) else { return };
        let v = result.unwrap_or_else(|e| panic!("{tool}: {e}"));
        assert!(v.is_object(), "{tool}: expected an object, got {v}");
    }
}