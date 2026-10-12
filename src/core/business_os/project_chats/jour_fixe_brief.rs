// Origin: CTOX
// License: AGPL-3.0-only
//! Native, owner-scoped preparation facts. Source diagnostics are not slide content.
#[cfg(test)]
#[path = "jour_fixe_repo_digest_brief_tests.rs"]
mod repo_digest_tests;
use super::super::workjet_jour_fixe_contract::Meeting;
use super::*;
use rusqlite::{params, OpenFlags, OptionalExtension};
use std::io::Read;
use std::time::Duration;

pub(in crate::business_os) const MAX_BYTES: usize = 12 * 1024;
const SCHEMA: &str = "
CREATE TABLE IF NOT EXISTS workjet_jour_fixe_briefs (
 operation_id TEXT PRIMARY KEY, owner_user_id TEXT NOT NULL, project_id TEXT NOT NULL,
 thread_id TEXT NOT NULL, brief_json TEXT NOT NULL
);
CREATE TABLE IF NOT EXISTS workjet_jour_fixe_brief_commands (
 command_id TEXT PRIMARY KEY, operation_id TEXT NOT NULL, thread_id TEXT NOT NULL
);";

fn section(value: anyhow::Result<Value>, budget: usize) -> Value {
    match value {
        Ok(Value::Null) => json!({"status":"missing_source","data":null}),
        Ok(mut data) => {
            let mut truncated = false;
            compact(&mut data, &mut truncated);
            while serde_json::to_vec(&data).map_or(usize::MAX, |v| v.len()) > budget {
                if !shrink(&mut data) {
                    data = Value::Null;
                    break;
                }
                truncated = true;
            }
            json!({"status":"ok","data":data,"truncated":truncated})
        }
        Err(error) => {
            let reason = match error.downcast_ref::<ureq::Error>() {
                Some(ureq::Error::Status(401, _)) => "unauthorized",
                Some(ureq::Error::Status(403 | 429, _)) => "denied_or_rate_limited",
                Some(ureq::Error::Status(404, _)) => "repository_not_visible",
                _ => "source_unavailable",
            };
            json!({"status":"unavailable","reason_code":reason,"data":null})
        }
    }
}
fn shrink(value: &mut Value) -> bool {
    match value {
        Value::Array(items) if items.len() > 1 => {
            items.pop();
            true
        }
        Value::Array(items) if items.len() == 1 => shrink(&mut items[0]),
        Value::Object(fields) if !fields.is_empty() => {
            let key = fields
                .iter()
                .max_by_key(|(_, v)| serde_json::to_vec(v).map_or(0, |b| b.len()))
                .map(|(k, _)| k.clone())
                .unwrap();
            if !shrink(fields.get_mut(&key).unwrap()) {
                fields.remove(&key);
            }
            true
        }
        Value::String(text) if text.chars().count() > 32 => {
            *text = text.chars().take(text.chars().count() / 2).collect();
            true
        }
        _ => false,
    }
}
fn compact(value: &mut Value, truncated: &mut bool) {
    match value {
        Value::String(text) if text.chars().count() > 700 => {
            *text = text.chars().take(700).collect();
            *truncated = true;
        }
        Value::Array(items) => {
            if items.len() > 16 {
                items.truncate(16);
                *truncated = true;
            }
            for item in items {
                compact(item, truncated);
            }
        }
        Value::Object(fields) => {
            for child in fields.values_mut() {
                compact(child, truncated);
            }
        }
        _ => {}
    }
}
fn repository(url: &str) -> anyhow::Result<String> {
    let url = url::Url::parse(url)?;
    ensure!(
        url.scheme() == "https"
            && url.host_str() == Some("github.com")
            && url.port().is_none()
            && url.username().is_empty()
            && url.password().is_none()
            && url.query().is_none()
            && url.fragment().is_none(),
        "unsupported repository source"
    );
    let path = url.path().trim_matches('/').trim_end_matches(".git");
    let parts: Vec<_> = path.split('/').collect();
    ensure!(
        parts.len() == 2
            && parts.iter().all(|p| !p.is_empty()
                && p.len() <= 100
                && *p != "."
                && *p != ".."
                && p.bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b"-_.".contains(&b))),
        "invalid repository path"
    );
    Ok(path.to_owned())
}
fn github_with(
    repo: Option<&str>,
    since: i64,
    mut fetch: impl FnMut(&str) -> anyhow::Result<Value>,
) -> Value {
    let Some(repo) = repo.filter(|r| !r.is_empty()) else {
        return json!({"status":"missing_source","reason_code":"repository_not_bound","data":null});
    };
    let Ok(repo) = repository(repo) else {
        return json!({"status":"unsupported_source","data":null});
    };
    let endpoints = [
        (
            "merged_prs",
            "pulls?state=closed&sort=updated&direction=desc&per_page=50",
        ),
        (
            "open_prs",
            "pulls?state=open&sort=updated&direction=desc&per_page=50",
        ),
        ("releases", "releases?per_page=20"),
        ("tags", "tags?per_page=20"),
    ];
    let mut sources = serde_json::Map::new();
    for (kind, endpoint) in endpoints {
        let result = fetch(&format!("/repos/{repo}/{endpoint}")).and_then(|raw| {
            let rows = raw.as_array().context("GitHub list response required")?;
            let mut facts = Vec::new();
            for row in rows {
                let after = |field: &str| row[field].as_str()
                    .and_then(|v| chrono::DateTime::parse_from_rfc3339(v).ok())
                    .is_some_and(|v| v.timestamp_millis() >= since);
                let fact = match kind {
                    "merged_prs" if after("merged_at") => json!({
                        "number":row["number"],"title":row["title"],"merged_at":row["merged_at"]}),
                    "merged_prs" => continue,
                    "open_prs" => json!({"number":row["number"],"title":row["title"],"updated_at":row["updated_at"]}),
                    "releases" if after("published_at") => json!({
                        "name":row["name"],"tag":row["tag_name"],"published_at":row["published_at"]}),
                    "releases" => continue,
                    // GitHub's tags endpoint has no publication time; never invent one.
                    "tags" => json!({"name":row["name"],"commit":row["commit"]["sha"]}),
                    _ => unreachable!(),
                };
                facts.push(fact);
            }
            Ok(json!({"items":facts,"list_limit_reached":rows.len() >= if kind.ends_with("prs") {50} else {20}}))
        });
        sources.insert(kind.into(), section(result, 650));
    }
    json!({"status":"collected","repository":repo,"since_ms":since,
        "tags_window":"latest_tags_without_publication_dates","data":sources})
}
fn github_with_operator_digest(
    api: Value,
    digest: anyhow::Result<Option<super::super::jour_fixe_repo_digest::RepoDigest>>,
    since: i64,
    now: i64,
) -> Value {
    // A successful API list, even an empty one, is authoritative. The outer
    // "collected" envelope alone does not establish a successful source.
    if api["status"] == "collected"
        && api["data"]
            .as_object()
            .is_some_and(|sources| sources.values().any(|source| source["status"] == "ok"))
    {
        return api;
    }
    if api["status"] == "unsupported_source" {
        return api;
    }
    let digest = match digest {
        Ok(Some(digest)) => digest,
        Ok(None) => return api,
        Err(_) => {
            return json!({"status":"missing_source","reason_code":"operator_digest_invalid","data":null})
        }
    };
    let collected = match super::super::jour_fixe_repo_digest::timestamp(&digest.collected_at) {
        Ok(at) => at,
        Err(_) => {
            return json!({"status":"missing_source","reason_code":"operator_digest_invalid","data":null})
        }
    };
    if now.saturating_sub(collected) >= super::super::jour_fixe_repo_digest::MAX_AGE_MS
        || collected > now
    {
        return json!({"status":"missing_source","reason_code":"operator_digest_stale","data":null});
    }
    let digest_since =
        super::super::jour_fixe_repo_digest::timestamp(&digest.since).expect("validated digest");
    let effective_since = since.max(digest_since);
    let after = |at: &str| {
        super::super::jour_fixe_repo_digest::timestamp(at).is_ok_and(|at| at >= effective_since)
    };
    let commits: Vec<_> = digest
        .commits
        .iter()
        .filter(|v| after(&v.date))
        .map(|v| json!({"sha":v.sha,"author":v.author,"date":v.date,"message":v.message}))
        .collect();
    let merged: Vec<_> = digest.pulls.iter().filter(|v| v.merged_at.as_deref().is_some_and(after)).map(|v|
        json!({"number":v.number,"title":v.title,"merged_at":v.merged_at,"author":v.author})).collect();
    let open: Vec<_> = digest.pulls.iter().filter(|v| v.state == "open").map(|v|
        json!({"number":v.number,"title":v.title,"updated_at":v.updated_at,"author":v.author})).collect();
    let issues: Vec<_> = digest.issues.iter().filter(|v| after(&v.updated_at)).map(|v|
        json!({"number":v.number,"title":v.title,"state":v.state,"updated_at":v.updated_at})).collect();
    let list = |items: Vec<Value>, budget| {
        section(
            Ok(json!({"items":items,"list_limit_reached":false})),
            budget,
        )
    };
    json!({"status":"collected","source":"operator_digest","repository":digest.repository,
        "collected_at":digest.collected_at,"since_ms":effective_since,"requested_since_ms":since,
        "data":{"commits":list(commits,700),"merged_prs":list(merged,650),"open_prs":list(open,650),
            "issues":list(issues,500),
            "releases":{"status":"missing_source","reason_code":"not_in_operator_digest","data":null},
            "tags":{"status":"missing_source","reason_code":"not_in_operator_digest","data":null}}})
}

