//! GitHub toolkit (REST API `https://api.github.com` + the GraphQL endpoint the
//! piece uses for discussions). Action set modelled on Activepieces
//! `piece-github` (MIT), i.e. `packages/pieces/community/github/src`.
//!
//! Auth (replicates `src/lib/auth.ts` + `src/lib/common/index.ts`):
//! * OAuth2 with GitHub's own authorize/token endpoints and exactly the piece's
//!   scopes (`admin:repo_hook`, `admin:org`, `repo`, `gist`).
//! * Personal access token (classic or fine-grained): the piece sends it as
//!   `Authorization: Bearer <token>` (`githubApiCall`), which is `AuthScheme::Bearer`.
//! * The piece's third auth option is a GitHub App installation
//!   (`PieceAuth.CustomAuth`, JWT -> `POST /app/installations/{id}/access_tokens`).
//!   CTOX has no signing primitive for RS256 JWTs, so it is not offered here; use
//!   OAuth or a PAT instead. Gists additionally require a *user* token, which the
//!   piece documents on `create-gist.ts`.
//!
//! PKCE: the piece declares `pkce: false`, CTOX always sends S256
//! (`tests/flows.rs` asserts the challenge); GitHub accepts it.
//!
//! Base URL: GitHub.com is fixed, so `https://api.github.com` is the default.
//! GitHub Enterprise Server installs live under their own host — construct the
//! adapter with `GitHub::with_api_base("https://github.<company>.com/api/v3")`.
//!
//! Not ported: the ~70 agent atomics under `src/lib/actions/ai/` (audience
//! `ai`) with one exception — `GITHUB_GET_REPO`, whose slug `tests/flows.rs`
//! drives and whose only source in this piece is
//! `src/lib/actions/ai/get-repository.ts`. Also not ported:
//! `raw-graphql-query.ts` (custom API call), file uploads and the webhook
//! triggers.
//!
//! Actions that need two HTTP calls are split into their two requests, because
//! one tool call is one request here: `create-discussion-comment.ts` becomes
//! `GITHUB_FIND_DISCUSSION` + `GITHUB_CREATE_DISCUSSION_COMMENT`, and
//! `create-branch.ts` reads the source SHA via `GITHUB_FIND_BRANCH` before
//! `GITHUB_CREATE_BRANCH` posts `/git/refs`.
use anyhow::{anyhow, bail, Result};
use base64::Engine as _;
use serde_json::{json, Map, Value};

use super::common::*;
use super::{path_segment, ToolkitAdapter};
use crate::http::{HttpRequest, HttpResponse};
use crate::model::*;

pub const SLUG: &str = "github";

/// `repo` covers every private read/write the ported actions do; `gist` is only
/// needed by `GITHUB_CREATE_GIST`. `src/lib/auth.ts` requests both.
const REPO: &str = "repo";
const GIST: &str = "gist";

/// GitHub numbers/ids are positive integers; workflow run ids already exceed
/// 10^10, so the bound is the JSON-safe integer range.
const MAX_NUMBER: u64 = 9_007_199_254_740_991;

// ---------------------------------------------------------------- schema helpers

fn flag(desc: &str, default: bool) -> Value {
    json!({"type": "boolean", "description": desc, "default": default})
}

fn choice(desc: &str, options: &[&str], default: &str) -> Value {
    json!({"type": "string", "description": desc, "enum": options, "default": default})
}

/// Optional dropdown: no schema default, so an unset value stays unset.
fn choice_opt(desc: &str, options: &[&str]) -> Value {
    json!({"type": "string", "description": desc, "enum": options})
}

/// Issue/PR/discussion/run numbers and diff positions: no default, so an unset
/// value stays unset.
fn number(desc: &str) -> Value {
    json!({"type": "integer", "description": desc, "minimum": 1, "maximum": MAX_NUMBER})
}

fn object(desc: &str) -> Value {
    json!({"type": "object", "description": desc})
}

fn repo_props() -> Vec<(&'static str, Value)> {
    vec![
        (
            "owner",
            s("Repository-Owner (Login eines Users oder einer Organisation)"),
        ),
        ("repo", s("Repository-Name ohne Owner-Präfix")),
    ]
}

