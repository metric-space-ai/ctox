//! Linear toolkit (GraphQL API). Action set modelled on Activepieces `piece-linear` (MIT).
//!
//! Auth: a Linear personal API key (`lin_api_…`) sent verbatim in the `Authorization`
//! header, exactly like the `@linear/sdk` client the piece uses. Linear has no OAuth
//! scopes; the `scopes` of every tool therefore carry the API-key *permission level*
//! the call needs (`read`, `write`, `admin`), which is what Linear documents for keys.
//!
//! Every action is one `POST {api_base}/graphql` with `{query, variables}`, mirroring the
//! queries/mutations in `src/lib/actions/atomics` and `src/lib/common/queries.ts`.
//! Ids travel in GraphQL variables, so they are validated with `path_segment`
//! (restricted charset) before they are interpolated.
use anyhow::{anyhow, bail, Result};
use serde_json::{json, Map, Value};

use super::common::*;
use super::{path_segment, ToolkitAdapter};
use crate::http::{HttpRequest, HttpResponse};
use crate::model::*;

pub const SLUG: &str = "linear";

const READ: &str = "read";
const WRITE: &str = "write";
const ADMIN: &str = "admin";

// ---------------------------------------------------------------- GraphQL selections

const PAGE: &str = "pageInfo { hasNextPage endCursor } nodes";

const ISSUE_SELECTION: &str = "id identifier number title description url priority priorityLabel \
estimate dueDate branchName createdAt updatedAt startedAt completedAt canceledAt archivedAt \
trashed state { id name type } team { id key name } assignee { id name email } \
creator { id name email } project { id name } projectMilestone { id name } \
cycle { id number name } parent { id identifier title } \
labels(first: 20) { pageInfo { hasNextPage endCursor } nodes { id name } }";

const ARCHIVED_ISSUE_SELECTION: &str = "id identifier title url trashed archivedAt";

const COMMENT_SELECTION: &str = "id body url createdAt updatedAt editedAt resolvedAt \
user { id name email } resolvingUser { id name } issue { id identifier title } parent { id }";

const TEAM_SELECTION: &str = "id key name description icon color timezone cyclesEnabled \
triageEnabled defaultIssueEstimate issueEstimationType createdAt updatedAt archivedAt";

const USER_SELECTION: &str = "id name displayName email active admin guest url avatarUrl \
timezone createdAt";

const CYCLE_SELECTION: &str = "id number name startsAt endsAt completedAt progress isActive \
isNext isPrevious team { id key name }";

const LABEL_SELECTION: &str =
    "id name color description isGroup createdAt team { id key name } parent { id name }";

const STATE_SELECTION: &str = "id name type color position description team { id key name }";

const MILESTONE_SELECTION: &str =
    "id name description targetDate sortOrder status progress createdAt project { id name }";

const PROJECT_SELECTION: &str = "id name description url icon color priority priorityLabel \
progress health startDate targetDate createdAt updatedAt completedAt canceledAt archivedAt \
trashed status { id name type } lead { id name email } creator { id name } \
teams(first: 10) { pageInfo { hasNextPage endCursor } nodes { id key name } }";

const PROJECT_UPDATE_SELECTION: &str = "id body health url isDiffHidden createdAt updatedAt \
editedAt archivedAt user { id name email } project { id name }";

const ATTACHMENT_SELECTION: &str =
    "id title subtitle url sourceType createdAt updatedAt issue { id identifier title } creator { id name }";

// ---------------------------------------------------------------- schema fragments

/// `flag` is a boolean property with a default (Activepieces `Property.Checkbox`).
fn flag(desc: &str, default: bool) -> Value {
    json!({"type": "boolean", "description": desc, "default": default})
}

fn choice(desc: &str, options: &[&str]) -> Value {
    json!({"type": "string", "description": desc, "enum": options})
}

fn date(desc: &str) -> Value {
    json!({"type": "string", "description": desc, "pattern": "^\\d{4}-\\d{2}-\\d{2}$"})
}

fn limit(default: u64, max: u64) -> Value {
    json!({"type": "integer",
        "description": format!("How many records to return in this page, 1 to {max}. Default {default}."),
        "minimum": 1, "maximum": max, "default": default})
}

/// Priority 0-4; no schema default, so an unset value stays unset.
fn priority_prop() -> Value {
    json!({"type": "integer", "description": "0 keine, 1 dringend, 2 hoch, 3 mittel, 4 niedrig",
        "minimum": 0, "maximum": 4})
}

/// Estimate points; no schema default, so an unset value stays unset.
fn estimate_prop() -> Value {
    json!({"type": "integer", "description": "Schätzpunkte in der Skala des Teams",
        "minimum": 0, "maximum": 1000})
}

fn cursor() -> Value {
    s("Pass end_cursor from the previous call to get the next page.")
}

/// Pagination props shared by every list action.
fn page_props(fallback: u64, max: u64) -> Vec<(&'static str, Value)> {
    vec![("limit", limit(fallback, max)), ("cursor", cursor())]
}

/// `page_props` plus action specific props.
fn with_page(
    fallback: u64,
    max: u64,
    extra: Vec<(&'static str, Value)>,
) -> Vec<(&'static str, Value)> {
    let mut v = page_props(fallback, max);
    v.extend(extra);
    v
}

// ---------------------------------------------------------------- id helpers

/// UUID as Linear writes them.
fn is_uuid(v: &str) -> bool {
    v.len() == 36
        && v.bytes().enumerate().all(|(i, b)| match i {
            8 | 13 | 18 | 23 => b == b'-',
            _ => b.is_ascii_hexdigit(),
        })
}

/// Human issue key such as `ENG-123`.
fn is_identifier(v: &str) -> bool {
    let Some((team, number)) = v.split_once('-') else {
        return false;
    };
    !team.is_empty()
        && team.starts_with(|c: char| c.is_ascii_alphabetic())
        && team
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'_')
        && !number.is_empty()
        && number.bytes().all(|b| b.is_ascii_digit())
}

/// Issue id or identifier; identifiers are upper-cased like `resolveIssueId` does.
fn issue_ref(args: &Value, key: &str) -> Result<String> {
    let raw = path_segment(args, key)?;
    if is_uuid(raw) {
        return Ok(raw.to_string());
    }
    if is_identifier(raw) {
        return Ok(raw.to_ascii_uppercase());
    }
    bail!("`{key}` must be an issue UUID or an identifier like ENG-123")
}

fn id_filter(v: &str) -> Value {
    json!({"id": {"eq": v}})
}

/// `definedOnly` from the piece: empty/blank values are dropped from the input object.
fn put_str(m: &mut Map<String, Value>, args: &Value, key: &str, field: &str) {
    if let Some(v) = opt_str(args, key) {
        m.insert(field.into(), json!(v));
    }
}

fn put_priority(m: &mut Map<String, Value>, args: &Value, key: &str) -> Result<()> {
    if args.get(key).and_then(Value::as_u64).is_some() {
        m.insert("priority".into(), json!(checked_priority(args)?));
    }
    Ok(())
}

fn opt_u64_or_err(args: &Value, key: &str) -> Result<Option<u64>> {
    match args.get(key) {
        None | Some(Value::Null) => Ok(None),
        Some(v) => v
            .as_u64()
            .map(Some)
            .ok_or_else(|| anyhow!("`{key}` must be a whole number")),
    }
}

/// `toTimelessDate`: accepts `YYYY-MM-DD` (or a full ISO time) and sends the date part.
fn put_timeless_date(
    m: &mut Map<String, Value>,
    args: &Value,
    key: &str,
    field: &str,
) -> Result<()> {
    if let Some(v) = opt_str(args, key) {
        let head: String = v.trim().chars().take(10).collect();
        if !looks_like_date(&head) {
            bail!("`{key}` must be a date in YYYY-MM-DD format, got \"{v}\".");
        }
        m.insert(field.into(), json!(head));
    }
    Ok(())
}

fn looks_like_date(v: &str) -> bool {
    v.len() == 10
        && v.bytes().enumerate().all(|(i, b)| match i {
            4 | 7 => b == b'-',
            _ => b.is_ascii_digit(),
        })
}

fn put_labels(m: &mut Map<String, Value>, args: &Value, key: &str, field: &str) {
    let ids = str_vec(args, key);
    if !ids.is_empty() {
        m.insert(field.into(), json!(ids));
    }
}

fn obj(pairs: Vec<(&str, Value)>) -> Value {
    let mut m = Map::new();
    for (k, v) in pairs {
        m.insert(k.into(), v);
    }
    Value::Object(m)
}

/// Merge `extra` into `base` (both objects); keeps wide `json!` literals readable.
fn extend(base: &mut Value, extra: Value) {
    if let (Some(b), Some(e)) = (base.as_object_mut(), extra.as_object()) {
        for (k, v) in e {
            b.insert(k.clone(), v.clone());
        }
    }
}

// ---------------------------------------------------------------- mappers

fn nodes(conn: &Value) -> Vec<Value> {
    conn["nodes"].as_array().cloned().unwrap_or_default()
}

fn page(conn: &Value, map: fn(&Value) -> Value) -> Value {
    let items: Vec<Value> = nodes(conn).iter().map(map).collect();
    json!({
        "items": items.clone(),
        "count": items.len(),
        "has_next_page": conn["pageInfo"]["hasNextPage"],
        "end_cursor": conn["pageInfo"]["endCursor"],
    })
}