fn github(root: &Path, repo: Option<&str>, since: i64) -> Value {
    if repo.is_none() || repo.is_some_and(|r| repository(r).is_err()) {
        return github_with(repo, since, |_| unreachable!());
    }
    // Runtime use only. No credential is stored in the brief, errors, prompt or receipt.
    let token = match crate::secrets::read_secret_value_optional(
        root,
        "credentials",
        "GITHUB_READ_TOKEN",
    ) {
        Ok(value) => value,
        Err(_) => {
            return json!({"status":"unavailable","reason_code":"credential_store_unavailable","data":null})
        }
    };
    let agent = ureq::AgentBuilder::new()
        .timeout(Duration::from_secs(4))
        .redirects(0)
        .build();
    github_with(repo, since, |path| {
        let mut request = agent
            .get(&format!("https://api.github.com{path}"))
            .set("User-Agent", "CTOX-JourFix-Preparation")
            .set("Accept", "application/vnd.github+json");
        if let Some(token) = token.as_ref() {
            request = request.set("Authorization", &format!("Bearer {token}"));
        }
        let response = request.call()?;
        let mut bytes = Vec::new();
        response
            .into_reader()
            .take(256 * 1024 + 1)
            .read_to_end(&mut bytes)?;
        ensure!(
            bytes.len() <= 256 * 1024,
            "GitHub source response exceeds budget"
        );
        Ok(serde_json::from_slice(&bytes)?)
    })
}
pub(super) fn prepare(
    root: &Path,
    operation: &str,
    meeting: &Meeting,
    project: &Value,
) -> anyhow::Result<()> {
    prepare_with(root, operation, meeting, project, |repo, since| {
        github(root, repo, since)
    })
}
fn prepare_with(
    root: &Path,
    operation: &str,
    meeting: &Meeting,
    project: &Value,
    fetch: impl FnOnce(Option<&str>, i64) -> Value,
) -> anyhow::Result<()> {
    let conn = open_store(root)?;
    conn.execute_batch(SCHEMA)?;
    let existing = read(
        &conn,
        operation,
        &meeting.owner_user_id,
        &meeting.project_id,
        &meeting.supervisor.workjet_thread_id,
    )?;
    if existing.is_some() {
        return Ok(());
    }
    let previous: Option<(String, i64)> = conn
        .query_row(
            "SELECT meeting_id,scheduled_at_ms FROM workjet_jour_fixe_meetings
         WHERE owner_user_id=?1 AND project_id=?2 AND scheduled_at_ms<?3
         AND json_extract(metadata_json,'$.state') IN ('review','confirmed')
         ORDER BY scheduled_at_ms DESC LIMIT 1",
            params![
                meeting.owner_user_id,
                meeting.project_id,
                meeting.scheduled_at_ms
            ],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .optional()?;
    let since = previous
        .as_ref()
        .map_or(meeting.scheduled_at_ms - 7 * 24 * 60 * 60 * 1000, |p| p.1);
    let core = Connection::open_with_flags(
        crate::paths::core_db(root),
        OpenFlags::SQLITE_OPEN_READ_ONLY,
    )
    .and_then(|conn| {
        conn.busy_timeout(Duration::from_secs(5))?;
        Ok(conn)
    });
    let goal = core
        .as_ref()
        .map_err(|_| anyhow::anyhow!("core unavailable"))
        .and_then(|core| {
            super::jour_fixe_confirmed_goal::current_goal_for_supervisor(
                core,
                &meeting.owner_user_id,
                &meeting.project_id,
                &meeting.supervisor.ctox_thread_key,
            )
        });
    let workers = core
        .as_ref()
        .map_err(|_| anyhow::anyhow!("core unavailable"))
        .and_then(|core| super::super::mcp_channel::workjet_outcomes_for_deck(core, meeting));
    let kpis = core
        .as_ref()
        .map_err(|_| anyhow::anyhow!("core unavailable"))
        .and_then(|core| {
            super::super::workjet_project_kpis::read_state(
                core,
                &meeting.project_id,
                &meeting.owner_user_id,
            )
            .and_then(|v| Ok(serde_json::to_value(v)?))
        });
    // Release SQLite readers before any external request.
    let goal = section(goal, 3000);
    let workers = section(workers, 1800);
    let kpis = section(kpis, 1800);
    let operator_digest = super::super::jour_fixe_repo_digest::load(&conn, &meeting.project_id);
    drop(core);
    drop(conn);
    // repository_url is the bound source; Workjet's project settings still write repo_url.
    // Until one field is canonical (Sync-v3 S4 note on #649) the older one is the fallback.
    let repo = project["repository_url"]
        .as_str()
        .filter(|url| !url.is_empty())
        .or_else(|| project["repo_url"].as_str().filter(|url| !url.is_empty()));
    let brief = json!({"schema":"ctox.workjet.jour_fixe_brief.v1",
        "meeting_id":meeting.id,"project_id":meeting.project_id,
        "previous_meeting":previous.map(|(id,at)| json!({"meeting_id":id,"scheduled_at_ms":at})),
        "window":{"since_ms":since,"until_ms":meeting.scheduled_at_ms},
        "confirmed_goal_and_progress":goal,"project_context":section(Ok(json!({"name":project["name"],"info":project["info"]})),1200),
        "worker_results":workers,"kpis":kpis,
        "github":github_with_operator_digest(fetch(repo,since),operator_digest,since,(super::super::store::now_ms() as i64))});
    let raw = serde_json::to_string(&brief)?;
    ensure!(
        raw.len() <= MAX_BYTES,
        "preparation brief exceeds native budget"
    );
    let conn = open_store(root)?;
    conn.execute(
        "INSERT INTO workjet_jour_fixe_briefs VALUES(?1,?2,?3,?4,?5)
        ON CONFLICT(operation_id) DO NOTHING",
        params![
            operation,
            meeting.owner_user_id,
            meeting.project_id,
            meeting.supervisor.workjet_thread_id,
            raw
        ],
    )?;
    read(
        &conn,
        operation,
        &meeting.owner_user_id,
        &meeting.project_id,
        &meeting.supervisor.workjet_thread_id,
    )?
    .context("brief persistence failed")?;
    Ok(())
}
fn read(
    conn: &Connection,
    operation: &str,
    owner: &str,
    project: &str,
    thread: &str,
) -> anyhow::Result<Option<String>> {
    let row: Option<(String,String,String,String)> = conn.query_row(
        "SELECT owner_user_id,project_id,thread_id,brief_json FROM workjet_jour_fixe_briefs WHERE operation_id=?1",
        [operation], |r| Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?))).optional()?;
    let Some((saved_owner, saved_project, saved_thread, raw)) = row else {
        return Ok(None);
    };
    ensure!(
        saved_owner == owner
            && saved_project == project
            && saved_thread == thread
            && raw.len() <= MAX_BYTES,
        "preparation brief scope conflicts"
    );
    Ok(Some(raw))
}
pub(in crate::business_os) fn attach(
    root: &Path,
    operation: &str,
    thread: &str,
    command: &str,
) -> anyhow::Result<()> {
    let conn = open_store(root)?;
    let exists: bool = conn.query_row(
        "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE name='workjet_jour_fixe_briefs')",
        [],
        |r| r.get(0),
    )?;
    if !exists {
        return Ok(());
    }
    let bound: Option<String> = conn
        .query_row(
            "SELECT thread_id FROM workjet_jour_fixe_briefs WHERE operation_id=?1",
            [operation],
            |r| r.get(0),
        )
        .optional()?;
    let Some(bound) = bound else {
        return Ok(());
    };
    ensure!(bound == thread, "preparation brief thread conflicts");
    conn.execute(
        "INSERT INTO workjet_jour_fixe_brief_commands VALUES(?1,?2,?3)",
        params![command, operation, thread],
    )?;
    Ok(())
}
pub(in crate::business_os) fn prompt(
    root: &Path,
    command_id: &str,
    command: &BusinessCommand,
) -> anyhow::Result<Option<String>> {
    if command.command_type != "business_os.chat.task"
        || command.payload["supervisor_turn"].is_null()
    {
        return Ok(None);
    }
    let conn = Connection::open_with_flags(
        super::super::store::business_os_store_path(root),
        OpenFlags::SQLITE_OPEN_READ_ONLY,
    )?;
    conn.busy_timeout(Duration::from_secs(5))?;
    let exists: bool = conn.query_row(
        "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE name='workjet_jour_fixe_brief_commands')",
        [],
        |r| r.get(0),
    )?;
    if !exists {
        return Ok(None);
    }
    let row: Option<(String, String, String)> = conn
        .query_row(
            "SELECT b.brief_json,c.operation_id,c.thread_id FROM workjet_jour_fixe_brief_commands c
         JOIN workjet_jour_fixe_briefs b ON b.operation_id=c.operation_id
         WHERE c.command_id=?1",
            [command_id],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )
        .optional()?;
    let Some((raw, operation, thread)) = row else {
        return Ok(None);
    };
    ensure!(
        command.command_type == "business_os.chat.task"
            && command.payload["thread_id"] == thread
            && command.payload["supervisor_turn"]["submit_command_id"] == operation
            && raw.len() <= MAX_BYTES,
        "native preparation command binding conflicts"
    );
    Ok(Some(format!("\nNative JourFix preparation brief JSON (verified facts only; source status/error diagnostics are for preparation, NEVER publish them on slides):\n{raw}\n")))
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn workjet_brief_github_uses_real_merge_times_and_classifies_missing_sources() {
        let since = chrono::DateTime::parse_from_rfc3339("2026-10-05T10:00:00Z")
            .unwrap()
            .timestamp_millis();
        let result = github_with(
            Some("https://github.com/metric-space-ai/ctox"),
            since,
            |path| {
                if path.contains("state=closed") {
                    Ok(json!([
                {"number":1,"title":"Merged","merged_at":"2026-10-06T10:00:00Z"},
                {"number":2,"title":"Old","merged_at":"2026-10-01T10:00:00Z"},
                {"number":3,"title":"Closed","merged_at":null}]))
                } else if path.contains("state=open") {
                    Err(anyhow::anyhow!("provider down"))
                } else {
                    Ok(json!([]))
                }
            },
        );
        assert_eq!(
            result["data"]["merged_prs"]["data"]["items"]
                .as_array()
                .unwrap()
                .len(),
            1
        );
        assert_eq!(result["data"]["open_prs"]["status"], "unavailable");
        assert_eq!(
            github_with(None, since, |_| panic!("must not fetch"))["status"],
            "missing_source"
        );
        for url in [
            "http://github.com/a/b",
            "https://github.com@evil.test/a/b",
            "https://github.com/a/b/issues",
            "https://github.com/a/b?token=x",
        ] {
            assert!(repository(url).is_err(), "{url}");
        }
    }
    #[test]
    fn workjet_brief_sections_are_complete_bounded_json() {
        let value = section(Ok(json!({"items":vec!["Ä".repeat(4096);100]})), 2500);
        assert!(serde_json::to_vec(&value).unwrap().len() < 2600);
        assert_eq!(value["truncated"], true);
        assert!(!value["data"]["items"].as_array().unwrap().is_empty());
        assert!(serde_json::from_slice::<Value>(&serde_json::to_vec(&value).unwrap()).is_ok());
    }

    #[test]
    fn workjet_brief_reads_repository_url_and_falls_back_to_repo_url() -> anyhow::Result<()> {
        let root = tempfile::tempdir()?;
        let meeting = Meeting {
            id: "meeting".into(),
            project_id: "project".into(),
            owner_user_id: "owner".into(),
            supervisor: super::super::super::workjet_jour_fixe_contract::SupervisorRef {
                workjet_thread_id: "thread".into(),
                ctox_thread_key: "native-thread".into(),
            },
            scheduled_at_ms: 1791889200000,
            prepare_at_ms: 1791882000000,
            timezone: "Europe/Berlin".into(),
            state: super::super::super::workjet_jour_fixe_contract::MeetingState::Planned,
            revision: 0,
            deck_revision: 0,
            previous_goal: None,
            slides: vec![],
            comments: vec![],
            transcript: vec![],
            todos: None,
            error: None,
        };
        let conn = open_store(root.path())?;
        conn.execute_batch(super::super::jour_fixe_preparation::SCHEMA)?;
        drop(conn);
        for (operation, project, expected) in [
            (
                "legacy",
                json!({"repo_url":"https://github.com/acme/legacy"}),
                Some("https://github.com/acme/legacy"),
            ),
            (
                "bound",
                json!({"repository_url":"https://github.com/acme/bound","repo_url":"https://github.com/acme/legacy"}),
                Some("https://github.com/acme/bound"),
            ),
            ("empty", json!({"repository_url":"","repo_url":""}), None),
        ] {
            prepare_with(root.path(), operation, &meeting, &project, |repo, since| {
                assert_eq!(repo, expected);
                github_with(None, since, |_| panic!("no external call"))
            })?;
        }
        Ok(())
    }

    #[test]
    fn workjet_brief_snapshot_replays_and_cannot_cross_owner_or_thread() -> anyhow::Result<()> {
        let root = tempfile::tempdir()?;
        let meeting = Meeting {
            id: "meeting".into(),
            project_id: "project".into(),
            owner_user_id: "owner".into(),
            supervisor: super::super::super::workjet_jour_fixe_contract::SupervisorRef {
                workjet_thread_id: "thread".into(),
                ctox_thread_key: "native-thread".into(),
            },
            scheduled_at_ms: 1791889200000,
            prepare_at_ms: 1791882000000,
            timezone: "Europe/Berlin".into(),
            state: super::super::super::workjet_jour_fixe_contract::MeetingState::Planned,
            revision: 0,
            deck_revision: 0,
            previous_goal: None,
            slides: vec![],
            comments: vec![],
            transcript: vec![],
            todos: None,
            error: None,
        };
        let conn = open_store(root.path())?;
        conn.execute_batch(super::super::jour_fixe_preparation::SCHEMA)?;
        conn.execute("INSERT INTO workjet_jour_fixe_meetings VALUES('previous','project','owner',?1,?2,NULL)",
            params![meeting.scheduled_at_ms - 604800000, r#"{"state":"confirmed"}"#])?;
        drop(conn);
        prepare_with(
            root.path(),
            "prepare",
            &meeting,
            &json!({}),
            |repo, since| {
                assert!(repo.is_none());
                assert_eq!(since, meeting.scheduled_at_ms - 604800000);
                github_with(repo, since, |_| panic!("no external call"))
            },
        )?;
        prepare_with(root.path(), "prepare", &meeting, &json!({}), |_, _| {
            panic!("replay must not refetch")
        })?;
        let conn = open_store(root.path())?;
        let raw = read(&conn, "prepare", "owner", "project", "thread")?.unwrap();
        assert!(raw.len() <= MAX_BYTES);
        let brief: Value = serde_json::from_str(&raw)?;
        assert_eq!(brief["previous_meeting"]["meeting_id"], "previous");
        assert_eq!(brief["github"]["status"], "missing_source");
        assert!(read(&conn, "prepare", "foreign", "project", "thread").is_err());
        assert!(read(&conn, "prepare", "owner", "project", "other").is_err());
        drop(conn);
        assert!(attach(root.path(), "prepare", "other", "ai").is_err());
        attach(root.path(), "prepare", "thread", "ai")?;
        let mut command: BusinessCommand = serde_json::from_value(json!({
            "id":"ai","module":"ctox","type":"business_os.chat.task",
            "payload":{"thread_id":"thread","supervisor_turn":{"submit_command_id":"prepare"}}
        }))?;
        assert!(prompt(root.path(), "ai", &command)?.unwrap().contains(&raw));
        command.payload["thread_id"] = json!("other");
        assert!(prompt(root.path(), "ai", &command).is_err());
        command.payload["thread_id"] = json!("thread");
        assert!(prompt(root.path(), "forged", &command)?.is_none());
        Ok(())
    }
}