/// Repo props + the action's own props.
fn with_repo(mut extra: Vec<(&'static str, Value)>) -> Vec<(&'static str, Value)> {
    let mut v = repo_props();
    v.append(&mut extra);
    v
}

/// `githubPaginatedApiCall` always sends `page` and `per_page=100`.
fn page_props() -> Vec<(&'static str, Value)> {
    vec![
        ("page", int("Seitenzahl (1-basiert)", 1, 1000, 1)),
        ("per_page", int("Einträge pro Seite (1-100)", 1, 100, 100)),
    ]
}

fn with_repo_page(extra: Vec<(&'static str, Value)>) -> Vec<(&'static str, Value)> {
    let mut v = with_repo(extra);
    v.extend(page_props());
    v
}

// ---------------------------------------------------------------- argument helpers

/// `owner` / `repo` path segments.
fn repo_of(args: &Value) -> Result<(String, String)> {
    Ok((
        path_segment(args, "owner")?.to_string(),
        path_segment(args, "repo")?.to_string(),
    ))
}

/// Slash-separated path parameter (branch names like `feature/x`, file paths).
/// Every segment goes through `path_segment`, so an empty segment, `..` or any
/// other injection attempt is rejected before it reaches the URL.
fn path_multi(args: &Value, key: &str) -> Result<String> {
    let raw = req_str(args, key)?;
    let mut parts = Vec::new();
    for seg in raw.split('/') {
        parts.push(path_segment(&json!({ key: seg }), key)?.to_string());
    }
    Ok(parts.join("/"))
}

fn number_of(args: &Value, key: &str) -> Result<u64> {
    let n = args
        .get(key)
        .and_then(Value::as_u64)
        .ok_or_else(|| anyhow!("missing argument `{key}`"))?;
    if n == 0 || n > MAX_NUMBER {
        bail!("`{key}` must be a positive whole number");
    }
    Ok(n)
}

/// Optional integer body field (`position`, `milestone`); `None` leaves it out.
fn opt_number(args: &Value, key: &str) -> Option<u64> {
    args.get(key).and_then(Value::as_u64).filter(|v| *v > 0)
}

/// `?page=…&per_page=…` as `githubPaginatedApiCall` always sends them, plus the
/// action's own query parameters.
fn page_query(args: &Value, extra: &[(&str, Option<String>)]) -> String {
    let page = args.get("page").and_then(Value::as_u64).unwrap_or(1).clamp(1, 1000);
    let per_page = opt_u64(args, "per_page", 100, 1, 100);
    let mut pairs: Vec<(&str, Option<String>)> = vec![
        ("page", Some(page.to_string())),
        ("per_page", Some(per_page.to_string())),
    ];
    pairs.extend_from_slice(extra);
    query(&pairs)
}

fn put_str(m: &mut Map<String, Value>, args: &Value, key: &str, field: &str) {
    if let Some(v) = opt_str(args, key) {
        m.insert(field.into(), json!(v));
    }
}

fn put_list(m: &mut Map<String, Value>, args: &Value, key: &str, field: &str) {
    let v = str_vec(args, key);
    if !v.is_empty() {
        m.insert(field.into(), json!(v));
    }
}

fn obj(pairs: Vec<(&str, Value)>) -> Value {
    let mut m = Map::new();
    for (k, v) in pairs {
        m.insert(k.into(), v);
    }
    Value::Object(m)
}

fn extend(base: &mut Value, extra: Value) {
    if let (Some(b), Some(e)) = (base.as_object_mut(), extra.as_object()) {
        for (k, v) in e {
            b.insert(k.clone(), v.clone());
        }
    }
}

// ---------------------------------------------------------------- response mappers

fn flatten_user(u: &Value) -> Value {
    json!({
        "id": u["id"], "login": u["login"], "name": u["name"], "email": u["email"],
        "bio": u["bio"], "company": u["company"], "blog": u["blog"], "location": u["location"],
        "type": u["type"], "site_admin": u["site_admin"], "html_url": u["html_url"],
        "avatar_url": u["avatar_url"], "public_repos": u["public_repos"],
        "public_gists": u["public_gists"], "followers": u["followers"],
        "following": u["following"], "created_at": u["created_at"], "updated_at": u["updated_at"],
    })
}

fn flatten_user_summary(u: &Value) -> Value {
    json!({
        "id": u["id"], "login": u["login"], "type": u["type"],
        "html_url": u["html_url"], "avatar_url": u["avatar_url"],
    })
}

fn flatten_repository(r: &Value) -> Value {
    json!({
        "id": r["id"], "name": r["name"], "full_name": r["full_name"],
        "description": r["description"], "private": r["private"], "fork": r["fork"],
        "archived": r["archived"], "language": r["language"], "default_branch": r["default_branch"],
        "html_url": r["html_url"], "topics": r["topics"], "license": r["license"]["spdx_id"],
        "stargazers_count": r["stargazers_count"], "forks_count": r["forks_count"],
        "open_issues_count": r["open_issues_count"], "owner_login": r["owner"]["login"],
        "created_at": r["created_at"], "updated_at": r["updated_at"], "pushed_at": r["pushed_at"],
    })
}

fn flatten_branch(b: &Value) -> Value {
    json!({
        "name": b["name"],
        "commit_sha": b["commit"]["sha"],
        "commit_url": b["commit"]["url"],
        "protected": b["protected"],
        "protection_enabled": b["protection"]["enabled"],
        "required_status_checks": b["protection"]["required_status_checks"]["contexts"],
    })
}

fn flatten_issue(i: &Value) -> Value {
    let assignees: Vec<Value> = i["assignees"].as_array().cloned().unwrap_or_default();
    let labels: Vec<Value> = i["labels"].as_array().cloned().unwrap_or_default();
    let label_names: Vec<&str> = labels.iter().filter_map(|l| l["name"].as_str()).collect();
    let mut out = json!({
        "id": i["id"], "number": i["number"], "title": i["title"], "body": i["body"],
        "state": i["state"], "state_reason": i["state_reason"], "locked": i["locked"],
        "comments": i["comments"], "html_url": i["html_url"],
        "author": i["user"]["login"], "author_url": i["user"]["html_url"],
        "is_pull_request": !i["pull_request"].is_null(),
        "created_at": i["created_at"], "updated_at": i["updated_at"],
        "closed_at": i["closed_at"], "closed_by": i["closed_by"]["login"],
        "milestone_number": i["milestone"]["number"], "milestone_title": i["milestone"]["title"],
    });
    extend(&mut out, json!({
        "assignees": assignees.iter().map(flatten_user_summary).collect::<Vec<_>>(),
        "assignee_logins": assignees
            .iter().filter_map(|a| a["login"].as_str()).collect::<Vec<_>>().join(", "),
        "labels": labels.iter().filter_map(|l| l["name"].as_str()).collect::<Vec<_>>(),
        "label_names": label_names.join(", "),
    }));
    out
}

fn flatten_comment(c: &Value) -> Value {
    json!({
        "id": c["id"], "body": c["body"], "path": c["path"], "position": c["position"],
        "html_url": c["html_url"], "author": c["user"]["login"],
        "commit_id": c["commit_id"], "pull_request_review_id": c["pull_request_review_id"],
        "in_reply_to_id": c["in_reply_to_id"], "created_at": c["created_at"],
        "updated_at": c["updated_at"],
    })
}

fn flatten_label(l: &Value) -> Value {
    json!({
        "id": l["id"], "name": l["name"], "color": l["color"],
        "description": l["description"], "default": l["default"],
    })
}

fn flatten_workflow(w: &Value) -> Value {
    json!({"id": w["id"], "node_id": w["node_id"], "name": w["name"], "path": w["path"], "state": w["state"]})
}

fn flatten_milestone(m: &Value) -> Value {
    json!({
        "number": m["number"], "title": m["title"], "description": m["description"],
        "state": m["state"], "open_issues": m["open_issues"], "closed_issues": m["closed_issues"],
        "due_on": m["due_on"], "html_url": m["html_url"],
        "created_at": m["created_at"], "updated_at": m["updated_at"],
    })
}

fn flatten_gist(g: &Value) -> Value {
    let files: Vec<Value> = g["files"]
        .as_object()
        .map(|m| {
            m.iter()
                .map(|(name, f)| {
                    json!({"filename": name, "language": f["language"], "size": f["size"],
                        "raw_url": f["raw_url"], "type": f["type"]})
                })
                .collect()
        })
        .unwrap_or_default();
    json!({
        "id": g["id"], "description": g["description"], "public": g["public"],
        "html_url": g["html_url"], "git_pull_url": g["git_pull_url"],
        "files": files, "created_at": g["created_at"], "updated_at": g["updated_at"],
    })
}

fn flatten_workflow_run(r: &Value) -> Value {
    json!({
        "id": r["id"], "name": r["name"], "event": r["event"], "status": r["status"],
        "conclusion": r["conclusion"], "workflow_id": r["workflow_id"],
        "run_number": r["run_number"], "run_attempt": r["run_attempt"],
        "head_branch": r["head_branch"], "head_sha": r["head_sha"],
        "html_url": r["html_url"], "url": r["url"], "created_at": r["created_at"],
        "updated_at": r["updated_at"],
    })
}

// ---------------------------------------------------------------- adapter

pub struct GitHub {
    spec: ToolkitSpec,
    api_base: String,
}

impl Default for GitHub {
    fn default() -> Self {
        Self::with_api_base("https://api.github.com")
    }
}

#[allow(clippy::too_many_lines)]
impl GitHub {
    pub fn with_api_base(api_base: &str) -> Self {
        let t = |slug, title, desc, props: Props<'_>, req: &[&str], scopes: &[&str], m| {
            tool(SLUG, slug, title, desc, props, req, scopes, m)
        };
        let spec = ToolkitSpec {
            slug: SLUG.into(),
            name: "GitHub".into(),
            description: LocalizedText::new(
                "GitHub: Repositories, Branches, Issues, Diskussionen, Gists und Actions-Workflows lesen und bearbeiten.",
                "GitHub: read and change repositories, branches, issues, discussions, gists and Actions workflows.",
            ),
            auth_schemes: vec![
                AuthScheme::OAuth2(OAuth2Scheme {
                    authorize_url: "https://github.com/login/oauth/authorize".into(),
                    token_url: "https://github.com/login/oauth/access_token".into(),
                    // Exactly the scopes of `githubOAuth2Auth` in `src/lib/auth.ts`.
                    default_scopes: [
                        "admin:repo_hook",
                        "admin:org",
                        "repo",
                        "gist",
                    ]
                    .iter()
                    .map(|s| (*s).to_string())
                    .collect(),
                    // The piece sets `pkce: false`; CTOX always sends S256.
                    pkce: true,
                    scope_separator: " ".into(),
                    refreshable: true,
                    extra_authorize_params: Vec::new(),
                }),
                // Personal access token, sent as `Authorization: Bearer <token>`
                // by `githubApiCall` in `src/lib/common/index.ts`.
                AuthScheme::Bearer,
            ],
            tools: vec![
                // ---------------------------------------------------- reads
                t(
                    "GITHUB_GET_REPO",
                    ("Repository lesen", "Get repository"),
                    (
                        "Liest die Metadaten eines Repositorys, u. a. Default-Branch und Sichtbarkeit.",
                        "Reads a repository's metadata, including its default branch and visibility.",
                    ),
                    &repo_props(),
                    &["owner", "repo"],
                    &[REPO],
                    false,
                ),
                t(
                    "GITHUB_GET_AUTHENTICATED_USER",
                    ("Angemeldeten Nutzer lesen", "Get authenticated user"),
                    (
                        "Liest das Profil hinter dem Token (Token aus OAuth oder Personal Access Token).",
                        "Reads the profile behind the token (OAuth or personal access token).",
                    ),
                    &[],
                    &[],
                    &[],
                    false,
                ),
                t(
                    "GITHUB_LIST_MY_REPOSITORIES",
                    ("Meine Repositories auflisten", "List my repositories"),
                    (
                        "Listet die Repositories, auf die die Verbindung zugreifen kann.",
                        "Lists the repositories the connection can access.",
                    ),
                    &page_props(),
                    &[],
                    &[REPO],
                    false,
                ),
                t(
                    "GITHUB_FIND_USER",
                    ("Nutzer suchen", "Find user"),
                    (
                        "Sucht ein GitHub-Profil per Login; ein unbekannter Login ergibt found=false.",
                        "Looks up a GitHub profile by login; an unknown login yields found=false.",
                    ),
                    &[("username", s("Login, also der Name hinter github.com/"))],
                    &["username"],
                    &[],
                    false,
                ),
                t(
                    "GITHUB_FIND_BRANCH",
                    ("Branch finden", "Find branch"),
                    (
                        "Liest einen Branch exakt nach Namen; ein fehlender Branch ergibt found=false.",
                        "Reads one branch by its exact name; a missing branch yields found=false.",
                    ),
                    &with_repo(vec![(
                        "branch",
                        s("Branch-Name, case-sensitive (darf / enthalten, z. B. feature/login)"),
                    )]),
                    &["owner", "repo", "branch"],
                    &[REPO],
                    false,
                ),
                t(
                    "GITHUB_LIST_BRANCHES",
                    ("Branches auflisten", "List branches"),
                    (
                        "Listet die Branches eines Repositorys (Quelle der Branch-Auswahl in der Piece).",
                        "Lists a repository's branches (the source of the piece's branch dropdown).",
                    ),
                    &with_repo_page(vec![]),
                    &["owner", "repo"],
                    &[REPO],
                    false,
                ),
                t(
                    "GITHUB_FIND_ISSUE",
                    ("Issue finden", "Find issue"),
                    (
                        "Sucht das erste Issue, dessen Titel den Text enthält, und meldet found.",
                        "Searches for the first issue whose title contains the text and reports found.",
                    ),
                    &with_repo(vec![
                        ("title", s("Wörter, nach denen im Issue-Titel gesucht wird")),
                        (
                            "state",
                            choice("Welche Issues durchsucht werden", &["open", "closed", "all"], "open"),
                        ),
                    ]),
                    &["owner", "repo", "title", "state"],
                    &[REPO],
                    false,
                ),
                t(
                    "GITHUB_GET_ISSUE",
                    ("Issue lesen", "Get issue"),
                    (
                        "Liest ein Issue per Nummer; Pull-Request-Nummern funktionieren ebenfalls.",
                        "Reads one issue by number; pull request numbers work too.",
                    ),
                    &with_repo(vec![("issue_number", number("Die Nummer hinter # auf GitHub."))]),
                    &["owner", "repo", "issue_number"],
                    &[REPO],
                    false,
                ),
                t(
                    "GITHUB_LIST_ISSUE_ASSIGNEES",
                    ("Zuweisbare Nutzer auflisten", "List issue assignees"),
                    (
                        "Listet die Nutzer, denen in diesem Repository Issues zugewiesen werden können.",
                        "Lists the users issues can be assigned to in this repository.",
                    ),
                    &with_repo_page(vec![]),
                    &["owner", "repo"],
                    &[REPO],
                    false,
                ),
                t(
                    "GITHUB_LIST_REPOSITORY_LABELS",
                    ("Labels auflisten", "List repository labels"),
                    (
                        "Listet die Labels eines Repositorys (Quelle der Label-Auswahl in der Piece).",
                        "Lists a repository's labels (the source of the piece's label dropdown).",
                    ),
                    &with_repo_page(vec![]),
                    &["owner", "repo"],
                    &[REPO],
                    false,
                ),
                t(
                    "GITHUB_LIST_REPOSITORY_MILESTONES",
                    ("Meilensteine auflisten", "List repository milestones"),
                    (
                        "Listet die offenen Meilensteine eines Repositorys (Quelle der Meilenstein-Auswahl).",
                        "Lists a repository's open milestones (the source of the milestone dropdown).",
                    ),
                    &with_repo_page(vec![]),
                    &["owner", "repo"],
                    &[REPO],
                    false,
                ),
                t(
                    "GITHUB_LIST_WORKFLOWS",
                    ("Actions-Workflows auflisten", "List workflows"),
                    (
                        "Listet die Actions-Workflows eines Repositorys; nur workflow_dispatch-fähige können gestartet werden.",
                        "Lists a repository's Actions workflows; only those with workflow_dispatch can run.",
                    ),
                    &with_repo_page(vec![]),
                    &["owner", "repo"],
                    &[REPO],
                    false,
                ),
                t(
                    "GITHUB_LIST_ENVIRONMENTS",
                    ("Environments auflisten", "List environments"),
                    (
                        "Listet die Deployment-Environments eines Repositorys.",
                        "Lists a repository's deployment environments.",
                    ),
                    &with_repo_page(vec![]),
                    &["owner", "repo"],
                    &[REPO],
                    false,
                ),
                t(
                    "GITHUB_GET_FILE_CONTENT",
                    ("Dateiinhalt lesen", "Get file content"),
                    (
                        "Liest eine Datei und dekodiert sie wie die Piece aus Base64 zu UTF-8.",
                        "Reads a file and decodes it from Base64 to UTF-8, like the piece does.",
                    ),
                    &with_repo(vec![
                        (
                            "path",
                            s("Pfad relativ zur Repository-Wurzel, z. B. src/lib/auth.ts"),
                        ),
                        ("ref", s("Branch, Tag oder Commit-SHA; leer = Default-Branch")),
                    ]),
                    &["owner", "repo", "path"],
                    &[REPO],
                    false,
                ),
                t(
                    "GITHUB_GET_WORKFLOW_RUN",
                    ("Workflow-Run lesen", "Get workflow run"),
                    (
                        "Liest einen Actions-Run mit Status und Ergebnis.",
                        "Reads one Actions run including its status and conclusion.",
                    ),
                    &with_repo(vec![("run_id", number("ID des Runs, aus dem Dispatch oder der Run-Liste."))]),
                    &["owner", "repo", "run_id"],
                    &[REPO],
                    false,
                ),
                t(
                    "GITHUB_FIND_DISCUSSION",
                    ("Diskussion finden", "Find discussion"),
                    (
                        "Löst die GraphQL-ID einer Diskussionsnummer auf (erster Schritt von create-discussion-comment).",
                        "Resolves the GraphQL node ID of a discussion number (first step of create-discussion-comment).",
                    ),
                    &with_repo(vec![("discussion_number", number("Die Nummer hinter # auf GitHub."))]),
                    &["owner", "repo", "discussion_number"],
                    &[REPO],
                    false,
                ),
                t(
                    "GITHUB_LIST_ISSUES",
                    ("Issues auflisten", "List issues"),
                    (
                        "Listet die Issues eines Repositorys; Pull Requests sind mit is_pull_request markiert.",
                        "Lists a repository's issues; pull requests are flagged with is_pull_request.",
                    ),
                    &with_repo_page(vec![(
                        "state",
                        choice("Welche Issues", &["open", "closed", "all"], "open"),
                    )]),
                    &["owner", "repo"],
                    &[REPO],
                    false,
                ),
                // ---------------------------------------------------- writes
                t(
                    "GITHUB_CREATE_ISSUE",
                    ("Issue erstellen", "Create issue"),
                    (
                        "Öffnet ein neues Issue in einem Repository; braucht --confirm.",
                        "Opens a new issue in a repository; requires --confirm.",
                    ),
                    &with_repo(vec![
                        ("title", s("Titel des Issues")),
                        ("description", s("Beschreibung; Markdown wird unterstützt.")),
                        ("labels", str_list("Label-Namen des Repositorys")),
                        ("assignees", str_list("Logins der Assignees")),
                    ]),
                    &["owner", "repo", "title"],
                    &[REPO],
                    true,
                ),
                t(
                    "GITHUB_UPDATE_ISSUE",
                    ("Issue aktualisieren", "Update issue"),
                    (
                        "Ändert nur die übergebenen Felder; Labels und Assignees werden ersetzt; braucht --confirm.",
                        "Changes only the fields you pass; labels and assignees are replaced; requires --confirm.",
                    ),
                    &with_repo(vec![
                        ("issue_number", number("Die Nummer hinter # auf GitHub.")),
                        ("title", s("Neuer Titel")),
                        ("body", s("Neue Beschreibung; Markdown wird unterstützt.")),
                        ("state", choice_opt("Neuer Zustand", &["open", "closed"])),
                        (
                            "state_reason",
                            choice_opt(
                                "Warum das Issue geschlossen oder wieder geöffnet wird",
                                &["completed", "not_planned", "reopened", "duplicate"],
                            ),
                        ),
                        ("milestone", number("Nummer des Meilensteins")),
                        ("labels", str_list("Ersetzt alle Label-Namen")),
                        ("assignees", str_list("Ersetzt alle Assignees")),
                    ]),
                    &["owner", "repo", "issue_number"],
                    &[REPO],
                    true,
                ),
                t(
                    "GITHUB_ADD_LABELS_TO_ISSUE",
                    ("Labels zum Issue hinzufügen", "Add labels to issue"),
                    (
                        "Hängt Labels an ein Issue und behält die vorhandenen; braucht --confirm.",
                        "Adds labels to an issue and keeps the existing ones; requires --confirm.",
                    ),
                    &with_repo(vec![
                        ("issue_number", number("Die Nummer hinter # auf GitHub.")),
                        ("labels", str_list("Label-Namen, die hinzukommen")),
                    ]),
                    &["owner", "repo", "issue_number", "labels"],
                    &[REPO],
                    true,
                ),
                t(
                    "GITHUB_CREATE_ISSUE_COMMENT",
                    ("Kommentar zu einem Issue schreiben", "Create comment on issue"),
                    (
                        "Schreibt einen Kommentar auf ein Issue oder einen Pull Request; braucht --confirm.",
                        "Posts a comment on an issue or pull request; requires --confirm.",
                    ),
                    &with_repo(vec![
                        ("issue_number", number("Die Nummer hinter # auf GitHub.")),
                        ("comment", s("Kommentartext; Markdown wird unterstützt.")),
                    ]),
                    &["owner", "repo", "issue_number", "comment"],
                    &[REPO],
                    true,
                ),
                t(
                    "GITHUB_CREATE_BRANCH",
                    ("Branch erstellen", "Create branch"),
                    (
                        "Legt einen Branch aus einer Commit-SHA an (die Piece liest die SHA vorher mit GITHUB_FIND_BRANCH); braucht --confirm.",
                        "Creates a branch from a commit SHA (the piece reads the SHA first with GITHUB_FIND_BRANCH); requires --confirm.",
                    ),
                    &with_repo(vec![
                        (
                            "source_sha",
                            s("SHA des Quell-Branches, aus result.commit_sha von GITHUB_FIND_BRANCH"),
                        ),
                        ("new_branch_name", s("Name des neuen Branches, z. B. feature/login")),
                    ]),
                    &["owner", "repo", "source_sha", "new_branch_name"],
                    &[REPO],
                    true,
                ),
                t(
                    "GITHUB_DELETE_BRANCH",
                    ("Branch löschen", "Delete branch"),
                    (
                        "Löscht einen Branch; das Repository bleibt bestehen; braucht --confirm.",
                        "Deletes a branch; the repository stays; requires --confirm.",
                    ),
                    &with_repo(vec![("branch", s("Branch-Name, z. B. main"))]),
                    &["owner", "repo", "branch"],
                    &[REPO],
                    true,
                ),
                t(
                    "GITHUB_LOCK_ISSUE",
                    ("Issue sperren", "Lock issue"),
                    (
                        "Sperrt ein Issue, sodass nur Mitwirkende kommentieren können; braucht --confirm.",
                        "Locks an issue so only collaborators can comment; requires --confirm.",
                    ),
                    &with_repo(vec![
                        ("issue_number", number("Die Nummer hinter # auf GitHub.")),
                        (
                            "lock_reason",
                            choice_opt(
                                "Wird in der Timeline des Issues angezeigt",
                                &["off-topic", "too heated", "resolved", "spam"],
                            ),
                        ),
                    ]),
                    &["owner", "repo", "issue_number"],
                    &[REPO],
                    true,
                ),
                t(
                    "GITHUB_UNLOCK_ISSUE",
                    ("Issue entsperren", "Unlock issue"),
                    (
                        "Entsperrt ein Issue; braucht --confirm.",
                        "Unlocks an issue; requires --confirm.",
                    ),
                    &with_repo(vec![("issue_number", number("Die Nummer hinter # auf GitHub."))]),
                    &["owner", "repo", "issue_number"],
                    &[REPO],
                    true,
                ),
                t(
                    "GITHUB_CREATE_GIST",
                    ("Gist erstellen", "Create gist"),
                    (
                        "Legt einen Gist mit einer Datei an; braucht einen User-Token, kein App-Token; braucht --confirm.",
                        "Creates a gist with one file; needs a user token, not an app token; requires --confirm.",
                    ),
                    &[
                        ("description", s("Wird über der Datei auf der Gist-Seite angezeigt.")),
                        (
                            "public",
                            flag(
                                "Aus für einen geheimen Gist. Wer den Link hat, kann ihn trotzdem öffnen.",
                                true,
                            ),
                        ),
                        ("filename", s("Dateiname mit Endung, damit GitHub den Code hervorhebt.")),
                        ("content", s("Inhalt der Datei.")),
                    ],
                    &["filename", "content"],
                    &[GIST],
                    true,
                ),
                t(
                    "GITHUB_CREATE_COMMIT_COMMENT",
                    ("Commit-Kommentar erstellen", "Create commit comment"),
                    (
                        "Kommentiert einen Commit, optional auf eine Position in einem Diff; braucht --confirm.",
                        "Comments on a commit, optionally on a position in a diff; requires --confirm.",
                    ),
                    &with_repo(vec![
                        ("sha", s("Vollständiger oder kurzer Commit-Hash.")),
                        ("body", s("Kommentartext; Markdown wird unterstützt.")),
                        ("path", s("Pfad relativ zur Repository-Wurzel.")),
                        ("position", number("Zeilen ab dem ersten @@-Hunk-Header; braucht einen Pfad.")),
                    ]),
                    &["owner", "repo", "sha", "body"],
                    &[REPO],
                    true,
                ),
                t(
                    "GITHUB_CREATE_PULL_REQUEST_REVIEW_COMMENT",
                    ("Review-Kommentar erstellen", "Create pull request review comment"),
                    (
                        "Kommentiert eine Stelle im Diff eines Pull Requests; braucht --confirm.",
                        "Comments on a position in a pull request diff; requires --confirm.",
                    ),
                    &with_repo(vec![
                        ("pull_number", number("Die Nummer des Pull Requests hinter # auf GitHub.")),
                        ("commit_id", s("Commit-SHA; der letzte Commit des Pull Requests.")),
                        ("path", s("Pfad relativ zur Repository-Wurzel.")),
                        ("body", s("Kommentartext; Markdown wird unterstützt.")),
                        ("position", number("Zeilen ab dem ersten @@-Hunk-Header in dieser Datei.")),
                    ]),
                    &["owner", "repo", "pull_number", "commit_id", "path", "body", "position"],
                    &[REPO],
                    true,
                ),
                t(
                    "GITHUB_CREATE_DISCUSSION_COMMENT",
                    ("Diskussionskommentar erstellen", "Create discussion comment"),
                    (
                        "Schreibt einen Kommentar auf eine Diskussion; nutzt GITHUB_FIND_DISCUSSION für die ID; braucht --confirm.",
                        "Posts a comment on a discussion; use GITHUB_FIND_DISCUSSION for the node ID; requires --confirm.",
                    ),
                    &with_repo(vec![
                        ("discussion_id", s("GraphQL-ID der Diskussion, aus GITHUB_FIND_DISCUSSION.")),
                        ("body", s("Kommentartext; Markdown wird unterstützt.")),
                    ]),
                    &["owner", "repo", "discussion_id", "body"],
                    &[REPO],
                    true,
                ),
                t(
                    "GITHUB_TRIGGER_WORKFLOW_DISPATCH",
                    ("Workflow starten", "Trigger workflow dispatch"),
                    (
                        "Startet einen Actions-Workflow auf einem Branch oder Tag; der Workflow braucht on: workflow_dispatch; gibt die Run-ID zurück (GITHUB_GET_WORKFLOW_RUN liest den Status); braucht --confirm.",
                        "Starts an Actions workflow on a branch or tag; the workflow needs on: workflow_dispatch; returns the run ID (GITHUB_GET_WORKFLOW_RUN reads the status); requires --confirm.",
                    ),
                    &with_repo(vec![
                        ("workflow_id", number("ID des Workflows, aus GITHUB_LIST_WORKFLOWS.")),
                        ("ref", s("Branch oder Tag, auf dem gestartet wird.")),
                        (
                            "inputs",
                            object("Werte für die Inputs, die der Workflow in der YAML deklariert."),
                        ),
                    ]),
                    &["owner", "repo", "workflow_id", "ref"],
                    &[REPO],
                    true,
                ),
            ],
        };
        Self {
            spec,
            api_base: api_base.trim_end_matches('/').to_string(),
        }
    }

    /// GitHub asks for `Accept: application/vnd.github+json` plus the pinned API
    /// version — exactly the headers `getConnectionIdentifier` sends in
    /// `src/lib/auth.ts`.
    fn base(&self, method: &str, path: &str) -> HttpRequest {
        HttpRequest::new(method, format!("{}{path}", self.api_base))
            .header("accept", "application/vnd.github+json")
            .header("x-github-api-version", "2022-11-28")
    }

    /// `run()` of one tool: exactly one provider request.
    #[allow(clippy::too_many_lines)]
    fn request(&self, tool: &str, args: &Value) -> Result<HttpRequest> {
        let (owner, repo) = match tool {
            "GITHUB_GET_AUTHENTICATED_USER" | "GITHUB_LIST_MY_REPOSITORIES" | "GITHUB_CREATE_GIST"
            | "GITHUB_FIND_USER" => {
                (String::new(), String::new())
            }
            _ => repo_of(args)?,
        };
        let repo_path = |suffix: &str| format!("/repos/{owner}/{repo}{suffix}");
        match tool {
            // AP: src/lib/common/auth-helpers.ts (`getAuthenticatedLogin`)
            "GITHUB_GET_AUTHENTICATED_USER" => Ok(self.base("GET", "/user")),
            // AP: src/lib/common/index.ts (`getUserRepo` -> /user/repos)
            "GITHUB_LIST_MY_REPOSITORIES" => Ok(self.base("GET", &format!("/user/repos{}", page_query(args, &[])))),
            // AP: src/lib/actions/find-user.ts
            "GITHUB_FIND_USER" => {
                let username = path_segment(args, "username")?;
                Ok(self.base("GET", &format!("/users/{username}")))
            }
            // AP: src/lib/actions/find-branch.ts
            "GITHUB_FIND_BRANCH" => {
                let branch = path_multi(args, "branch")?;
                Ok(self.base("GET", &repo_path(&format!("/branches/{branch}"))))
            }
            // AP: src/lib/common/index.ts (`branchDropdown`)
            "GITHUB_LIST_BRANCHES" => Ok(self.base("GET", &repo_path(&format!("/branches{}", page_query(args, &[]))))),
            // AP: src/lib/actions/ai/get-repository.ts (`/repos/{owner}/{repo}`)
            "GITHUB_GET_REPO" => Ok(self.base("GET", &repo_path(""))),
            // AP: src/lib/actions/get-issue-information.ts
            "GITHUB_GET_ISSUE" => {
                let n = number_of(args, "issue_number")?;
                Ok(self.base("GET", &repo_path(&format!("/issues/{n}"))))
            }
            // AP: src/lib/common/index.ts (`issueDropdown`)
            "GITHUB_LIST_ISSUES" => {
                let state = opt_str(args, "state").unwrap_or("open");
                if !matches!(state, "open" | "closed" | "all") {
                    bail!("`state` must be open, closed or all");
                }
                Ok(self.base("GET", &repo_path(&format!("/issues{}", page_query(args, &[("state", Some(state.into()))])))))
            }
            // AP: src/lib/common/index.ts (`getAssignee`)
            "GITHUB_LIST_ISSUE_ASSIGNEES" => {
                Ok(self.base("GET", &repo_path(&format!("/assignees{}", page_query(args, &[])))))
            }
            // AP: src/lib/common/index.ts (`listIssueLabels`)
            "GITHUB_LIST_REPOSITORY_LABELS" => {
                Ok(self.base("GET", &repo_path(&format!("/labels{}", page_query(args, &[])))))
            }
            // AP: src/lib/common/index.ts (`milestoneDropdown`)
            "GITHUB_LIST_REPOSITORY_MILESTONES" => {
                Ok(self.base("GET", &repo_path(&format!("/milestones{}", page_query(args, &[])))))
            }
            // AP: src/lib/common/index.ts (`getWorkflows`)
            "GITHUB_LIST_WORKFLOWS" => {
                Ok(self.base("GET", &repo_path(&format!("/actions/workflows{}", page_query(args, &[])))))
            }
            // AP: src/lib/common/index.ts (`getRepoEnvironments`)
            "GITHUB_LIST_ENVIRONMENTS" => {
                Ok(self.base("GET", &repo_path(&format!("/environments{}", page_query(args, &[])))))
            }
            // AP: src/lib/common/index.ts (`getRepoFileContent`)
            "GITHUB_GET_FILE_CONTENT" => {
                let path = path_multi(args, "path")?;
                let qs = query(&[("ref", opt_str(args, "ref").map(String::from))]);
                Ok(self.base("GET", &repo_path(&format!("/contents/{path}{qs}"))))
            }
            // AP: src/lib/common/index.ts (`getWorkflowRun`)
            "GITHUB_GET_WORKFLOW_RUN" => {
                let run_id = number_of(args, "run_id")?;
                Ok(self.base("GET", &repo_path(&format!("/actions/runs/{run_id}"))))
            }
            // AP: src/lib/actions/find-issue.ts
            "GITHUB_FIND_ISSUE" => {
                let title = req_str(args, "title")?;
                let state = opt_str(args, "state").unwrap_or("open");
                if !matches!(state, "open" | "closed" | "all") {
                    bail!("`state` must be open, closed or all");
                }
                // The piece replaces `"` and `\` with a space, then quotes the phrase.
                let phrase: String = title
                    .chars()
                    .map(|c| if matches!(c, '"' | '\\') { ' ' } else { c })
                    .collect();
                let mut q = format!("repo:{owner}/{repo} is:issue in:title \"{phrase}\"");
                if state != "all" {
                    q.push_str(&format!(" state:{state}"));
                }
                Ok(self.base(
                    "GET",
                    &format!(
                        "/search/issues{}",
                        query(&[("q", Some(q)), ("per_page", Some("1".into()))])
                    ),
                ))
            }
            // AP: src/lib/actions/create-discussion-comment.ts (first request)
            "GITHUB_FIND_DISCUSSION" => {
                let number = number_of(args, "discussion_number")?;
                let variables = obj(vec![
                    ("owner", json!(owner)),
                    ("name", json!(repo)),
                    ("number", json!(number)),
                ]);
                Ok(self
                    .base("POST", "/graphql")
                    .json(&json!({"query": DISCUSSION_LOOKUP_QUERY, "variables": variables})))
            }
            // AP: src/lib/actions/create-issue.ts
            "GITHUB_CREATE_ISSUE" => {
                let mut body = Map::new();
                body.insert("title".into(), json!(req_str(args, "title")?.trim()));
                put_str(&mut body, args, "description", "body");
                put_list(&mut body, args, "labels", "labels");
                put_list(&mut body, args, "assignees", "assignees");
                Ok(self
                    .base("POST", &repo_path("/issues"))
                    .json(&Value::Object(body)))
            }
            // AP: src/lib/actions/update-issue.ts (only the fields you pass)
            "GITHUB_UPDATE_ISSUE" => {
                let n = number_of(args, "issue_number")?;
                let mut body = Map::new();
                put_str(&mut body, args, "title", "title");
                put_str(&mut body, args, "body", "body");
                put_str(&mut body, args, "state", "state");
                put_str(&mut body, args, "state_reason", "state_reason");
                put_list(&mut body, args, "labels", "labels");
                put_list(&mut body, args, "assignees", "assignees");
                if let Some(m) = opt_number(args, "milestone") {
                    body.insert("milestone".into(), json!(m));
                }
                if body.is_empty() {
                    bail!("nothing to update: pass at least one field to change");
                }
                Ok(self
                    .base("PATCH", &repo_path(&format!("/issues/{n}")))
                    .json(&Value::Object(body)))
            }
            // AP: src/lib/actions/add-labels-to-issue.ts
            "GITHUB_ADD_LABELS_TO_ISSUE" => {
                let n = number_of(args, "issue_number")?;
                let labels = str_vec(args, "labels");
                if labels.is_empty() {
                    bail!("`labels` needs at least one label name");
                }
                Ok(self
                    .base("POST", &repo_path(&format!("/issues/{n}/labels")))
                    .json(&json!({"labels": labels})))
            }
            // AP: src/lib/actions/create-comment-on-a-issue.ts
            "GITHUB_CREATE_ISSUE_COMMENT" => {
                let n = number_of(args, "issue_number")?;
                Ok(self
                    .base("POST", &repo_path(&format!("/issues/{n}/comments")))
                    .json(&json!({"body": req_str(args, "comment")?})))
            }
            // AP: src/lib/actions/create-branch.ts (the piece resolves the SHA with
            // `GET /repos/{o}/{r}/branches/{source_branch}` before this call)
            "GITHUB_CREATE_BRANCH" => {
                let source_sha = path_segment(args, "source_sha")?;
                let new_branch = path_multi(args, "new_branch_name")?;
                Ok(self
                    .base("POST", &repo_path("/git/refs"))
                    .json(&json!({
                        "ref": format!("refs/heads/{new_branch}"),
                        "sha": source_sha,
                    })))
            }
            // AP: src/lib/actions/delete-branch.ts
            "GITHUB_DELETE_BRANCH" => {
                let branch = path_multi(args, "branch")?;
                Ok(self.base("DELETE", &repo_path(&format!("/git/refs/heads/{branch}"))))
            }
            // AP: src/lib/actions/lock-issue.ts
            "GITHUB_LOCK_ISSUE" => {
                let n = number_of(args, "issue_number")?;
                let mut body = Map::new();
                put_str(&mut body, args, "lock_reason", "lock_reason");
                Ok(self
                    .base("PUT", &repo_path(&format!("/issues/{n}/lock")))
                    .json(&Value::Object(body)))
            }
            // AP: src/lib/actions/unlock-issue.ts
            "GITHUB_UNLOCK_ISSUE" => {
                let n = number_of(args, "issue_number")?;
                Ok(self.base("DELETE", &repo_path(&format!("/issues/{n}/lock"))))
            }
            // AP: src/lib/actions/create-gist.ts
            "GITHUB_CREATE_GIST" => {
                let filename = path_segment(args, "filename")?;
                let mut body = Map::new();
                put_str(&mut body, args, "description", "description");
                body.insert(
                    "public".into(),
                    json!(args.get("public").and_then(Value::as_bool).unwrap_or(true)),
                );
                body.insert(
                    "files".into(),
                    json!({filename: {"content": req_str(args, "content")?}}),
                );
                Ok(self.base("POST", "/gists").json(&Value::Object(body)))
            }
            // AP: src/lib/actions/create-commit-comment.ts
            "GITHUB_CREATE_COMMIT_COMMENT" => {
                let sha = path_segment(args, "sha")?;
                let mut body = Map::new();
                body.insert("body".into(), json!(req_str(args, "body")?));
                put_str(&mut body, args, "path", "path");
                if let Some(p) = opt_number(args, "position") {
                    body.insert("position".into(), json!(p));
                }
                Ok(self
                    .base("POST", &repo_path(&format!("/commits/{sha}/comments")))
                    .json(&Value::Object(body)))
            }
            // AP: src/lib/actions/create-pull-request-review-comment.ts
            "GITHUB_CREATE_PULL_REQUEST_REVIEW_COMMENT" => {
                let n = number_of(args, "pull_number")?;
                let mut body = Map::new();
                body.insert("commit_id".into(), json!(path_segment(args, "commit_id")?));
                body.insert("path".into(), json!(req_str(args, "path")?));
                body.insert("body".into(), json!(req_str(args, "body")?));
                if let Some(p) = opt_number(args, "position") {
                    body.insert("position".into(), json!(p));
                }
                Ok(self
                    .base("POST", &repo_path(&format!("/pulls/{n}/comments")))
                    .json(&Value::Object(body)))
            }
            // AP: src/lib/actions/create-discussion-comment.ts (second request)
            "GITHUB_CREATE_DISCUSSION_COMMENT" => {
                let discussion_id = path_segment(args, "discussion_id")?;
                let variables = obj(vec![
                    ("discussionId", json!(discussion_id)),
                    ("body", json!(req_str(args, "body")?)),
                ]);
                Ok(self
                    .base("POST", "/graphql")
                    .json(&json!({"query": ADD_DISCUSSION_COMMENT_MUTATION, "variables": variables})))
            }
            // AP: src/lib/actions/trigger-workflow-dispatch.ts
            "GITHUB_TRIGGER_WORKFLOW_DISPATCH" => {
                let workflow_id = number_of(args, "workflow_id")?;
                let mut body = Map::new();
                body.insert("ref".into(), json!(req_str(args, "ref")?.trim()));
                if let Some(inputs) = args.get("inputs").and_then(Value::as_object) {
                    if !inputs.is_empty() {
                        body.insert("inputs".into(), Value::Object(inputs.clone()));
                    }
                }
                body.insert("return_run_details".into(), json!(true));
                Ok(self
                    .base(
                        "POST",
                        &repo_path(&format!("/actions/workflows/{workflow_id}/dispatches")),
                    )
                    .json(&Value::Object(body)))
            }
            other => bail!("unknown GitHub tool `{other}`"),
        }
    }
}

/// `DISCUSSION_LOOKUP_QUERY` from `src/lib/actions/create-discussion-comment.ts`.
const DISCUSSION_LOOKUP_QUERY: &str = "query($owner: String!, $name: String!, $number: Int!) {\n  repository(owner: $owner, name: $name) {\n    discussion(number: $number) {\n      id\n    }\n  }\n}";

/// `ADD_DISCUSSION_COMMENT_MUTATION` from `src/lib/actions/create-discussion-comment.ts`.
const ADD_DISCUSSION_COMMENT_MUTATION: &str = "mutation($discussionId: ID!, $body: String!) {\n  addDiscussionComment(input: { discussionId: $discussionId, body: $body }) {\n    comment {\n      id\n      body\n      url\n    }\n  }\n}";

impl ToolkitAdapter for GitHub {
    fn spec(&self) -> &ToolkitSpec {
        &self.spec
    }

    fn build_request(&self, tool: &str, args: &Value) -> Result<HttpRequest> {
        self.request(tool, args)
    }

    fn parse_response(&self, tool: &str, response: &HttpResponse) -> Result<Value> {
        // `find_branch` / `find_user` / `find_issue` turn a 404 into `found:false`
        // in the piece, so the same shape is kept here.
        if response.status == 404
            && matches!(
                tool,
                "GITHUB_FIND_BRANCH" | "GITHUB_FIND_USER" | "GITHUB_FIND_DISCUSSION"
            )
        {
            return Ok(json!({"found": false, "result": {}}));
        }
        let v = response.json()?;
        let items = |field: &str| -> Vec<Value> {
            v[field].as_array().cloned().unwrap_or_default()
        };
        Ok(match tool {
            "GITHUB_GET_AUTHENTICATED_USER" => flatten_user(&v),
            "GITHUB_LIST_MY_REPOSITORIES" => {
                Value::Array(v.as_array().cloned().unwrap_or_default().iter().map(flatten_repository).collect())
            }
            "GITHUB_FIND_USER" => json!({"found": true, "result": flatten_user(&v)}),
            "GITHUB_FIND_BRANCH" => json!({"found": true, "result": flatten_branch(&v)}),
            "GITHUB_LIST_BRANCHES" => {
                Value::Array(v.as_array().cloned().unwrap_or_default().iter().map(flatten_branch).collect())
            }
            "GITHUB_FIND_ISSUE" => {
                let found = v["total_count"].as_u64().unwrap_or(0) > 0;
                json!({"found": found,
                    "total_count": v["total_count"],
                    "result": v["items"].as_array().cloned().unwrap_or_default()
                        .iter().map(flatten_issue).collect::<Vec<_>>()})
            }
            "GITHUB_GET_REPO" => flatten_repository(&v),
            "GITHUB_GET_ISSUE" => flatten_issue(&v),
            "GITHUB_LIST_ISSUES" => Value::Array(
                v.as_array()
                    .cloned()
                    .unwrap_or_default()
                    .iter()
                    // GitHub's issues endpoint also returns pull requests. Filtering
                    // them out (as the piece does) made whole pages look empty in
                    // live tests, so they stay in the list, flagged.
                    .map(|i| {
                        json!({"number": i["number"], "title": i["title"], "state": i["state"],
                            "is_pull_request": i.get("pull_request").is_some(),
                            "html_url": i["html_url"], "user": i["user"]["login"], "labels":
                            i["labels"].as_array().map(|l| l.iter().map(|x| x["name"].clone()).collect::<Vec<_>>())})
                    })
                    .collect(),
            ),
            "GITHUB_LIST_ISSUE_ASSIGNEES" => Value::Array(
                v.as_array()
                    .cloned()
                    .unwrap_or_default()
                    .iter()
                    .map(flatten_user_summary)
                    .collect(),
            ),
            "GITHUB_LIST_REPOSITORY_LABELS" => {
                Value::Array(v.as_array().cloned().unwrap_or_default().iter().map(flatten_label).collect())
            }
            "GITHUB_LIST_REPOSITORY_MILESTONES" => Value::Array(
                v.as_array()
                    .cloned()
                    .unwrap_or_default()
                    .iter()
                    .map(flatten_milestone)
                    .collect(),
            ),
            "GITHUB_LIST_WORKFLOWS" => {
                Value::Array(items("workflows").iter().map(flatten_workflow).collect())
            }
            "GITHUB_LIST_ENVIRONMENTS" => Value::Array(
                items("environments")
                    .iter()
                    .map(|e| json!({"id": e["id"], "name": e["name"], "html_url": e["html_url"],
                        "protection_rules": e["protection_rules"], "created_at": e["created_at"],
                        "updated_at": e["updated_at"]}))
                    .collect(),
            ),
            "GITHUB_GET_FILE_CONTENT" => {
                let encoding = v["encoding"].as_str().unwrap_or_default();
                if encoding != "base64" {
                    bail!("unexpected encoding \"{encoding}\" returned for the file");
                }
                let raw = v["content"].as_str().unwrap_or_default();
                let bytes = base64::engine::general_purpose::STANDARD
                    // GitHub wraps base64 at 60 columns with `\n`.
                    .decode(raw.chars().filter(|c| !c.is_ascii_whitespace()).collect::<String>())
                    .map_err(|e| anyhow!("GitHub returned undecodable base64 content: {e}"))?;
                json!({
                    "path": v["path"], "name": v["name"], "sha": v["sha"], "size": v["size"],
                    "encoding": encoding, "html_url": v["html_url"], "download_url": v["download_url"],
                    "content": String::from_utf8_lossy(&bytes),
                })
            }
            "GITHUB_GET_WORKFLOW_RUN" => flatten_workflow_run(&v),
            "GITHUB_FIND_DISCUSSION" => {
                let id = &v["data"]["repository"]["discussion"]["id"];
                json!({"found": id.is_string(), "discussion_id": id})
            }
            "GITHUB_CREATE_ISSUE" | "GITHUB_UPDATE_ISSUE" => flatten_issue(&v),
            "GITHUB_ADD_LABELS_TO_ISSUE" => {
                let labels: Vec<Value> = v.as_array().cloned().unwrap_or_default();
                json!({
                    "count": labels.len(),
                    "labels": labels.iter().map(flatten_label).collect::<Vec<_>>(),
                })
            }
            "GITHUB_CREATE_ISSUE_COMMENT"
            | "GITHUB_CREATE_COMMIT_COMMENT"
            | "GITHUB_CREATE_PULL_REQUEST_REVIEW_COMMENT" => flatten_comment(&v),
            "GITHUB_CREATE_BRANCH" => json!({
                "ref": v["ref"], "node_id": v["node_id"], "url": v["url"],
                "sha": v["object"]["sha"], "object_type": v["object"]["type"],
            }),
            "GITHUB_DELETE_BRANCH" => json!({"deleted": true}),
            "GITHUB_LOCK_ISSUE" => json!({"locked": true}),
            "GITHUB_UNLOCK_ISSUE" => json!({"locked": false}),
            "GITHUB_CREATE_GIST" => flatten_gist(&v),
            "GITHUB_CREATE_DISCUSSION_COMMENT" => {
                let c = &v["data"]["addDiscussionComment"]["comment"];
                json!({"id": c["id"], "body": c["body"], "url": c["url"]})
            }
            "GITHUB_TRIGGER_WORKFLOW_DISPATCH" => json!({
                "dispatched": true, "workflow_run_id": v["workflow_run_id"],
            }),
            other => bail!("unknown GitHub tool `{other}`"),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn body_of(r: &HttpRequest) -> Value {
        serde_json::from_slice(r.body.as_deref().expect("body")).unwrap()
    }

    #[test]
    fn catalog_and_paths() {
        let gh = GitHub::with_api_base("https://gh.test/");
        let r = gh
            .build_request(
                "GITHUB_LIST_ISSUES",
                &json!({"owner": "octo", "repo": "hello", "per_page": 500}),
            )
            .unwrap();
        assert_eq!(r.method, "GET");
        assert_eq!(
            r.url,
            "https://gh.test/repos/octo/hello/issues?page=1&per_page=100&state=open"
        );

        let r = gh
            .build_request(
                "GITHUB_FIND_ISSUE",
                &json!({"owner": "octo", "repo": "hello", "title": "a \"b\" c", "state": "closed"}),
            )
            .unwrap();
        assert!(r.url.contains("/search/issues?"), "{}", r.url);
        assert!(r.url.contains("q=repo%3Aocto%2Fhello+is%3Aissue+in%3Atitle+%22a++b++c%22+state%3Aclosed"), "{}", r.url);
        assert!(r.url.contains("per_page=1"));

        let r = gh
            .build_request(
                "GITHUB_GET_FILE_CONTENT",
                &json!({"owner": "octo", "repo": "hello", "path": "src/lib/auth.ts", "ref": "main"}),
            )
            .unwrap();
        assert_eq!(r.url, "https://gh.test/repos/octo/hello/contents/src/lib/auth.ts?ref=main");

        let r = gh
            .build_request(
                "GITHUB_CREATE_BRANCH",
                &json!({"owner": "octo", "repo": "hello", "source_sha": "abc123", "new_branch_name": "feature/login"}),
            )
            .unwrap();
        assert_eq!(r.url, "https://gh.test/repos/octo/hello/git/refs");
        assert_eq!(body_of(&r), json!({"ref": "refs/heads/feature/login", "sha": "abc123"}));

        let r = gh
            .build_request(
                "GITHUB_TRIGGER_WORKFLOW_DISPATCH",
                &json!({"owner": "octo", "repo": "hello", "workflow_id": 7, "ref": "main",
                        "inputs": {"env": "prod"}}),
            )
            .unwrap();
        assert_eq!(r.url, "https://gh.test/repos/octo/hello/actions/workflows/7/dispatches");
        assert_eq!(
            body_of(&r),
            json!({"ref": "main", "inputs": {"env": "prod"}, "return_run_details": true})
        );
    }

    #[test]
    fn injection_is_rejected() {
        let gh = GitHub::with_api_base("https://gh.test");
        assert!(gh
            .build_request("GITHUB_GET_ISSUE", &json!({"owner": "a/../b", "repo": "x", "issue_number": 1}))
            .is_err());
        assert!(gh
            .build_request("GITHUB_FIND_BRANCH", &json!({"owner": "a", "repo": "b", "branch": "main/../../etc"}))
            .is_err());
        assert!(gh
            .build_request("GITHUB_GET_FILE_CONTENT", &json!({"owner": "a", "repo": "b", "path": "/etc/passwd"}))
            .is_err());
        assert!(gh
            .build_request(
                "GITHUB_FIND_ISSUE",
                &json!({"owner": "a", "repo": "b", "title": "t", "state": "x&y=1"})
            )
            .is_err());
        assert!(gh
            .build_request("GITHUB_UPDATE_ISSUE", &json!({"owner": "a", "repo": "b", "issue_number": 1}))
            .is_err());
        assert!(gh
            .build_request("GITHUB_CREATE_ISSUE", &json!({"owner": "a", "repo": "b", "title": "  "}))
            .is_err());
        assert!(gh.build_request("GITHUB_NOPE", &json!({})).is_err());
    }

    #[test]
    fn mappers_and_catalog_shape() {
        let gh = GitHub::with_api_base("https://gh.test");
        let issue = json!({"number": 7, "title": "T", "state": "open", "labels": [{"name": "bug"}],
            "assignees": [{"login": "ada"}], "user": {"login": "ada"}, "pull_request": {"url": "u"}});
        let flat = flatten_issue(&issue);
        assert_eq!(flat["label_names"], "bug");
        assert_eq!(flat["assignee_logins"], "ada");
        assert_eq!(flat["is_pull_request"], json!(true));

        let env = HttpResponse { status: 204, body: Vec::new(), session_id: None };
        assert_eq!(gh.parse_response("GITHUB_DELETE_BRANCH", &env).unwrap(), json!({"deleted": true}));
        let missing = HttpResponse { status: 404, body: br#"{"message":"Not Found"}"#.to_vec(), session_id: None };
        assert_eq!(
            gh.parse_response("GITHUB_FIND_USER", &missing).unwrap(),
            json!({"found": false, "result": {}})
        );

        for t in &gh.spec().tools {
            assert!(!t.title.de.is_empty() && !t.title.en.is_empty(), "{}", t.slug);
            assert!(!t.description.de.is_empty() && !t.description.en.is_empty(), "{}", t.slug);
            assert!(t.slug.starts_with("GITHUB_"), "{}", t.slug);
        }
        assert!(gh.spec().tools.iter().any(|t| t.mutating));
        assert!(gh.spec().tools.iter().any(|t| !t.mutating));
    }
}