fn flatten_issue(i: &Value) -> Value {
    let labels = nodes(&i["labels"]);
    let names: Vec<&str> = labels.iter().filter_map(|l| l["name"].as_str()).collect();
    let mut out = json!({
        "id": i["id"],
        "identifier": i["identifier"],
        "number": i["number"],
        "title": i["title"],
        "description": i["description"],
        "url": i["url"],
        "priority": i["priority"],
        "priority_label": i["priorityLabel"],
        "estimate": i["estimate"],
        "due_date": i["dueDate"],
        "branch_name": i["branchName"],
        "state_id": i["state"]["id"],
        "state_name": i["state"]["name"],
        "state_type": i["state"]["type"],
        "team_id": i["team"]["id"],
        "team_key": i["team"]["key"],
        "team_name": i["team"]["name"],
    });
    extend(&mut out, json!({
        "assignee_id": i["assignee"]["id"],
        "assignee_name": i["assignee"]["name"],
        "assignee_email": i["assignee"]["email"],
        "creator_id": i["creator"]["id"],
        "creator_name": i["creator"]["name"],
        "creator_email": i["creator"]["email"],
        "project_id": i["project"]["id"],
        "project_name": i["project"]["name"],
        "project_milestone_id": i["projectMilestone"]["id"],
        "project_milestone_name": i["projectMilestone"]["name"],
        "cycle_id": i["cycle"]["id"],
        "cycle_number": i["cycle"]["number"],
        "cycle_name": i["cycle"]["name"],
        "parent_id": i["parent"]["id"],
        "parent_identifier": i["parent"]["identifier"],
        "parent_title": i["parent"]["title"],
    }));
    extend(&mut out, json!({
        "label_ids": labels.iter().map(|l| l["id"].clone()).collect::<Vec<_>>(),
        "label_names": names.join(", "),
        "labels_complete": i["labels"]["pageInfo"]["hasNextPage"] != json!(true),
        "created_at": i["createdAt"],
        "updated_at": i["updatedAt"],
        "started_at": i["startedAt"],
        "completed_at": i["completedAt"],
        "canceled_at": i["canceledAt"],
        "archived_at": i["archivedAt"],
        "trashed": if i["trashed"].is_boolean() { i["trashed"].clone() } else { json!(false) },
    }));
    out
}

fn flatten_archived_issue(e: &Value) -> Value {
    json!({"success": true, "id": e["id"], "identifier": e["identifier"], "title": e["title"],
        "url": e["url"], "trashed": e["trashed"], "archived_at": e["archivedAt"]})
}

fn flatten_comment(c: &Value) -> Value {
    json!({
        "id": c["id"],
        "body": c["body"],
        "url": c["url"],
        "user_id": c["user"]["id"],
        "user_name": c["user"]["name"],
        "user_email": c["user"]["email"],
        "issue_id": c["issue"]["id"],
        "issue_identifier": c["issue"]["identifier"],
        "issue_title": c["issue"]["title"],
        "parent_id": c["parent"]["id"],
        "resolved_at": c["resolvedAt"],
        "resolving_user_id": c["resolvingUser"]["id"],
        "resolving_user_name": c["resolvingUser"]["name"],
        "created_at": c["createdAt"],
        "updated_at": c["updatedAt"],
        "edited_at": c["editedAt"],
    })
}

fn flatten_team(t: &Value) -> Value {
    json!({
        "id": t["id"], "key": t["key"], "name": t["name"], "description": t["description"],
        "icon": t["icon"], "color": t["color"], "timezone": t["timezone"],
        "cycles_enabled": t["cyclesEnabled"], "triage_enabled": t["triageEnabled"],
        "default_issue_estimate": t["defaultIssueEstimate"],
        "issue_estimation_type": t["issueEstimationType"],
        "created_at": t["createdAt"], "updated_at": t["updatedAt"], "archived_at": t["archivedAt"],
    })
}

fn flatten_team_details(t: &Value) -> Value {
    let mut v = flatten_team(t);
    v["issue_count"] = t["issueCount"].clone();
    v["default_issue_state_id"] = t["defaultIssueState"]["id"].clone();
    v["default_issue_state_name"] = t["defaultIssueState"]["name"].clone();
    v["default_issue_state_type"] = t["defaultIssueState"]["type"].clone();
    v
}

fn flatten_user(u: &Value) -> Value {
    json!({
        "id": u["id"], "name": u["name"], "display_name": u["displayName"], "email": u["email"],
        "active": u["active"], "admin": u["admin"], "guest": u["guest"], "url": u["url"],
        "avatar_url": u["avatarUrl"], "timezone": u["timezone"], "created_at": u["createdAt"],
    })
}

fn flatten_viewer(u: &Value) -> Value {
    let mut v = flatten_user(u);
    v["organization_id"] = u["organization"]["id"].clone();
    v["organization_name"] = u["organization"]["name"].clone();
    v["organization_url_key"] = u["organization"]["urlKey"].clone();
    v
}

fn flatten_cycle(c: &Value) -> Value {
    json!({
        "id": c["id"], "number": c["number"], "name": c["name"], "starts_at": c["startsAt"],
        "ends_at": c["endsAt"], "completed_at": c["completedAt"], "progress": c["progress"],
        "is_active": c["isActive"], "is_next": c["isNext"], "is_previous": c["isPrevious"],
        "team_id": c["team"]["id"], "team_key": c["team"]["key"], "team_name": c["team"]["name"],
    })
}

fn flatten_label(l: &Value) -> Value {
    json!({
        "id": l["id"], "name": l["name"], "color": l["color"], "description": l["description"],
        "is_group": l["isGroup"], "is_workspace_label": !l["team"].is_object() || l["team"].as_object().is_some_and(Map::is_empty),
        "team_id": l["team"]["id"], "team_key": l["team"]["key"], "team_name": l["team"]["name"],
        "parent_id": l["parent"]["id"], "parent_name": l["parent"]["name"],
        "created_at": l["createdAt"],
    })
}

fn flatten_state(s: &Value) -> Value {
    json!({
        "id": s["id"], "name": s["name"], "type": s["type"], "color": s["color"],
        "position": s["position"], "description": s["description"],
        "team_id": s["team"]["id"], "team_key": s["team"]["key"], "team_name": s["team"]["name"],
    })
}

fn flatten_milestone(m: &Value) -> Value {
    json!({
        "id": m["id"], "name": m["name"], "description": m["description"],
        "target_date": m["targetDate"], "sort_order": m["sortOrder"], "status": m["status"],
        "progress": m["progress"], "project_id": m["project"]["id"],
        "project_name": m["project"]["name"], "created_at": m["createdAt"],
    })
}

fn flatten_project(p: &Value) -> Value {
    let teams = nodes(&p["teams"]);
    let names: Vec<&str> = teams.iter().filter_map(|t| t["name"].as_str()).collect();
    json!({
        "id": p["id"], "name": p["name"], "description": p["description"], "url": p["url"],
        "icon": p["icon"], "color": p["color"], "priority": p["priority"],
        "priority_label": p["priorityLabel"], "progress": p["progress"], "health": p["health"],
        "start_date": p["startDate"], "target_date": p["targetDate"],
        "status_id": p["status"]["id"], "status_name": p["status"]["name"],
        "status_type": p["status"]["type"],
        "lead_id": p["lead"]["id"], "lead_name": p["lead"]["name"], "lead_email": p["lead"]["email"],
        "creator_id": p["creator"]["id"], "creator_name": p["creator"]["name"],
        "team_ids": teams.iter().map(|t| t["id"].clone()).collect::<Vec<_>>(),
        "team_names": names.join(", "),
        "teams_complete": p["teams"]["pageInfo"]["hasNextPage"] != json!(true),
        "created_at": p["createdAt"], "updated_at": p["updatedAt"],
        "completed_at": p["completedAt"], "canceled_at": p["canceledAt"],
        "archived_at": p["archivedAt"],
        "trashed": if p["trashed"].is_boolean() { p["trashed"].clone() } else { json!(false) },
    })
}

fn flatten_project_with_milestones(p: &Value) -> Value {
    let milestones: Vec<Value> = nodes(&p["projectMilestones"])
        .iter()
        .map(|m| {
            json!({"id": m["id"], "name": m["name"], "target_date": m["targetDate"], "status": m["status"]})
        })
        .collect();
    let mut v = flatten_project(p);
    v["milestones"] = Value::Array(milestones);
    v["milestones_complete"] = json!(p["projectMilestones"]["pageInfo"]["hasNextPage"] != json!(true));
    v
}

fn flatten_status_update(u: &Value) -> Value {
    json!({
        "id": u["id"], "body": u["body"], "health": u["health"], "url": u["url"],
        "is_diff_hidden": u["isDiffHidden"],
        "user_id": u["user"]["id"], "user_name": u["user"]["name"], "user_email": u["user"]["email"],
        "project_id": u["project"]["id"], "project_name": u["project"]["name"],
        "created_at": u["createdAt"], "updated_at": u["updatedAt"], "edited_at": u["editedAt"],
        "archived_at": u["archivedAt"],
    })
}

fn flatten_attachment(a: &Value) -> Value {
    json!({
        "id": a["id"], "title": a["title"], "subtitle": a["subtitle"], "url": a["url"],
        "source_type": a["sourceType"],
        "issue_id": a["issue"]["id"], "issue_identifier": a["issue"]["identifier"],
        "issue_title": a["issue"]["title"],
        "creator_id": a["creator"]["id"], "creator_name": a["creator"]["name"],
        "created_at": a["createdAt"], "updated_at": a["updatedAt"],
    })
}

fn flatten_relation(r: &Value) -> Value {
    json!({
        "id": r["id"], "type": r["type"],
        "issue_id": r["issue"]["id"], "issue_identifier": r["issue"]["identifier"],
        "issue_title": r["issue"]["title"],
        "related_issue_id": r["relatedIssue"]["id"],
        "related_issue_identifier": r["relatedIssue"]["identifier"],
        "related_issue_title": r["relatedIssue"]["title"],
        "created_at": r["createdAt"],
    })
}

// ---------------------------------------------------------------- adapter

pub struct Linear {
    spec: ToolkitSpec,
    api_base: String,
}

impl Default for Linear {
    fn default() -> Self {
        Self::with_api_base("https://api.linear.app")
    }
}

#[allow(clippy::too_many_lines)]
impl Linear {
    pub fn with_api_base(api_base: &str) -> Self {
        let t = |slug, title, desc, props: Props<'_>, req: &[&str], scopes: &[&str], m| {
            tool(SLUG, slug, title, desc, props, req, scopes, m)
        };
        let spec = ToolkitSpec {
            slug: SLUG.into(),
            name: "Linear".into(),
            description: LocalizedText::new(
                "Issues, Projekte, Teams und Kommentare in Linear lesen und bearbeiten.",
                "Read and change issues, projects, teams and comments in Linear.",
            ),
            auth_schemes: vec![AuthScheme::ApiKey {
                header_name: "authorization".into(),
                prefix: String::new(),
            }],
            tools: vec![
                t(
                    "LINEAR_GET_VIEWER",
                    ("Aktuellen Nutzer lesen", "Get current user"),
                    (
                        "Liest den Nutzer hinter dem API-Key und den Workspace.",
                        "Reads the user owning the API key and their workspace.",
                    ),
                    &[],
                    &[],
                    &[READ],
                    false,
                ),
                t(
                    "LINEAR_LIST_TEAMS",
                    ("Teams auflisten", "List teams"),
                    (
                        "Listet die zugänglichen Teams mit ID und Schlüssel.",
                        "Lists accessible teams with ID and key.",
                    ),
                    &with_page(
                        100,
                        250,
                        vec![("include_archived", flag("Archivierte Teams einschließen", false))],
                    ),
                    &[],
                    &[READ],
                    false,
                ),
                t(
                    "LINEAR_GET_TEAM",
                    ("Team lesen", "Get team"),
                    (
                        "Liest ein Team mit Standardstatus und Schätzskala.",
                        "Reads one team with its default status and estimation scale.",
                    ),
                    &[("team_id", s("Team-UUID (aus LINEAR_LIST_TEAMS)"))],
                    &["team_id"],
                    &[READ],
                    false,
                ),
                t(
                    "LINEAR_LIST_USERS",
                    ("Nutzer auflisten", "List users"),
                    (
                        "Listet Workspace-Mitglieder, optional nach E-Mail oder Name.",
                        "Lists workspace members, optionally by email or name.",
                    ),
                    &with_page(
                        100,
                        250,
                        vec![
                            ("email", s("Exakte E-Mail, case-insensitiv")),
                            ("name_contains", s("Teil des Namens, case-insensitiv")),
                            ("include_disabled", flag("Deaktivierte Nutzer einschließen", false)),
                        ],
                    ),
                    &[],
                    &[READ],
                    false,
                ),
                t(
                    "LINEAR_LIST_WORKFLOW_STATES",
                    ("Status auflisten", "List workflow states"),
                    (
                        "Listet die Status (Backlog, In Arbeit, Fertig) eines Teams.",
                        "Lists the statuses (Backlog, In Progress, Done) of a team.",
                    ),
                    &with_page(
                        100,
                        250,
                        vec![("team_id", s("Nur dieses Team; leer = alle Teams"))],
                    ),
                    &[],
                    &[READ],
                    false,
                ),
                t(
                    "LINEAR_LIST_CYCLES",
                    ("Zyklen auflisten", "List cycles"),
                    (
                        "Listet Zyklen (Sprints), optional auf aktuellen/ nächsten beschränkt.",
                        "Lists cycles (sprints), optionally only current, next or previous.",
                    ),
                    &with_page(
                        50,
                        250,
                        vec![
                            ("team_id", s("Nur dieses Team; leer = alle Teams")),
                            ("which", choice("isActive, isNext oder isPrevious", &["isActive", "isNext", "isPrevious"])),
                        ],
                    ),
                    &[],
                    &[READ],
                    false,
                ),
                t(
                    "LINEAR_LIST_ISSUE_LABELS",
                    ("Issue-Labels auflisten", "List issue labels"),
                    (
                        "Listet Labels eines Teams plus die Workspace-Labels.",
                        "Lists a team's labels plus the workspace labels.",
                    ),
                    &with_page(
                        100,
                        250,
                        vec![
                            ("team_id", s("Nur dieses Team; leer = alle Labels")),
                            ("name_contains", s("Teil des Label-Namens")),
                        ],
                    ),
                    &[],
                    &[READ],
                    false,
                ),
                t(
                    "LINEAR_GET_ISSUE",
                    ("Issue lesen", "Get issue"),
                    (
                        "Liest ein Issue per UUID oder Kennung wie ENG-123.",
                        "Reads one issue by UUID or identifier such as ENG-123.",
                    ),
                    &[("issue_id", s("Issue-UUID oder Kennung, z. B. ENG-123"))],
                    &["issue_id"],
                    &[READ],
                    false,
                ),
                t(
                    "LINEAR_LIST_ISSUES",
                    ("Issues auflisten", "List issues"),
                    (
                        "Listet Issues mit strukturierten Filtern (Team, Projekt, Status …).",
                        "Lists issues with structured filters (team, project, status, …).",
                    ),
                    &with_page(
                        50,
                        250,
                        vec![
                            ("team_id", s("Nur Issues dieses Teams")),
                            ("project_id", s("Nur Issues dieses Projekts")),
                            ("assignee_id", s("Nur Issues dieses Nutzers")),
                            ("state_id", s("Nur Issues in diesem Workflow-Status")),
                            ("state_type", choice("Nur Issues mit diesem Statustyp", &["triage", "backlog", "unstarted", "started", "completed", "canceled"])),
                            ("label_id", s("Nur Issues mit diesem Label")),
                            ("cycle_id", s("Nur Issues in diesem Zyklus")),
                            ("priority", priority_prop()),
                            ("updated_after", s("Nur Issues, die nach diesem ISO-Zeitpunkt aktualisiert wurden")),
                            ("include_archived", flag("Archivierte Issues einschließen", false)),
                            ("order_by", choice("Sortierung", &["createdAt", "updatedAt"])),
                        ],
                    ),
                    &[],
                    &[READ],
                    false,
                ),
                t(
                    "LINEAR_SEARCH_ISSUES",
                    ("Issues durchsuchen", "Search issues"),
                    (
                        "Volltextsuche über Titel und Beschreibungen, beste Treffer zuerst.",
                        "Full-text search over titles and descriptions, best matches first.",
                    ),
                    &with_page(
                        25,
                        100,
                        vec![
                            ("term", s("Suchbegriff, z. B. \"login timeout safari\"")),
                            ("team_id", s("Nur Issues dieses Teams")),
                            ("include_comments", flag("Auch Kommentare durchsuchen", false)),
                            ("include_archived", flag("Archivierte Issues einschließen", false)),
                        ],
                    ),
                    &["term"],
                    &[READ],
                    false,
                ),
                t(
                    "LINEAR_GET_COMMENT",
                    ("Kommentar lesen", "Get comment"),
                    ("Liest einen Kommentar per UUID.", "Reads one comment by UUID."),
                    &[("comment_id", s("Kommentar-UUID"))],
                    &["comment_id"],
                    &[READ],
                    false,
                ),
                t(
                    "LINEAR_LIST_COMMENTS",
                    ("Kommentare auflisten", "List comments"),
                    (
                        "Listet Kommentare, meist die eines Issues.",
                        "Lists comments, normally the comments of one issue.",
                    ),
                    &with_page(
                        50,
                        250,
                        vec![("issue_id", s("Nur Kommentare dieses Issues; leer = alle"))],
                    ),
                    &[],
                    &[READ],
                    false,
                ),
                t(
                    "LINEAR_GET_PROJECT",
                    ("Projekt lesen", "Get project"),
                    (
                        "Liest ein Projekt mit Teams und Meilensteinen.",
                        "Reads one project with its teams and milestones.",
                    ),
                    &[("project_id", s("Projekt-UUID"))],
                    &["project_id"],
                    &[READ],
                    false,
                ),
                t(
                    "LINEAR_LIST_PROJECTS",
                    ("Projekte auflisten", "List projects"),
                    (
                        "Listet Projekte, optional nach Team, Statustyp oder Name.",
                        "Lists projects, optionally by team, status type or name.",
                    ),
                    &with_page(
                        50,
                        250,
                        vec![
                            ("team_id", s("Nur Projekte mit Zugang zu diesem Team")),
                            ("status_type", choice("Nur Projekte mit diesem Statustyp", &["backlog", "planned", "started", "paused", "completed", "canceled"])),
                            ("name_contains", s("Teil des Projektnamens")),
                            ("include_archived", flag("Archivierte Projekte einschließen", false)),
                        ],
                    ),
                    &[],
                    &[READ],
                    false,
                ),
                t(
                    "LINEAR_GET_PROJECT_STATUS_UPDATE",
                    ("Projekt-Update lesen", "Get project status update"),
                    (
                        "Liest ein Status-Update eines Projekts.",
                        "Reads one project status update.",
                    ),
                    &[("status_update_id", s("Status-Update-UUID"))],
                    &["status_update_id"],
                    &[READ],
                    false,
                ),
                t(
                    "LINEAR_LIST_PROJECT_STATUS_UPDATES",
                    ("Projekt-Updates auflisten", "List project status updates"),
                    (
                        "Listet Status-Updates, optional eines Projekts.",
                        "Lists status updates, optionally of one project.",
                    ),
                    &with_page(
                        25,
                        250,
                        vec![
                            ("project_id", s("Nur Updates dieses Projekts")),
                            ("include_archived", flag("Archivierte Updates einschließen", false)),
                        ],
                    ),
                    &[],
                    &[READ],
                    false,
                ),
                t(
                    "LINEAR_GET_ATTACHMENT",
                    ("Anhang lesen", "Get attachment"),
                    (
                        "Liest einen Anhang (Link oder Datei) eines Issues.",
                        "Reads one attachment (link or file) of an issue.",
                    ),
                    &[("attachment_id", s("Anhang-UUID"))],
                    &["attachment_id"],
                    &[READ],
                    false,
                ),
                t(
                    "LINEAR_CREATE_ISSUE",
                    ("Issue anlegen", "Create issue"),
                    (
                        "Legt ein Issue in einem Team an; braucht --confirm.",
                        "Creates an issue in a team; requires --confirm.",
                    ),
                    &[
                        ("team_id", s("Team-UUID, das das Issue besitzt")),
                        ("title", s("Titel des Issues")),
                        ("description", s("Beschreibung in Markdown")),
                        ("assignee_id", s("Nutzer-UUID des Assignees")),
                        ("state_id", s("Workflow-Status-UUID des Teams")),
                        ("priority", priority_prop()),
                        ("label_ids", str_list("Label-UUIDs")),
                        ("project_id", s("Projekt-UUID")),
                        ("project_milestone_id", s("Meilenstein-UUID des Projekts")),
                        ("cycle_id", s("Zyklus-UUID des Teams")),
                        ("parent_id", s("UUID oder Kennung des übergeordneten Issues")),
                        ("due_date", date("Fälligkeitsdatum, YYYY-MM-DD")),
                        ("estimate", estimate_prop()),
                        ("template_id", s("Issue-Template-UUID des Teams")),
                    ],
                    &["team_id", "title"],
                    &[WRITE],
                    true,
                ),
                t(
                    "LINEAR_UPDATE_ISSUE",
                    ("Issue ändern", "Update issue"),
                    (
                        "Ändert nur die übergebenen Felder eines Issues; braucht --confirm.",
                        "Changes only the fields you pass on an issue; requires --confirm.",
                    ),
                    &[
                        ("issue_id", s("Issue-UUID oder Kennung, z. B. ENG-123")),
                        ("title", s("Neuer Titel")),
                        ("description", s("Neue Beschreibung in Markdown")),
                        ("assignee_id", s("UUID des neuen Assignees")),
                        ("state_id", s("Workflow-Status-UUID")),
                        ("priority", priority_prop()),
                        ("label_ids", str_list("Ersetzt alle Labels")),
                        ("add_label_ids", str_list("Labels hinzufügen, Rest behalten")),
                        ("remove_label_ids", str_list("Labels entfernen, Rest behalten")),
                        ("team_id", s("In dieses Team verschieben")),
                        ("project_id", s("Projekt-UUID")),
                        ("project_milestone_id", s("Meilenstein-UUID")),
                        ("cycle_id", s("Zyklus-UUID")),
                        ("parent_id", s("UUID oder Kennung des neuen Eltern-Issues")),
                        ("due_date", date("Fälligkeitsdatum, YYYY-MM-DD")),
                        ("estimate", estimate_prop()),
                        ("clear_fields", str_list("Felder leeren: assigneeId, description, dueDate, estimate, projectId, projectMilestoneId, cycleId, parentId, labelIds")),
                    ],
                    &["issue_id"],
                    &[WRITE],
                    true,
                ),
                t(
                    "LINEAR_ARCHIVE_ISSUE",
                    ("Issue archivieren", "Archive issue"),
                    (
                        "Archiviert ein Issue; braucht --confirm.",
                        "Archives an issue; requires --confirm.",
                    ),
                    &[("issue_id", s("Issue-UUID oder Kennung, z. B. ENG-123"))],
                    &["issue_id"],
                    &[WRITE],
                    true,
                ),
                t(
                    "LINEAR_UNARCHIVE_ISSUE",
                    ("Issue wiederherstellen", "Unarchive issue"),
                    (
                        "Holt ein archiviertes Issue zurück; braucht --confirm.",
                        "Restores an archived issue; requires --confirm.",
                    ),
                    &[("issue_id", s("Issue-UUID oder Kennung, z. B. ENG-123"))],
                    &["issue_id"],
                    &[WRITE],
                    true,
                ),
                t(
                    "LINEAR_DELETE_ISSUE",
                    ("Issue löschen", "Delete issue"),
                    (
                        "Verschiebt ein Issue in den Papierkorb; braucht --confirm.",
                        "Trashes an issue; requires --confirm.",
                    ),
                    &[("issue_id", s("Issue-UUID oder Kennung, z. B. ENG-123"))],
                    &["issue_id"],
                    &[WRITE],
                    true,
                ),
                t(
                    "LINEAR_ADD_ISSUE_LABEL",
                    ("Label hinzufügen", "Add issue label"),
                    (
                        "Hängt ein Label an ein Issue; braucht --confirm.",
                        "Adds a label to an issue; requires --confirm.",
                    ),
                    &[
                        ("issue_id", s("Issue-UUID oder Kennung, z. B. ENG-123")),
                        ("label_id", s("Label-UUID")),
                    ],
                    &["issue_id", "label_id"],
                    &[WRITE],
                    true,
                ),
                t(
                    "LINEAR_REMOVE_ISSUE_LABEL",
                    ("Label entfernen", "Remove issue label"),
                    (
                        "Entfernt ein Label von einem Issue; braucht --confirm.",
                        "Removes a label from an issue; requires --confirm.",
                    ),
                    &[
                        ("issue_id", s("Issue-UUID oder Kennung, z. B. ENG-123")),
                        ("label_id", s("Label-UUID")),
                    ],
                    &["issue_id", "label_id"],
                    &[WRITE],
                    true,
                ),
                t(
                    "LINEAR_CREATE_ISSUE_RELATION",
                    ("Issue verknüpfen", "Create issue relation"),
                    (
                        "Verknüpft zwei Issues (blocks, related, duplicate); braucht --confirm.",
                        "Relates two issues (blocks, related, duplicate); requires --confirm.",
                    ),
                    &[
                        ("issue_id", s("Issue-UUID oder Kennung, z. B. ENG-123")),
                        ("related_issue_id", s("UUID oder Kennung des zweiten Issues")),
                        ("type", choice("Art der Verknüpfung", &["blocks", "duplicate", "related"])),
                    ],
                    &["issue_id", "related_issue_id", "type"],
                    &[WRITE],
                    true,
                ),
                t(
                    "LINEAR_CREATE_COMMENT",
                    ("Kommentar schreiben", "Create comment"),
                    (
                        "Schreibt einen Kommentar auf ein Issue; braucht --confirm.",
                        "Posts a comment on an issue; requires --confirm.",
                    ),
                    &[
                        ("issue_id", s("Issue-UUID oder Kennung, z. B. ENG-123")),
                        ("body", s("Kommentartext in Markdown")),
                        ("parent_id", s("UUID des Kommentars, auf den geantwortet wird")),
                    ],
                    &["issue_id", "body"],
                    &[WRITE],
                    true,
                ),
                t(
                    "LINEAR_UPDATE_COMMENT",
                    ("Kommentar ändern", "Update comment"),
                    (
                        "Ersetzt den Text eines eigenen Kommentars; braucht --confirm.",
                        "Replaces the body of your own comment; requires --confirm.",
                    ),
                    &[
                        ("comment_id", s("Kommentar-UUID")),
                        ("body", s("Neuer Kommentartext in Markdown")),
                    ],
                    &["comment_id", "body"],
                    &[WRITE],
                    true,
                ),
                t(
                    "LINEAR_RESOLVE_COMMENT",
                    ("Kommentar-Thread auflösen", "Resolve comment thread"),
                    (
                        "Markiert einen Kommentar-Thread als gelöst; braucht --confirm.",
                        "Marks a comment thread as resolved; requires --confirm.",
                    ),
                    &[
                        ("comment_id", s("UUID des ersten Kommentars des Threads")),
                        ("resolving_comment_id", s("UUID der Antwort, die gelöst hat")),
                    ],
                    &["comment_id"],
                    &[WRITE],
                    true,
                ),
                t(
                    "LINEAR_UNRESOLVE_COMMENT",
                    ("Thread wieder öffnen", "Unresolve comment thread"),
                    (
                        "Öffnet einen gelösten Thread wieder; braucht --confirm.",
                        "Reopens a resolved thread; requires --confirm.",
                    ),
                    &[("comment_id", s("UUID des ersten Kommentars des Threads"))],
                    &["comment_id"],
                    &[WRITE],
                    true,
                ),
                t(
                    "LINEAR_CREATE_TEAM",
                    ("Team anlegen", "Create team"),
                    (
                        "Legt ein Team mit eigenem Issue-Schlüssel an; braucht --confirm.",
                        "Creates a team with its own issue key; requires --confirm.",
                    ),
                    &[
                        ("name", s("Teamname, z. B. \"Platform\"")),
                        ("key", s("Issue-Schlüssel, z. B. PLT; leer = von Linear abgeleitet")),
                        ("description", s("Beschreibung")),
                        ("icon", s("Icon")),
                        ("color", s("Hex-Farbe, z. B. #5E6AD2")),
                        ("timezone", s("IANA-Zeitzone, z. B. Europe/Berlin")),
                        ("cycles_enabled", choice("Zyklen aktivieren", &["true", "false"])),
                        ("triage_enabled", choice("Triage aktivieren", &["true", "false"])),
                    ],
                    &["name"],
                    &[ADMIN],
                    true,
                ),
                t(
                    "LINEAR_UPDATE_TEAM",
                    ("Team ändern", "Update team"),
                    (
                        "Ändert nur die übergebenen Team-Einstellungen; braucht --confirm.",
                        "Changes only the team settings you pass; requires --confirm.",
                    ),
                    &[
                        ("team_id", s("Team-UUID")),
                        ("name", s("Neuer Name")),
                        ("key", s("Neuer Issue-Schlüssel; benennt alle Kennungen um")),
                        ("description", s("Neue Beschreibung")),
                        ("icon", s("Icon")),
                        ("color", s("Hex-Farbe, z. B. #5E6AD2")),
                        ("timezone", s("IANA-Zeitzone, z. B. Europe/Berlin")),
                        ("cycles_enabled", choice("Zyklen aktivieren", &["true", "false"])),
                        ("triage_enabled", choice("Triage aktivieren", &["true", "false"])),
                    ],
                    &["team_id"],
                    &[ADMIN],
                    true,
                ),
                t(
                    "LINEAR_CREATE_PROJECT",
                    ("Projekt anlegen", "Create project"),
                    (
                        "Legt ein Projekt in mindestens einem Team an; braucht --confirm.",
                        "Creates a project in at least one team; requires --confirm.",
                    ),
                    &[
                        ("team_ids", str_list("Team-UUIDs des Projekts (mindestens eine)")),
                        ("name", s("Projektname")),
                        ("description", s("Beschreibung in Markdown")),
                        ("lead_id", s("UUID des Projektleiters")),
                        ("priority", priority_prop()),
                        ("status_id", s("Projektstatus-UUID")),
                        ("start_date", date("Startdatum, YYYY-MM-DD")),
                        ("target_date", date("Zieldatum, YYYY-MM-DD")),
                        ("icon", s("Icon")),
                        ("color", s("Hex-Farbe")),
                    ],
                    &["team_ids", "name"],
                    &[WRITE],
                    true,
                ),
                t(
                    "LINEAR_UPDATE_PROJECT",
                    ("Projekt ändern", "Update project"),
                    (
                        "Ändert nur die übergebenen Projektfelder; braucht --confirm.",
                        "Changes only the project fields you pass; requires --confirm.",
                    ),
                    &[
                        ("project_id", s("Projekt-UUID")),
                        ("name", s("Neuer Name")),
                        ("description", s("Neue Beschreibung in Markdown")),
                        ("lead_id", s("UUID des neuen Projektleiters")),
                        ("priority", priority_prop()),
                        ("status_id", s("Projektstatus-UUID")),
                        ("start_date", date("Startdatum, YYYY-MM-DD")),
                        ("target_date", date("Zieldatum, YYYY-MM-DD")),
                        ("icon", s("Icon")),
                        ("color", s("Hex-Farbe")),
                        ("team_ids", str_list("Team-UUIDs des Projekts (ersetzt alle)")),
                        ("clear_fields", str_list("Felder leeren: description, leadId, priority, statusId, startDate, targetDate, icon, color")),
                    ],
                    &["project_id"],
                    &[WRITE],
                    true,
                ),
                t(
                    "LINEAR_CREATE_PROJECT_STATUS_UPDATE",
                    ("Projekt-Update schreiben", "Create project status update"),
                    (
                        "Postet ein Status-Update zu einem Projekt; braucht --confirm.",
                        "Posts a status update on a project; requires --confirm.",
                    ),
                    &[
                        ("project_id", s("Projekt-UUID")),
                        ("body", s("Update-Text in Markdown")),
                        ("health", choice("onTrack, atRisk oder offTrack", &["onTrack", "atRisk", "offTrack"])),
                    ],
                    &["project_id", "body"],
                    &[WRITE],
                    true,
                ),
                t(
                    "LINEAR_CREATE_PROJECT_MILESTONE",
                    ("Meilenstein anlegen", "Create project milestone"),
                    (
                        "Legt einen Meilenstein in einem Projekt an; braucht --confirm.",
                        "Creates a milestone in a project; requires --confirm.",
                    ),
                    &[
                        ("project_id", s("Projekt-UUID")),
                        ("name", s("Name des Meilensteins, z. B. \"Beta\"")),
                        ("description", s("Beschreibung in Markdown")),
                        ("target_date", date("Zieldatum, YYYY-MM-DD")),
                    ],
                    &["project_id", "name"],
                    &[WRITE],
                    true,
                ),
            ],
        };
        Self {
            spec,
            api_base: api_base.trim_end_matches('/').to_string(),
        }
    }

    fn url(&self) -> String {
        format!("{}/graphql", self.api_base)
    }

    /// GraphQL document for `tool`.
    fn query(&self, tool: &str) -> Result<String> {
        Ok(match tool {
            "LINEAR_GET_VIEWER" => format!(
                "query LinearAtomicViewerGet {{ viewer {{ {USER_SELECTION} organization {{ id name urlKey }} }} }}"
            ),
            "LINEAR_LIST_TEAMS" => format!(
                "query LinearAtomicTeamsList($first: Int, $after: String, $includeArchived: Boolean) {{ teams(first: $first, after: $after, includeArchived: $includeArchived) {{ {PAGE} {{ {TEAM_SELECTION} }} }} }}"
            ),
            "LINEAR_GET_TEAM" => format!(
                "query LinearAtomicTeamGet($id: String!) {{ team(id: $id) {{ {TEAM_SELECTION} issueCount defaultIssueState {{ id name type }} }} }}"
            ),
            "LINEAR_LIST_USERS" => format!(
                "query LinearAtomicUsersList($filter: UserFilter, $first: Int, $after: String, $includeDisabled: Boolean) {{ users(filter: $filter, first: $first, after: $after, includeDisabled: $includeDisabled) {{ {PAGE} {{ {USER_SELECTION} }} }} }}"
            ),
            "LINEAR_LIST_WORKFLOW_STATES" => format!(
                "query LinearAtomicWorkflowStatesList($filter: WorkflowStateFilter, $first: Int, $after: String) {{ workflowStates(filter: $filter, first: $first, after: $after) {{ {PAGE} {{ {STATE_SELECTION} }} }} }}"
            ),
            "LINEAR_LIST_CYCLES" => format!(
                "query LinearAtomicCyclesList($filter: CycleFilter, $first: Int, $after: String) {{ cycles(filter: $filter, first: $first, after: $after) {{ {PAGE} {{ {CYCLE_SELECTION} }} }} }}"
            ),
            "LINEAR_LIST_ISSUE_LABELS" => format!(
                "query LinearAtomicIssueLabelsList($filter: IssueLabelFilter, $first: Int, $after: String) {{ issueLabels(filter: $filter, first: $first, after: $after) {{ {PAGE} {{ {LABEL_SELECTION} }} }} }}"
            ),
            "LINEAR_GET_ISSUE" => format!(
                "query LinearGetIssue($id: String!) {{ issue(id: $id) {{ {ISSUE_SELECTION} }} }}"
            ),
            "LINEAR_LIST_ISSUES" => format!(
                "query LinearAtomicIssuesList($filter: IssueFilter, $first: Int, $after: String, $includeArchived: Boolean, $orderBy: PaginationOrderBy) {{ issues(filter: $filter, first: $first, after: $after, includeArchived: $includeArchived, orderBy: $orderBy) {{ {PAGE} {{ {ISSUE_SELECTION} }} }} }}"
            ),
            "LINEAR_SEARCH_ISSUES" => format!(
                "query LinearSearchIssues($term: String!, $filter: IssueFilter, $first: Int, $after: String, $includeArchived: Boolean, $includeComments: Boolean) {{ searchIssues(term: $term, filter: $filter, first: $first, after: $after, includeArchived: $includeArchived, includeComments: $includeComments) {{ totalCount {PAGE} {{ {ISSUE_SELECTION} }} }} }}"
            ),
            "LINEAR_GET_COMMENT" => format!(
                "query LinearAtomicCommentGet($id: String!) {{ comment(id: $id) {{ {COMMENT_SELECTION} }} }}"
            ),
            "LINEAR_LIST_COMMENTS" => format!(
                "query LinearAtomicCommentsList($filter: CommentFilter, $first: Int, $after: String) {{ comments(filter: $filter, first: $first, after: $after) {{ {PAGE} {{ {COMMENT_SELECTION} }} }} }}"
            ),
            "LINEAR_GET_PROJECT" => format!(
                "query LinearAtomicProjectGet($id: String!) {{ project(id: $id) {{ {PROJECT_SELECTION} projectMilestones(first: 250) {{ {PAGE} {{ id name targetDate status }} }} }} }}"
            ),
            "LINEAR_LIST_PROJECTS" => format!(
                "query LinearAtomicProjectsList($filter: ProjectFilter, $first: Int, $after: String, $includeArchived: Boolean) {{ projects(filter: $filter, first: $first, after: $after, includeArchived: $includeArchived) {{ {PAGE} {{ {PROJECT_SELECTION} }} }} }}"
            ),
            "LINEAR_GET_PROJECT_STATUS_UPDATE" => format!(
                "query LinearAtomicProjectStatusUpdateGet($id: String!) {{ projectUpdate(id: $id) {{ {PROJECT_UPDATE_SELECTION} }} }}"
            ),
            "LINEAR_LIST_PROJECT_STATUS_UPDATES" => format!(
                "query LinearAtomicProjectStatusUpdatesList($filter: ProjectUpdateFilter, $first: Int, $after: String, $includeArchived: Boolean) {{ projectUpdates(filter: $filter, first: $first, after: $after, includeArchived: $includeArchived) {{ {PAGE} {{ {PROJECT_UPDATE_SELECTION} }} }} }}"
            ),
            "LINEAR_GET_ATTACHMENT" => format!(
                "query LinearAtomicAttachmentGet($id: String!) {{ attachment(id: $id) {{ {ATTACHMENT_SELECTION} }} }}"
            ),
            "LINEAR_CREATE_ISSUE" => format!(
                "mutation LinearAtomicIssueCreate($input: IssueCreateInput!) {{ issueCreate(input: $input) {{ success issue {{ {ISSUE_SELECTION} }} }} }}"
            ),
            "LINEAR_UPDATE_ISSUE" => format!(
                "mutation LinearAtomicIssueUpdate($id: String!, $input: IssueUpdateInput!) {{ issueUpdate(id: $id, input: $input) {{ success issue {{ {ISSUE_SELECTION} }} }} }}"
            ),
            "LINEAR_ARCHIVE_ISSUE" => format!(
                "mutation LinearAtomicIssueArchive($id: String!) {{ issueArchive(id: $id) {{ success entity {{ {ARCHIVED_ISSUE_SELECTION} }} }} }}"
            ),
            "LINEAR_UNARCHIVE_ISSUE" => format!(
                "mutation LinearAtomicIssueUnarchive($id: String!) {{ issueUnarchive(id: $id) {{ success entity {{ {ARCHIVED_ISSUE_SELECTION} }} }} }}"
            ),
            "LINEAR_DELETE_ISSUE" => format!(
                "mutation LinearIssueDelete($id: String!) {{ issueDelete(id: $id) {{ success entity {{ {ARCHIVED_ISSUE_SELECTION} }} }} }}"
            ),
            "LINEAR_ADD_ISSUE_LABEL" => format!(
                "mutation LinearIssueAddLabel($id: String!, $labelId: String!) {{ issueAddLabel(id: $id, labelId: $labelId) {{ success issue {{ {ISSUE_SELECTION} }} }} }}"
            ),
            "LINEAR_REMOVE_ISSUE_LABEL" => format!(
                "mutation LinearIssueRemoveLabel($id: String!, $labelId: String!) {{ issueRemoveLabel(id: $id, labelId: $labelId) {{ success issue {{ {ISSUE_SELECTION} }} }} }}"
            ),
            "LINEAR_CREATE_ISSUE_RELATION" => {
                "mutation LinearAtomicIssueRelationCreate($input: IssueRelationCreateInput!) { issueRelationCreate(input: $input) { success issueRelation { id type createdAt issue { id identifier title } relatedIssue { id identifier title } } } }".into()
            }
            "LINEAR_CREATE_COMMENT" => format!(
                "mutation LinearAtomicCommentCreate($input: CommentCreateInput!) {{ commentCreate(input: $input) {{ success comment {{ {COMMENT_SELECTION} }} }} }}"
            ),
            "LINEAR_UPDATE_COMMENT" => format!(
                "mutation LinearAtomicCommentUpdate($id: String!, $input: CommentUpdateInput!) {{ commentUpdate(id: $id, input: $input) {{ success comment {{ {COMMENT_SELECTION} }} }} }}"
            ),
            "LINEAR_RESOLVE_COMMENT" => format!(
                "mutation LinearAtomicCommentResolve($id: String!, $resolvingCommentId: String) {{ commentResolve(id: $id, resolvingCommentId: $resolvingCommentId) {{ success comment {{ {COMMENT_SELECTION} }} }} }}"
            ),
            "LINEAR_UNRESOLVE_COMMENT" => format!(
                "mutation LinearAtomicCommentUnresolve($id: String!) {{ commentUnresolve(id: $id) {{ success comment {{ {COMMENT_SELECTION} }} }} }}"
            ),
            "LINEAR_CREATE_TEAM" => format!(
                "mutation LinearAtomicTeamCreate($input: TeamCreateInput!) {{ teamCreate(input: $input) {{ success team {{ {TEAM_SELECTION} }} }} }}"
            ),
            "LINEAR_UPDATE_TEAM" => format!(
                "mutation LinearAtomicTeamUpdate($id: String!, $input: TeamUpdateInput!) {{ teamUpdate(id: $id, input: $input) {{ success team {{ {TEAM_SELECTION} }} }} }}"
            ),
            "LINEAR_CREATE_PROJECT" => format!(
                "mutation LinearAtomicProjectCreate($input: ProjectCreateInput!) {{ projectCreate(input: $input) {{ success project {{ {PROJECT_SELECTION} }} }} }}"
            ),
            "LINEAR_UPDATE_PROJECT" => format!(
                "mutation LinearAtomicProjectUpdate($id: String!, $input: ProjectUpdateInput!) {{ projectUpdate(id: $id, input: $input) {{ success project {{ {PROJECT_SELECTION} }} }} }}"
            ),
            "LINEAR_CREATE_PROJECT_STATUS_UPDATE" => format!(
                "mutation LinearProjectStatusUpdateCreate($input: ProjectUpdateCreateInput!) {{ projectUpdateCreate(input: $input) {{ success projectUpdate {{ {PROJECT_UPDATE_SELECTION} }} }} }}"
            ),
            "LINEAR_CREATE_PROJECT_MILESTONE" => format!(
                "mutation LinearAtomicProjectMilestoneCreate($input: ProjectMilestoneCreateInput!) {{ projectMilestoneCreate(input: $input) {{ success projectMilestone {{ {MILESTONE_SELECTION} }} }} }}"
            ),
            other => bail!("unknown Linear tool `{other}`"),
        })
    }

    /// GraphQL variables for `tool`.
    #[allow(clippy::too_many_lines)]
    fn variables(&self, tool: &str, args: &Value) -> Result<Value> {
        let first = |key: &str, fallback: u64, max: u64| json!(opt_u64(args, key, fallback, 1, max));
        let after = || opt_str(args, "cursor").map(String::from);
        let mut m = Map::new();
        let mut filter: Option<Value> = None;
        match tool {
            // The viewer query takes no arguments.
            "LINEAR_GET_VIEWER" => {}
            "LINEAR_LIST_TEAMS" => {
                m.insert("first".into(), first("limit", 100, 250));
                m.insert("includeArchived".into(), json!(flag_value(args, "include_archived")));
            }
            "LINEAR_GET_TEAM" => {
                m.insert("id".into(), json!(path_segment(args, "team_id")?));
            }
            "LINEAR_LIST_USERS" => {
                let mut pairs: Vec<(&str, Value)> = Vec::new();
                if let Some(v) = opt_str(args, "email") {
                    pairs.push(("email", json!({"eqIgnoreCase": v})));
                }
                if let Some(v) = opt_str(args, "name_contains") {
                    pairs.push(("name", json!({"containsIgnoreCase": v})));
                }
                filter = Some(obj(pairs));
                m.insert("includeDisabled".into(), json!(flag_value(args, "include_disabled")));
                m.insert("first".into(), first("limit", 100, 250));
            }
            "LINEAR_LIST_WORKFLOW_STATES" => {
                if non_blank(args, "team_id").is_some() {
                    filter = Some(obj(vec![("team", id_filter(path_segment(args, "team_id")?))]));
                }
                m.insert("first".into(), first("limit", 100, 250));
            }
            "LINEAR_LIST_CYCLES" => {
                let mut pairs = Vec::new();
                if non_blank(args, "team_id").is_some() {
                    pairs.push(("team", id_filter(path_segment(args, "team_id")?)));
                }
                match opt_str(args, "which") {
                    Some(w) if ["isActive", "isNext", "isPrevious"].contains(&w) => {
                        pairs.push((w, json!({"eq": true})));
                    }
                    Some(_) => bail!("`which` must be isActive, isNext or isPrevious"),
                    None => {}
                }
                filter = Some(obj(pairs));
                m.insert("first".into(), first("limit", 50, 250));
            }
            "LINEAR_LIST_ISSUE_LABELS" => {
                let mut pairs = Vec::new();
                if let Some(v) = opt_str(args, "name_contains") {
                    pairs.push(("name", json!({"containsIgnoreCase": v})));
                }
                if non_blank(args, "team_id").is_some() {
                    pairs.push((
                        "or",
                        json!([
                            {"team": id_filter(path_segment(args, "team_id")?)},
                            {"team": {"null": true}}
                        ]),
                    ));
                }
                filter = Some(obj(pairs));
                m.insert("first".into(), first("limit", 100, 250));
            }
            "LINEAR_GET_ISSUE" => {
                m.insert("id".into(), json!(issue_ref(args, "issue_id")?));
            }
            "LINEAR_LIST_ISSUES" => {
                let mut pairs: Vec<(&str, Value)> = Vec::new();
                for (key, field) in [
                    ("team_id", "team"),
                    ("project_id", "project"),
                    ("assignee_id", "assignee"),
                    ("cycle_id", "cycle"),
                ] {
                    if non_blank(args, key).is_some() {
                        pairs.push((field, id_filter(path_segment(args, key)?)));
                    }
                }
                let mut state_pairs: Vec<(&str, Value)> = Vec::new();
                if non_blank(args, "state_id").is_some() {
                    state_pairs.push(("id", json!({"eq": path_segment(args, "state_id")?})));
                }
                if let Some(t) = opt_str(args, "state_type") {
                    state_pairs.push(("type", json!({"eq": t})));
                }
                if !state_pairs.is_empty() {
                    pairs.push(("state", obj(state_pairs)));
                }
                if non_blank(args, "label_id").is_some() {
                    pairs.push((
                        "labels",
                        json!({"some": {"id": {"eq": path_segment(args, "label_id")?}}}),
                    ));
                }
                if args.get("priority").and_then(Value::as_u64).is_some() {
                    pairs.push((
                        "priority",
                        json!({"eq": checked_priority(args)?}),
                    ));
                }
                if let Some(v) = opt_str(args, "updated_after") {
                    if !v.contains('-') || v.len() < 8 {
                        bail!("`updated_after` must be an ISO 8601 date-time, for example 2026-09-01T00:00:00Z");
                    }
                    pairs.push(("updatedAt", json!({"gt": v})));
                }
                filter = Some(obj(pairs));
                m.insert("first".into(), first("limit", 50, 250));
                m.insert("includeArchived".into(), json!(flag_value(args, "include_archived")));
                if let Some(o) = opt_str(args, "order_by") {
                    m.insert("orderBy".into(), json!(o));
                }
            }
            "LINEAR_SEARCH_ISSUES" => {
                m.insert("term".into(), json!(req_str(args, "term")?.trim()));
                if non_blank(args, "team_id").is_some() {
                    filter = Some(obj(vec![(
                        "team",
                        id_filter(path_segment(args, "team_id")?),
                    )]));
                }
                m.insert("first".into(), first("limit", 25, 100));
                m.insert("includeComments".into(), json!(flag_value(args, "include_comments")));
                m.insert("includeArchived".into(), json!(flag_value(args, "include_archived")));
            }
            "LINEAR_GET_COMMENT" => {
                m.insert("id".into(), json!(path_segment(args, "comment_id")?));
            }
            "LINEAR_LIST_COMMENTS" => {
                if non_blank(args, "issue_id").is_some() {
                    filter = Some(obj(vec![(
                        "issue",
                        id_filter(&issue_ref(args, "issue_id")?),
                    )]));
                }
                m.insert("first".into(), first("limit", 50, 250));
            }
            "LINEAR_GET_PROJECT" => {
                m.insert("id".into(), json!(path_segment(args, "project_id")?));
            }
            "LINEAR_LIST_PROJECTS" => {
                let mut pairs = Vec::new();
                if non_blank(args, "team_id").is_some() {
                    pairs.push((
                        "accessibleTeams",
                        json!({"some": {"id": {"eq": path_segment(args, "team_id")?}}}),
                    ));
                }
                if let Some(t) = opt_str(args, "status_type") {
                    pairs.push(("status", json!({"type": {"eq": t}})));
                }
                if let Some(v) = opt_str(args, "name_contains") {
                    pairs.push(("name", json!({"containsIgnoreCase": v})));
                }
                filter = Some(obj(pairs));
                m.insert("first".into(), first("limit", 50, 250));
                m.insert("includeArchived".into(), json!(flag_value(args, "include_archived")));
            }
            "LINEAR_GET_PROJECT_STATUS_UPDATE" => {
                m.insert("id".into(), json!(path_segment(args, "status_update_id")?));
            }
            "LINEAR_LIST_PROJECT_STATUS_UPDATES" => {
                if non_blank(args, "project_id").is_some() {
                    filter = Some(obj(vec![(
                        "project",
                        id_filter(path_segment(args, "project_id")?),
                    )]));
                }
                m.insert("first".into(), first("limit", 25, 250));
                m.insert("includeArchived".into(), json!(flag_value(args, "include_archived")));
            }
            "LINEAR_GET_ATTACHMENT" => {
                m.insert("id".into(), json!(path_segment(args, "attachment_id")?));
            }
            "LINEAR_CREATE_ISSUE" => {
                let mut input = Map::new();
                input.insert(
                    "teamId".into(),
                    json!(req_str(args, "team_id")?.trim().to_string()),
                );
                put_str(&mut input, args, "title", "title");
                if input.get("title").is_none() {
                    bail!("missing argument `title`");
                }
                put_str(&mut input, args, "description", "description");
                put_str(&mut input, args, "assignee_id", "assigneeId");
                put_str(&mut input, args, "state_id", "stateId");
                put_priority(&mut input, args, "priority")?;
                put_labels(&mut input, args, "label_ids", "labelIds");
                put_str(&mut input, args, "project_id", "projectId");
                put_str(&mut input, args, "project_milestone_id", "projectMilestoneId");
                put_str(&mut input, args, "cycle_id", "cycleId");
                if non_blank(args, "parent_id").is_some() {
                    input.insert("parentId".into(), json!(issue_ref(args, "parent_id")?));
                }
                put_timeless_date(&mut input, args, "due_date", "dueDate")?;
                if let Some(e) = opt_u64_or_err(args, "estimate")? {
                    input.insert("estimate".into(), json!(e));
                }
                put_str(&mut input, args, "template_id", "templateId");
                m.insert("input".into(), Value::Object(input));
            }
            "LINEAR_UPDATE_ISSUE" => {
                let mut input = Map::new();
                put_str(&mut input, args, "title", "title");
                put_str(&mut input, args, "description", "description");
                put_str(&mut input, args, "assignee_id", "assigneeId");
                put_str(&mut input, args, "state_id", "stateId");
                put_priority(&mut input, args, "priority")?;
                let replace = str_vec(args, "label_ids");
                let added = str_vec(args, "add_label_ids");
                let removed = str_vec(args, "remove_label_ids");
                if !replace.is_empty() && (!added.is_empty() || !removed.is_empty()) {
                    bail!("use either `label_ids` (replace all) or `add_label_ids`/`remove_label_ids`, not both");
                }
                if !replace.is_empty() {
                    input.insert("labelIds".into(), json!(replace));
                }
                if !added.is_empty() {
                    input.insert("addedLabelIds".into(), json!(added));
                }
                if !removed.is_empty() {
                    input.insert("removedLabelIds".into(), json!(removed));
                }
                put_str(&mut input, args, "team_id", "teamId");
                put_str(&mut input, args, "project_id", "projectId");
                put_str(&mut input, args, "project_milestone_id", "projectMilestoneId");
                put_str(&mut input, args, "cycle_id", "cycleId");
                if non_blank(args, "parent_id").is_some() {
                    input.insert("parentId".into(), json!(issue_ref(args, "parent_id")?));
                }
                put_timeless_date(&mut input, args, "due_date", "dueDate")?;
                if let Some(e) = opt_u64_or_err(args, "estimate")? {
                    input.insert("estimate".into(), json!(e));
                }
                for field in str_vec(args, "clear_fields") {
                    if input.contains_key(&field) {
                        bail!("`{field}` is both set and cleared; pick one");
                    }
                    input.insert(field.clone(), if field == "labelIds" { json!([]) } else { Value::Null });
                }
                if input.is_empty() {
                    bail!("nothing to update: pass at least one field to change or clear");
                }
                m.insert("id".into(), json!(issue_ref(args, "issue_id")?));
                m.insert("input".into(), Value::Object(input));
            }
            "LINEAR_ARCHIVE_ISSUE" | "LINEAR_UNARCHIVE_ISSUE" | "LINEAR_DELETE_ISSUE" => {
                m.insert("id".into(), json!(issue_ref(args, "issue_id")?));
            }
            "LINEAR_ADD_ISSUE_LABEL" | "LINEAR_REMOVE_ISSUE_LABEL" => {
                m.insert("id".into(), json!(issue_ref(args, "issue_id")?));
                m.insert(
                    "labelId".into(),
                    json!(req_str(args, "label_id")?.trim().to_string()),
                );
            }
            "LINEAR_CREATE_ISSUE_RELATION" => {
                let id = issue_ref(args, "issue_id")?;
                let related = issue_ref(args, "related_issue_id")?;
                if id == related {
                    bail!("an issue cannot be related to itself");
                }
                let kind = req_str(args, "type")?;
                if !["blocks", "duplicate", "related"].contains(&kind) {
                    bail!("`type` must be blocks, duplicate or related");
                }
                m.insert(
                    "input".into(),
                    json!({"issueId": id, "relatedIssueId": related, "type": kind}),
                );
            }
            "LINEAR_CREATE_COMMENT" => {
                let mut input = Map::new();
                input.insert("issueId".into(), json!(issue_ref(args, "issue_id")?));
                input.insert("body".into(), json!(req_str(args, "body")?));
                if non_blank(args, "parent_id").is_some() {
                    input.insert("parentId".into(), json!(path_segment(args, "parent_id")?));
                }
                m.insert("input".into(), Value::Object(input));
            }
            "LINEAR_UPDATE_COMMENT" => {
                m.insert("id".into(), json!(path_segment(args, "comment_id")?));
                m.insert(
                    "input".into(),
                    json!({"body": req_str(args, "body")?}),
                );
            }
            "LINEAR_RESOLVE_COMMENT" => {
                m.insert("id".into(), json!(path_segment(args, "comment_id")?));
                if non_blank(args, "resolving_comment_id").is_some() {
                    m.insert(
                        "resolvingCommentId".into(),
                        json!(path_segment(args, "resolving_comment_id")?),
                    );
                }
            }
            "LINEAR_UNRESOLVE_COMMENT" => {
                m.insert("id".into(), json!(path_segment(args, "comment_id")?));
            }
            "LINEAR_CREATE_TEAM" => {
                let mut input = Map::new();
                input.insert("name".into(), json!(req_str(args, "name")?.trim()));
                for (key, field) in [
                    ("key", "key"),
                    ("description", "description"),
                    ("icon", "icon"),
                    ("color", "color"),
                    ("timezone", "timezone"),
                ] {
                    put_str(&mut input, args, key, field);
                }
                put_tristate(&mut input, args, "cycles_enabled", "cyclesEnabled")?;
                put_tristate(&mut input, args, "triage_enabled", "triageEnabled")?;
                m.insert("input".into(), Value::Object(input));
            }
            "LINEAR_UPDATE_TEAM" => {
                let mut input = Map::new();
                for (key, field) in [
                    ("name", "name"),
                    ("key", "key"),
                    ("description", "description"),
                    ("icon", "icon"),
                    ("color", "color"),
                    ("timezone", "timezone"),
                ] {
                    put_str(&mut input, args, key, field);
                }
                put_tristate(&mut input, args, "cycles_enabled", "cyclesEnabled")?;
                put_tristate(&mut input, args, "triage_enabled", "triageEnabled")?;
                if input.is_empty() {
                    bail!("nothing to update: pass at least one team setting");
                }
                m.insert("id".into(), json!(path_segment(args, "team_id")?));
                m.insert("input".into(), Value::Object(input));
            }
            "LINEAR_CREATE_PROJECT" => {
                let team_ids = str_vec(args, "team_ids");
                if team_ids.is_empty() {
                    bail!("`team_ids` needs at least one team UUID");
                }
                let mut input = Map::new();
                input.insert("teamIds".into(), json!(team_ids));
                input.insert("name".into(), json!(req_str(args, "name")?.trim()));
                put_str(&mut input, args, "description", "description");
                put_str(&mut input, args, "lead_id", "leadId");
                put_priority(&mut input, args, "priority")?;
                put_str(&mut input, args, "status_id", "statusId");
                put_timeless_date(&mut input, args, "start_date", "startDate")?;
                put_timeless_date(&mut input, args, "target_date", "targetDate")?;
                put_str(&mut input, args, "icon", "icon");
                put_str(&mut input, args, "color", "color");
                m.insert("input".into(), Value::Object(input));
            }
            "LINEAR_UPDATE_PROJECT" => {
                let mut input = Map::new();
                for (key, field) in [
                    ("name", "name"),
                    ("description", "description"),
                    ("lead_id", "leadId"),
                    ("status_id", "statusId"),
                    ("icon", "icon"),
                    ("color", "color"),
                ] {
                    put_str(&mut input, args, key, field);
                }
                put_priority(&mut input, args, "priority")?;
                put_timeless_date(&mut input, args, "start_date", "startDate")?;
                put_timeless_date(&mut input, args, "target_date", "targetDate")?;
                put_labels(&mut input, args, "team_ids", "teamIds");
                for field in str_vec(args, "clear_fields") {
                    if input.contains_key(&field) {
                        bail!("`{field}` is both set and cleared; pick one");
                    }
                    input.insert(field.clone(), Value::Null);
                }
                if input.is_empty() {
                    bail!("nothing to update: pass at least one project field to change or clear");
                }
                m.insert("id".into(), json!(path_segment(args, "project_id")?));
                m.insert("input".into(), Value::Object(input));
            }
            "LINEAR_CREATE_PROJECT_STATUS_UPDATE" => {
                let body = req_str(args, "body")?;
                if body.trim().is_empty() {
                    bail!("`body` cannot be empty");
                }
                let mut input = Map::new();
                input.insert(
                    "projectId".into(),
                    json!(req_str(args, "project_id")?.trim().to_string()),
                );
                input.insert("body".into(), json!(body));
                put_str(&mut input, args, "health", "health");
                m.insert("input".into(), Value::Object(input));
            }
            "LINEAR_CREATE_PROJECT_MILESTONE" => {
                let mut input = Map::new();
                input.insert(
                    "projectId".into(),
                    json!(req_str(args, "project_id")?.trim().to_string()),
                );
                input.insert("name".into(), json!(req_str(args, "name")?.trim()));
                put_str(&mut input, args, "description", "description");
                put_timeless_date(&mut input, args, "target_date", "targetDate")?;
                m.insert("input".into(), Value::Object(input));
            }
            other => bail!("unknown Linear tool `{other}`"),
        }
        if let Some(f) = filter {
            if !f.as_object().is_some_and(Map::is_empty) {
                m.insert("filter".into(), f);
            }
        }
        if let Some(c) = after() {
            m.insert("after".into(), json!(c));
        }
        Ok(Value::Object(m))
    }
}

fn flag_value(args: &Value, key: &str) -> bool {
    args.get(key).and_then(Value::as_bool).unwrap_or(false)
}

fn non_blank<'a>(args: &'a Value, key: &str) -> Option<&'a str> {
    args.get(key)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|v| !v.is_empty())
}

fn checked_priority(args: &Value) -> Result<u64> {
    let p = opt_u64_or_err(args, "priority")?.unwrap_or(0);
    if p > 4 {
        bail!("`priority` must be a whole number from 0 to 4 (0 none, 1 urgent, 2 high, 3 medium, 4 low).");
    }
    Ok(p)
}

fn put_tristate(m: &mut Map<String, Value>, args: &Value, key: &str, field: &str) -> Result<()> {
    match opt_str(args, key) {
        Some("true") => {
            m.insert(field.into(), json!(true));
        }
        Some("false") => {
            m.insert(field.into(), json!(false));
        }
        Some(_) => bail!("`{key}` must be \"true\" or \"false\""),
        None => {}
    }
    Ok(())
}

/// `data` of a GraphQL response, with provider errors turned into an `anyhow` error.
fn data(response: &HttpResponse) -> Result<Value> {
    let v = response.json()?;
    if let Some(errors) = v["errors"].as_array() {
        if !errors.is_empty() {
            let msg = errors
                .iter()
                .filter_map(|e| e["message"].as_str())
                .collect::<Vec<_>>()
                .join("; ");
            bail!("Linear API error: {}", if msg.is_empty() { "unknown" } else { &msg });
        }
    }
    match v.get("data") {
        Some(d) => Ok(d.clone()),
        None => Err(anyhow!("Linear returned an empty response")),
    }
}

impl ToolkitAdapter for Linear {
    fn spec(&self) -> &ToolkitSpec {
        &self.spec
    }

    fn build_request(&self, tool: &str, args: &Value) -> Result<HttpRequest> {
        let query = self.query(tool)?;
        let variables = self.variables(tool, args)?;
        Ok(HttpRequest::new("POST", self.url()).json(&json!({"query": query, "variables": variables})))
    }

    fn parse_response(&self, tool: &str, response: &HttpResponse) -> Result<Value> {
        let d = data(response)?;
        let one = |field: &str| -> Result<Value> {
            match d.get(field) {
                Some(v) if !v.is_null() => Ok(v.clone()),
                _ => Err(anyhow!("Linear returned no `{field}`")),
            }
        };
        let payload = |field: &str, entity: &str| -> Result<Value> {
            match d[field].get(entity) {
                Some(v) if v.is_object() => Ok(v.clone()),
                _ => Err(anyhow!("Linear returned no `{entity}`")),
            }
        };
        Ok(match tool {
            "LINEAR_GET_VIEWER" => flatten_viewer(&d["viewer"]),
            "LINEAR_LIST_TEAMS" => page(&d["teams"], flatten_team),
            "LINEAR_GET_TEAM" => flatten_team_details(&one("team")?),
            "LINEAR_LIST_USERS" => page(&d["users"], flatten_user),
            "LINEAR_LIST_WORKFLOW_STATES" => page(&d["workflowStates"], flatten_state),
            "LINEAR_LIST_CYCLES" => page(&d["cycles"], flatten_cycle),
            "LINEAR_LIST_ISSUE_LABELS" => page(&d["issueLabels"], flatten_label),
            "LINEAR_GET_ISSUE" => flatten_issue(&one("issue")?),
            "LINEAR_LIST_ISSUES" => page(&d["issues"], flatten_issue),
            "LINEAR_SEARCH_ISSUES" => {
                let mut out = page(&d["searchIssues"], flatten_issue);
                out["total_count"] = d["searchIssues"]["totalCount"].clone();
                out
            }
            "LINEAR_GET_COMMENT" => flatten_comment(&one("comment")?),
            "LINEAR_LIST_COMMENTS" => page(&d["comments"], flatten_comment),
            "LINEAR_GET_PROJECT" => flatten_project_with_milestones(&one("project")?),
            "LINEAR_LIST_PROJECTS" => page(&d["projects"], flatten_project),
            "LINEAR_GET_PROJECT_STATUS_UPDATE" => flatten_status_update(&one("projectUpdate")?),
            "LINEAR_LIST_PROJECT_STATUS_UPDATES" => page(&d["projectUpdates"], flatten_status_update),
            "LINEAR_GET_ATTACHMENT" => flatten_attachment(&one("attachment")?),
            "LINEAR_CREATE_ISSUE" | "LINEAR_UPDATE_ISSUE" | "LINEAR_ADD_ISSUE_LABEL"
            | "LINEAR_REMOVE_ISSUE_LABEL" => {
                let field = mutation_field(tool);
                require_success(&d, field)?;
                flatten_issue(&payload(field, "issue")?)
            }
            "LINEAR_ARCHIVE_ISSUE" | "LINEAR_UNARCHIVE_ISSUE" | "LINEAR_DELETE_ISSUE" => {
                let field = mutation_field(tool);
                require_success(&d, field)?;
                flatten_archived_issue(&d[field]["entity"])
            }
            "LINEAR_CREATE_ISSUE_RELATION" => {
                require_success(&d, "issueRelationCreate")?;
                flatten_relation(&payload("issueRelationCreate", "issueRelation")?)
            }
            "LINEAR_CREATE_COMMENT" | "LINEAR_UPDATE_COMMENT" | "LINEAR_RESOLVE_COMMENT"
            | "LINEAR_UNRESOLVE_COMMENT" => {
                let field = mutation_field(tool);
                require_success(&d, field)?;
                flatten_comment(&payload(field, "comment")?)
            }
            "LINEAR_CREATE_TEAM" | "LINEAR_UPDATE_TEAM" => {
                let field = mutation_field(tool);
                require_success(&d, field)?;
                flatten_team(&payload(field, "team")?)
            }
            "LINEAR_CREATE_PROJECT" | "LINEAR_UPDATE_PROJECT" => {
                let field = mutation_field(tool);
                require_success(&d, field)?;
                flatten_project(&payload(field, "project")?)
            }
            "LINEAR_CREATE_PROJECT_STATUS_UPDATE" => {
                require_success(&d, "projectUpdateCreate")?;
                flatten_status_update(&payload("projectUpdateCreate", "projectUpdate")?)
            }
            "LINEAR_CREATE_PROJECT_MILESTONE" => {
                require_success(&d, "projectMilestoneCreate")?;
                flatten_milestone(&payload("projectMilestoneCreate", "projectMilestone")?)
            }
            other => bail!("unknown Linear tool `{other}`"),
        })
    }
}

/// GraphQL mutation field carrying `{success, …}` for `tool`.
fn mutation_field(tool: &str) -> &str {
    match tool {
        "LINEAR_CREATE_ISSUE" => "issueCreate",
        "LINEAR_UPDATE_ISSUE" => "issueUpdate",
        "LINEAR_ADD_ISSUE_LABEL" => "issueAddLabel",
        "LINEAR_REMOVE_ISSUE_LABEL" => "issueRemoveLabel",
        "LINEAR_ARCHIVE_ISSUE" => "issueArchive",
        "LINEAR_UNARCHIVE_ISSUE" => "issueUnarchive",
        "LINEAR_DELETE_ISSUE" => "issueDelete",
        "LINEAR_CREATE_COMMENT" => "commentCreate",
        "LINEAR_UPDATE_COMMENT" => "commentUpdate",
        "LINEAR_RESOLVE_COMMENT" => "commentResolve",
        "LINEAR_UNRESOLVE_COMMENT" => "commentUnresolve",
        "LINEAR_CREATE_TEAM" => "teamCreate",
        "LINEAR_UPDATE_TEAM" => "teamUpdate",
        "LINEAR_CREATE_PROJECT" => "projectCreate",
        "LINEAR_UPDATE_PROJECT" => "projectUpdate",
        other => other,
    }
}

fn require_success(d: &Value, field: &str) -> Result<()> {
    if d[field]["success"] != json!(true) {
        bail!("Linear did not confirm the change ({field}).");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn requests_and_validation() {
        let l = Linear::with_api_base("https://l.test");
        let r = l
            .build_request(
                "LINEAR_GET_ISSUE",
                &json!({"issue_id": "eng-123"}),
            )
            .unwrap();
        assert_eq!(r.method, "POST");
        assert_eq!(r.url, "https://l.test/graphql");
        let body: Value = serde_json::from_slice(r.body.as_deref().unwrap()).unwrap();
        assert_eq!(body["variables"]["id"], "ENG-123");
        assert!(body["query"].as_str().unwrap().contains("LinearGetIssue"));

        assert!(l.build_request("LINEAR_GET_ISSUE", &json!({"issue_id": "../x"})).is_err());
        assert!(l.build_request("LINEAR_GET_ISSUE", &json!({"issue_id": "nope"})).is_err());
        assert!(l
            .build_request(
                "LINEAR_LIST_ISSUES",
                &json!({"priority": 9})
            )
            .is_err());
        assert!(l.build_request("LINEAR_UPDATE_ISSUE", &json!({"issue_id": "ENG-1"})).is_err());
        assert!(l
            .build_request(
                "LINEAR_UPDATE_ISSUE",
                &json!({"issue_id": "ENG-1", "title": "T", "clear_fields": ["title"]})
            )
            .is_err());
    }

    #[test]
    fn filters_and_mappers() {
        let l = Linear::with_api_base("https://l.test");
        let r = l
            .build_request(
                "LINEAR_LIST_ISSUES",
                &json!({"team_id": "11111111-1111-4111-8111-111111111111", "state_type": "started",
                        "label_id": "22222222-2222-4222-8222-222222222222", "order_by": "updatedAt"}),
            )
            .unwrap();
        let body: Value = serde_json::from_slice(r.body.as_deref().unwrap()).unwrap();
        assert_eq!(
            body["variables"]["filter"]["team"],
            json!({"id": {"eq": "11111111-1111-4111-8111-111111111111"}})
        );
        assert_eq!(body["variables"]["filter"]["state"], json!({"type": {"eq": "started"}}));
        assert_eq!(body["variables"]["orderBy"], "updatedAt");
        assert_eq!(body["variables"]["first"], 50);

        // An empty filter is not sent at all.
        let r = l.build_request("LINEAR_LIST_USERS", &json!({})).unwrap();
        let body: Value = serde_json::from_slice(r.body.as_deref().unwrap()).unwrap();
        assert!(body["variables"].get("filter").is_none());

        let issue = json!({"id": "i1", "identifier": "ENG-1", "number": 1, "title": "T", "url": "u",
            "labels": {"nodes": [{"id": "l1", "name": "Bug"}], "pageInfo": {"hasNextPage": false}}});
        assert_eq!(flatten_issue(&issue)["label_names"], "Bug");
        assert_eq!(flatten_issue(&issue)["labels_complete"], json!(true));
        assert_eq!(flatten_issue(&issue)["state_name"], Value::Null);
        assert_eq!(flatten_label(&json!({"id": "l1", "name": "Bug"}))["is_workspace_label"], json!(true));
        assert_eq!(flatten_viewer(&json!({"id": "u1", "organization": {"name": "Acme"}}))["organization_name"], "Acme");
    }
}