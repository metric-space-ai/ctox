// Origin: CTOX
// License: Apache-2.0
//
// News app (modules/news): native side of its four commands.
//
// - `news.topic.schedule` (control): owns the topic's `ctox schedule` entry.
// - `news.source.auth_assist` (control): hands the X/Reddit sign-in to the
//   owner's CTOX browser profile, the same path Outbound uses for LinkedIn/XING.
// - `news.issue.generate` (queue): the harness researches the sources with the
//   `news-aggregation` skill and replies with a `ctox.news.issue.v1` object;
//   this file validates it and writes news_issues/news_posts.
// - `news.source.adapter` (queue): a `universal-scraping` run, or for X and
//   Reddit the account connection (sign in, or register with the CTOX
//   mailbox) answered as `ctox.news.source.v1`; projected onto the source row.
//
// Scheduled runs re-enter through `emit_scheduled_issue`, called from the
// schedule emitter before the generic cron path, so every issue — manual or
// scheduled — is the same typed business command with the same writeback.

use super::store::{
    accept_rxdb_business_command_with_origin, active_domain_recovery_session, extract_json_object,
    find_rxdb_collection_records_by_string_field, first_string_field, load_rxdb_collection_record,
    now_ms, upsert_rxdb_collection_record, BusinessCommand, BusinessOsSession, CommandOrigin,
};
use super::store_projections::upsert_business_record;
use crate::mission::schedule::{self, ScheduleCalendar, ScheduleEnsureRequest, ScheduledTaskView};
use anyhow::{ensure, Context};
use rusqlite::Connection;
use serde::{Deserialize, Serialize};
use serde_json::{json, Map, Value};
use std::path::Path;

pub(super) const NEWS_MODULE: &str = "news";
pub(super) const NEWS_ISSUE_GENERATE: &str = "news.issue.generate";
pub(super) const NEWS_TOPIC_SCHEDULE: &str = "news.topic.schedule";
pub(super) const NEWS_SOURCE_ADAPTER: &str = "news.source.adapter";
pub(super) const NEWS_SOURCE_AUTH_ASSIST: &str = "news.source.auth_assist";
pub(super) const NEWS_ISSUE_CONTRACT: &str = "ctox.news.issue.v1";
const NEWS_SKILL: &str = "news-aggregation";
const SCHEDULE_PREFIX: &str = "news:";
const ROUTE_MARKER: &str = "ctox-news-schedule:";
pub(super) const NEWS_SOURCE_CONTRACT: &str = "ctox.news.source.v1";
const MAX_POSTS: usize = 40;
const MAX_TEXT: usize = 8_000;
const CATEGORIES: [&str; 4] = ["model", "tool", "research", "debate"];
const HOUR_MS: i64 = 3_600_000;
const DAY_MS: i64 = 24 * HOUR_MS;
/// Slack before the window start for feeds that stamp a story a few hours
/// before it actually went live.
const FRESHNESS_GRACE_MS: i64 = 6 * HOUR_MS;
/// Covered stories of the same topic are remembered this long.
const COVERED_LOOKBACK_MS: i64 = 30 * DAY_MS;
const MAX_COVERED_URLS: usize = 120;
const MAX_TREND_TITLE: usize = 60;
/// Per-run effort cap. A worker attempt is cut at 840 s; on Welsch
/// (15.10.2026) 24 sources took ~330k input tokens per attempt and all five
/// attempts timed out at the final check step.
const MAX_SOURCES_PER_RUN: usize = 12;
/// Source states that cannot deliver anything this run.
const UNUSABLE_SOURCE_STATES: [&str; 3] = ["failed", "needs_auth", "auth_requested"];

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
struct ScheduleRoute {
    topic_id: String,
    owner_user_id: String,
    cadence: String,
    #[serde(default)]
    deadline_date: String,
}

fn valid_record_id(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 220
        && value
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '_' | '-' | '.' | ':'))
}

fn schedule_name(topic_id: &str) -> String {
    format!("{SCHEDULE_PREFIX}{topic_id}")
}

fn topic_thread_key(topic_id: &str) -> String {
    format!("business-os/news/{topic_id}")
}

fn write_record(
    root: &Path,
    conn: &Connection,
    collection: &str,
    id: &str,
    now: i64,
    record: Value,
) -> anyhow::Result<()> {
    upsert_business_record(conn, collection, id, now, record.clone())?;
    upsert_rxdb_collection_record(root, collection, id, now, record)
}

fn patch_record(
    root: &Path,
    conn: &Connection,
    collection: &str,
    id: &str,
    now: i64,
    patch: Value,
) -> anyhow::Result<Option<Value>> {
    let Some(mut record) = load_rxdb_collection_record(root, collection, id)? else {
        return Ok(None);
    };
    if let (Some(target), Some(fields)) = (record.as_object_mut(), patch.as_object()) {
        for (key, value) in fields {
            target.insert(key.clone(), value.clone());
        }
        target.insert("updated_at_ms".to_string(), json!(now));
    }
    write_record(root, conn, collection, id, now, record.clone())?;
    Ok(Some(record))
}

/// Account sources have no API path: X and Reddit are read through a real
/// account in the owner's CTOX browser profile.
fn account_auth_source(kind: &str) -> Option<&'static str> {
    match kind {
        "x" => Some("x.com"),
        "reddit" => Some("reddit.com"),
        _ => None,
    }
}

fn session_user_id(session: &BusinessOsSession) -> anyhow::Result<String> {
    session
        .user
        .as_ref()
        .map(|user| user.id.trim().to_string())
        .filter(|id| !id.is_empty())
        .context("news command requires an authenticated user")
}

// --- control commands -------------------------------------------------------

pub(super) fn handle_news_control_command(
    root: &Path,
    session: &BusinessOsSession,
    command: &BusinessCommand,
) -> anyhow::Result<Value> {
    ensure!(
        command.module == NEWS_MODULE,
        "news control commands must originate from the news module"
    );
    match command.command_type.as_str() {
        NEWS_TOPIC_SCHEDULE => handle_topic_schedule(root, session, command),
        NEWS_SOURCE_AUTH_ASSIST => handle_source_auth_assist(root, session, command),
        other => anyhow::bail!("unsupported news control command `{other}`"),
    }
}

fn handle_topic_schedule(
    root: &Path,
    session: &BusinessOsSession,
    command: &BusinessCommand,
) -> anyhow::Result<Value> {
    let topic_id = first_string_field(&command.payload, &["topic_id"])
        .or_else(|| command.record_id.clone())
        .unwrap_or_default();
    ensure!(
        valid_record_id(&topic_id),
        "news.topic.schedule requires a valid topic_id"
    );
    let deleted = command.payload.get("deleted").and_then(Value::as_bool) == Some(true);
    let cron_expr = first_string_field(&command.payload, &["cron_expr"]).unwrap_or_default();
    let enabled = !deleted
        && command.payload.get("enabled").and_then(Value::as_bool) != Some(false)
        && !cron_expr.trim().is_empty();
    let name = schedule_name(&topic_id);
    let thread_key = topic_thread_key(&topic_id);
    let existing = schedule::list_tasks(root)?
        .into_iter()
        .find(|task| task.name == name && task.thread_key == thread_key);

    if !enabled {
        if let Some(task) = existing.as_ref() {
            if deleted {
                schedule::remove_task(root, &task.task_id)?;
            } else if task.enabled {
                schedule::set_task_enabled(root, &task.task_id, false)?;
            }
        }
        if !deleted {
            store_schedule_task_id(
                root,
                &topic_id,
                existing.as_ref().map(|t| t.task_id.as_str()),
            )?;
        }
        return Ok(json!({
            "ok": true,
            "topic_id": topic_id,
            "enabled": false,
            "removed": deleted,
            "schedule_task_id": existing.map(|task| task.task_id).unwrap_or_default(),
        }));
    }

    let timezone = first_string_field(&command.payload, &["timezone"])
        .filter(|value| value.parse::<chrono_tz::Tz>().is_ok())
        .unwrap_or_else(|| "Europe/Berlin".to_string());
    let route = ScheduleRoute {
        topic_id: topic_id.clone(),
        owner_user_id: session_user_id(session)?,
        cadence: first_string_field(&command.payload, &["cadence"]).unwrap_or_default(),
        deadline_date: first_string_field(&command.payload, &["deadline_date"])
            .or_else(|| {
                command
                    .payload
                    .pointer("/record_snapshot/topic/deadline_date")
                    .and_then(Value::as_str)
                    .map(str::to_string)
            })
            .unwrap_or_default(),
    };
    let task = schedule::ensure_task_with_calendar(
        root,
        ScheduleEnsureRequest {
            name,
            cron_expr: cron_expr.trim().to_string(),
            prompt: format!("{ROUTE_MARKER}{}", serde_json::to_string(&route)?),
            thread_key,
            skill: None,
        },
        ScheduleCalendar {
            timezone,
            lead_minutes: 0,
        },
    )?;
    let task = if task.enabled {
        task
    } else {
        schedule::set_task_enabled(root, &task.task_id, true)?
    };
    store_schedule_task_id(root, &topic_id, Some(&task.task_id))?;
    Ok(json!({
        "ok": true,
        "topic_id": topic_id,
        "enabled": true,
        "schedule_task_id": task.task_id,
        "next_run_at": task.next_run_at,
    }))
}

fn store_schedule_task_id(
    root: &Path,
    topic_id: &str,
    task_id: Option<&str>,
) -> anyhow::Result<()> {
    let conn = super::store::open_store(root)?;
    patch_record(
        root,
        &conn,
        "news_topics",
        topic_id,
        now_ms() as i64,
        json!({ "schedule_task_id": task_id.unwrap_or_default() }),
    )?;
    Ok(())
}

fn handle_source_auth_assist(
    root: &Path,
    session: &BusinessOsSession,
    command: &BusinessCommand,
) -> anyhow::Result<Value> {
    let source_id = first_string_field(&command.payload, &["source_id"])
        .or_else(|| command.record_id.clone())
        .unwrap_or_default();
    ensure!(
        valid_record_id(&source_id),
        "news.source.auth_assist requires a valid source_id"
    );
    let source = load_rxdb_collection_record(root, "news_sources", &source_id)?
        .context("news source not found")?;
    let auth_source = source
        .get("kind")
        .and_then(Value::as_str)
        .and_then(account_auth_source)
        .context("only X and Reddit sources sign in through the browser")?;
    let owner_user_id = session_user_id(session)?;
    let requesting_task_id = command
        .id
        .as_deref()
        .filter(|value| !value.trim().is_empty())
        .unwrap_or(&source_id)
        .to_string();
    let effect = crate::service::business_os::enqueue_web_stack_auth_assist_request(
        root,
        auth_source,
        None,
        None,
        None,
        None,
        &requesting_task_id,
        NEWS_MODULE,
        NEWS_SOURCE_AUTH_ASSIST,
        Some(&owner_user_id),
        true,
        true,
    )?;
    let conn = super::store::open_store(root)?;
    let mut payload = source
        .get("payload")
        .and_then(Value::as_object)
        .cloned()
        .unwrap_or_default();
    payload.insert("auth_assist".to_string(), effect.clone());
    patch_record(
        root,
        &conn,
        "news_sources",
        &source_id,
        now_ms() as i64,
        json!({
            "status": "auth_requested",
            "account_handle": source.get("account_handle").cloned().unwrap_or(json!("")),
            "payload": Value::Object(payload),
        }),
    )?;
    Ok(json!({ "ok": true, "source_id": source_id, "auth_assist": effect }))
}

// --- scheduled emission -----------------------------------------------------

/// Called by the schedule emitter for every due task. Returns `None` for
/// tasks that are not news schedules.
pub(crate) fn emit_scheduled_issue(
    root: &Path,
    task: &ScheduledTaskView,
    scheduled_for: &str,
) -> anyhow::Result<Option<(String, String)>> {
    if !task.name.starts_with(SCHEDULE_PREFIX) {
        return Ok(None);
    }
    let route: ScheduleRoute = serde_json::from_str(
        task.prompt
            .strip_prefix(ROUTE_MARKER)
            .context("news schedule route missing")?,
    )?;
    ensure!(
        task.name == schedule_name(&route.topic_id)
            && task.thread_key == topic_thread_key(&route.topic_id),
        "news schedule does not match its topic binding"
    );
    let skip_key = format!("news-skip:{}:{scheduled_for}", task.task_id);
    let issue_date = scheduled_for.get(..10).unwrap_or_default().to_string();
    // A deadline fires once; the yearly cron repeat is skipped.
    if route.cadence == "deadline"
        && !route.deadline_date.is_empty()
        && issue_date != route.deadline_date
    {
        return Ok(Some((skip_key, "skipped".to_string())));
    }
    let Some(topic) = load_rxdb_collection_record(root, "news_topics", &route.topic_id)? else {
        return Ok(Some((skip_key, "skipped".to_string())));
    };
    if topic.get("enabled").and_then(Value::as_bool) == Some(false) {
        return Ok(Some((skip_key, "skipped".to_string())));
    }
    let sources: Vec<Value> = find_rxdb_collection_records_by_string_field(
        root,
        "news_sources",
        "topic_id",
        &route.topic_id,
        200,
    )?
    .into_iter()
    .map(|(_, record)| record)
    .filter(|record| record.get("enabled").and_then(Value::as_bool) != Some(false))
    .collect();

    let digest =
        super::hashing::hex_sha256(format!("{}::{scheduled_for}", task.task_id).as_bytes());
    let issue_id = format!("issue_sched_{}", &digest[..24]);
    let cadence = topic
        .get("cadence")
        .and_then(Value::as_str)
        .unwrap_or(route.cadence.as_str())
        .to_string();
    let window = issue_window(root, &route.topic_id, &cadence, &issue_id, now_ms() as i64)?;
    let command_id = format!("cmd_news_issue_generate_sched_{}", &digest[..24]);
    let now = now_ms() as i64;
    let conn = super::store::open_store(root)?;
    let title = topic
        .get("title")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string();
    if load_rxdb_collection_record(root, "news_issues", &issue_id)?.is_none() {
        write_record(
            root,
            &conn,
            "news_issues",
            &issue_id,
            now,
            json!({
                "id": issue_id,
                "topic_id": route.topic_id,
                "title": title,
                "issue_date": issue_date,
                "status": "queued",
                "command_id": command_id,
                "task_id": "",
                "post_count": 0,
                "generated_at_ms": 0,
                "error": "",
                "payload": { "trigger": "schedule", "scheduled_for": scheduled_for },
                "created_at_ms": now,
                "updated_at_ms": now,
            }),
        )?;
    }
    drop(conn);

    let session = active_domain_recovery_session(root, &route.owner_user_id)?;
    let mut payload = generate_payload(
        &topic,
        &sources,
        &issue_id,
        &issue_date,
        &command_id,
        &window,
    );
    attach_checkpoint(&mut payload, root, &issue_id);
    let accepted = accept_rxdb_business_command_with_origin(
        root,
        json!({
            "id": command_id,
            "command_id": command_id,
            "module": NEWS_MODULE,
            "command_type": NEWS_ISSUE_GENERATE,
            "record_id": route.topic_id,
            "inbound_channel": "business_os.news",
            "sync_collections": ["news_issues", "news_posts"],
            "payload": payload,
            "client_context": {
                "actor": super::threads::actor_payload(&session),
                "source": "native-news-schedule",
            },
        }),
        CommandOrigin::TrustedLocal,
    )?;
    let key = accepted
        .get("task_id")
        .or_else(|| accepted.get("execution_task_id"))
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
        .with_context(|| {
            format!(
                "news issue not admitted to the queue: {}",
                accepted
                    .get("error_message")
                    .cloned()
                    .unwrap_or(Value::Null)
            )
        })?;
    let conn = super::store::open_store(root)?;
    patch_record(
        root,
        &conn,
        "news_issues",
        &issue_id,
        now_ms() as i64,
        json!({ "task_id": key }),
    )?;
    Ok(Some((key, "emitted".to_string())))
}

// --- freshness window and covered stories -----------------------------------

/// What an issue may contain: stories first published after `start_ms` that
/// no earlier issue of the same topic already carried.
#[derive(Debug, Clone, Default, PartialEq)]
struct IssueWindow {
    start_ms: i64,
    previous_issue_id: String,
    covered_urls: Vec<String>,
}

fn fallback_window_ms(cadence: &str) -> i64 {
    if cadence == "daily" {
        DAY_MS
    } else {
        7 * DAY_MS
    }
}

fn is_digest_issue(issue: &Value) -> bool {
    issue.pointer("/payload/kind").and_then(Value::as_str) == Some("weekly_digest")
}

/// Canonical form for comparing story links across issues.
fn normalize_story_url(url: &str) -> String {
    let url = url.trim();
    let url = url.split('#').next().unwrap_or_default();
    let scheme_less = url.split_once("://").map_or(url, |(_, rest)| rest);
    let (host, path) = scheme_less.split_once('/').unwrap_or((scheme_less, ""));
    let host = host.to_ascii_lowercase();
    let host = host.strip_prefix("www.").unwrap_or(&host);
    let path = path.trim_end_matches('/');
    if path.is_empty() {
        host.to_string()
    } else {
        format!("{host}/{path}")
    }
}

/// The story a post stands for: its link card, else its primary source.
fn primary_story_url(post: &Value) -> Option<String> {
    let card = post
        .pointer("/link_card/url")
        .and_then(Value::as_str)
        .filter(|url| !url.is_empty());
    let sources = post.get("sources").and_then(Value::as_array);
    let primary = sources.and_then(|items| {
        items
            .iter()
            .find(|source| source.get("kind").and_then(Value::as_str) == Some("primary"))
            .or_else(|| items.first())
            .and_then(|source| source.get("url").and_then(Value::as_str))
    });
    card.or(primary).map(normalize_story_url)
}

fn all_story_urls(post: &Value) -> Vec<String> {
    let mut urls: Vec<String> = Vec::new();
    if let Some(url) = primary_story_url(post) {
        urls.push(url);
    }
    for source in post
        .get("sources")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
    {
        if source.get("kind").and_then(Value::as_str) == Some("discussion") {
            continue;
        }
        if let Some(url) = source.get("url").and_then(Value::as_str) {
            let url = normalize_story_url(url);
            if !urls.contains(&url) {
                urls.push(url);
            }
        }
    }
    urls
}

/// Window for the next regular issue of `topic_id`: since the previous ready
/// issue, otherwise the last 24 hours (daily) or 7 days.
fn issue_window(
    root: &Path,
    topic_id: &str,
    cadence: &str,
    exclude_issue_id: &str,
    now: i64,
) -> anyhow::Result<IssueWindow> {
    let issues: Vec<Value> = find_rxdb_collection_records_by_string_field(
        root,
        "news_issues",
        "topic_id",
        topic_id,
        500,
    )?
    .into_iter()
    .map(|(_, record)| record)
    .filter(|issue| {
        issue.get("id").and_then(Value::as_str) != Some(exclude_issue_id)
            && issue.get("status").and_then(Value::as_str) == Some("ready")
            && !is_digest_issue(issue)
    })
    .collect();
    let posts: Vec<Value> = find_rxdb_collection_records_by_string_field(
        root,
        "news_posts",
        "topic_id",
        topic_id,
        2_000,
    )?
    .into_iter()
    .map(|(_, record)| record)
    .collect();
    Ok(window_from_history(&issues, &posts, cadence, now))
}

fn window_from_history(issues: &[Value], posts: &[Value], cadence: &str, now: i64) -> IssueWindow {
    let generated = |issue: &Value| {
        issue
            .get("generated_at_ms")
            .and_then(Value::as_i64)
            .unwrap_or(0)
    };
    let previous = issues
        .iter()
        .filter(|issue| generated(issue) > 0)
        .max_by_key(|issue| generated(issue));
    let fallback_start = now - fallback_window_ms(cadence);
    // A long pause never reaches further back than a week.
    let start_ms = match previous {
        Some(issue) if generated(issue) > now - 7 * DAY_MS => generated(issue).min(now),
        _ => fallback_start,
    };
    let recent: std::collections::HashSet<&str> = issues
        .iter()
        .filter(|issue| generated(issue) > now - COVERED_LOOKBACK_MS)
        .filter_map(|issue| issue.get("id").and_then(Value::as_str))
        .collect();
    let mut covered: Vec<String> = Vec::new();
    for post in posts {
        let in_recent = post
            .get("issue_id")
            .and_then(Value::as_str)
            .is_some_and(|id| recent.contains(id));
        if !in_recent {
            continue;
        }
        for url in all_story_urls(post) {
            if !covered.contains(&url) {
                covered.push(url);
            }
        }
    }
    covered.truncate(MAX_COVERED_URLS * 4);
    IssueWindow {
        start_ms,
        previous_issue_id: previous
            .and_then(|issue| issue.get("id").and_then(Value::as_str))
            .unwrap_or_default()
            .to_string(),
        covered_urls: covered,
    }
}

fn rfc3339(ms: i64) -> String {
    chrono::DateTime::<chrono::Utc>::from_timestamp_millis(ms)
        .map(|value| value.to_rfc3339_opts(chrono::SecondsFormat::Secs, true))
        .unwrap_or_default()
}

/// Why a post leaves the issue, or `None` when it stays.
fn freshness_rejection(post: &Value, window: &IssueWindow, now: i64) -> Option<&'static str> {
    let published = post
        .get("published_at_ms")
        .and_then(Value::as_i64)
        .unwrap_or(0);
    if published <= 0 {
        return Some("missing_published_at");
    }
    if published < window.start_ms - FRESHNESS_GRACE_MS {
        return Some("published_before_window");
    }
    if published > now + DAY_MS {
        return Some("published_in_future");
    }
    let primary = primary_story_url(post)?;
    if window.covered_urls.contains(&primary) {
        return Some("already_covered");
    }
    None
}

fn generate_payload(
    topic: &Value,
    sources: &[Value],
    issue_id: &str,
    issue_date: &str,
    command_id: &str,
    window: &IssueWindow,
) -> Value {
    let field = |key: &str| {
        topic
            .get(key)
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string()
    };
    let topic_id = field("id");
    let (usable, skipped) = select_run_sources(sources);
    let sources = usable.as_slice();
    let source_line = sources
        .iter()
        .map(|source| {
            format!(
                "{}:{}",
                source.get("kind").and_then(Value::as_str).unwrap_or("web"),
                source.get("label").and_then(Value::as_str).unwrap_or("")
            )
        })
        .collect::<Vec<_>>()
        .join("; ");
    let signed_in = sources.iter().any(|source| {
        source
            .get("kind")
            .and_then(Value::as_str)
            .and_then(account_auth_source)
            .is_some()
    });
    let mut instruction = vec![
        format!("Erstelle die News-Ausgabe für das Thema \"{}\" mit dem Skill {NEWS_SKILL}.", field("title")),
        format!("Fokus: {}", field("prompt")),
        format!(
            "Höchstens {} Beiträge. Quellen ({}): {}.",
            topic.get("max_posts").and_then(Value::as_u64).unwrap_or(12),
            sources.len(),
            if source_line.is_empty() { "keine konfiguriert, nutze die Websuche".to_string() } else { source_line }
        ),
        format!(
            "Zeitfenster: nur Meldungen, deren Primärquelle nach {} veröffentlicht wurde; ältere Beiträge verwirft die Übernahme.",
            rfc3339(window.start_ms)
        ),
        format!(
            "{} Meldungen stehen schon in früheren Ausgaben (covered_urls) und werden nicht wiederholt.",
            window.covered_urls.len().min(MAX_COVERED_URLS)
        ),
        format!("Liefere das Ergebnis im Vertrag {NEWS_ISSUE_CONTRACT}; jeder Beitrag braucht mindestens eine Quelle mit Link."),
        "Aufwand begrenzen: höchstens zwei Abrufe pro Quelle, keine Quelle zweimal lesen; sobald genug Beiträge belegt sind, nicht weiter suchen, sondern das Ergebnis liefern.".to_string(),
    ];
    if !skipped.is_empty() {
        instruction.push(format!(
            "Diese Quellen in diesem Lauf überspringen: {}.",
            skipped
                .iter()
                .map(|(label, reason)| format!("{label} ({reason})"))
                .collect::<Vec<_>>()
                .join("; ")
        ));
    }
    if signed_in {
        // The owner of an authenticated read is resolved from this command.
        instruction.insert(
            3,
            format!("Angemeldete Quellen (X, Reddit) über das CTOX-Konto lesen; --task-id {command_id}."),
        );
    }
    let instruction = instruction.join("\n");
    let snapshot_sources: Vec<Value> = sources
        .iter()
        .map(|source| {
            json!({
                "id": source.get("id"),
                "kind": source.get("kind"),
                "label": source.get("label"),
                "url": source.get("url"),
                "adapter_kind": source.get("adapter_kind"),
                "target_key": source.get("target_key"),
                "auth_mode": source.get("auth_mode"),
                "auth_source": source
                    .get("kind")
                    .and_then(Value::as_str)
                    .and_then(account_auth_source)
                    .unwrap_or_default(),
                "items": source.get("items"),
                "status": source.get("status"),
            })
        })
        .collect();
    json!({
        "topic_id": topic_id,
        "issue_id": issue_id,
        "issue_date": issue_date,
        "instruction": instruction,
        "required_skills": [NEWS_SKILL],
        "thread_key": topic_thread_key(&topic_id),
        "output_contract": NEWS_ISSUE_CONTRACT,
        "previous_issue_id": window.previous_issue_id,
        "window_start": rfc3339(window.start_ms),
        "window_start_ms": window.start_ms,
        "covered_urls": window.covered_urls.iter().take(MAX_COVERED_URLS).collect::<Vec<_>>(),
        "skipped_sources": skipped
            .iter()
            .map(|(label, reason)| json!({ "label": label, "reason": reason }))
            .collect::<Vec<_>>(),
        "record_snapshot": {
            "topic": {
                "id": topic_id,
                "title": field("title"),
                "prompt": field("prompt"),
                "cadence": field("cadence"),
                "language": field("language"),
                "categories": topic.get("categories").cloned().unwrap_or(json!([])),
                "max_posts": topic.get("max_posts").cloned().unwrap_or(json!(12)),
            },
            "sources": snapshot_sources,
        },
    })
}

/// Sources for one run: drop ones that cannot deliver (failed, needs_auth)
/// and cap the rest. Returns the usable sources and `(label, reason)` for the
/// skipped ones. If nothing usable is left the run falls back to web search.
fn select_run_sources(sources: &[Value]) -> (Vec<Value>, Vec<(String, String)>) {
    let mut usable = Vec::new();
    let mut skipped = Vec::new();
    for source in sources {
        let label = source
            .get("label")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string();
        let status = source.get("status").and_then(Value::as_str).unwrap_or("");
        if UNUSABLE_SOURCE_STATES.contains(&status) {
            skipped.push((label, status.to_string()));
        } else if usable.len() >= MAX_SOURCES_PER_RUN {
            skipped.push((label, "limit".to_string()));
        } else {
            usable.push(source.clone());
        }
    }
    (usable, skipped)
}

/// Gathered items survive a session timeout: the run writes them to this
/// file after reading the sources, and every later attempt (worker retry or
/// a Crew "Wiederholen") resumes from it instead of re-reading all sources.
fn issue_checkpoint_path(root: &Path, issue_id: &str) -> std::path::PathBuf {
    root.join("runtime")
        .join("news")
        .join("checkpoints")
        .join(format!("{issue_id}.json"))
}

fn attach_checkpoint(payload: &mut Value, root: &Path, issue_id: &str) {
    let path = issue_checkpoint_path(root, issue_id);
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let path = path.to_string_lossy().to_string();
    let note = format!(
        "Zwischenstand: Prüfe zuerst {path}. Existiert die Datei, sind die Quellen schon gelesen: übernimm die Kandidaten daraus und lies nur die dort fehlenden Quellen. Sonst nach dem Lesen der Quellen sofort alle Kandidaten (Titel, Link, Datum, Quelle, Kurzinhalt) als JSON dorthin schreiben, bevor du auswählst und schreibst."
    );
    if let Some(object) = payload.as_object_mut() {
        let instruction = object
            .get("instruction")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string();
        object.insert(
            "instruction".into(),
            json!(format!("{instruction}\n{note}")),
        );
        object.insert("checkpoint_path".into(), json!(path));
    }
}

/// Crew "Wiederholen" on a failed issue run submits the command again; put
/// the issue back to queued under the new command so the News app follows it.
pub(super) fn requeue_issue_for_retry(
    root: &Path,
    payload: &Value,
    command_id: &str,
    task_id: &str,
) -> anyhow::Result<()> {
    let Some(issue_id) =
        first_string_field(payload, &["issue_id"]).filter(|id| valid_record_id(id))
    else {
        return Ok(());
    };
    let conn = super::store::open_store(root)?;
    patch_record(
        root,
        &conn,
        "news_issues",
        &issue_id,
        now_ms() as i64,
        json!({ "status": "queued", "error": "", "command_id": command_id, "task_id": task_id }),
    )?;
    Ok(())
}

// --- queue writeback ----------------------------------------------------------

pub(super) fn is_news_queue_command(command: &BusinessCommand) -> bool {
    command.module == NEWS_MODULE
        && matches!(
            command.command_type.as_str(),
            NEWS_ISSUE_GENERATE | NEWS_SOURCE_ADAPTER
        )
}

/// Domain writeback after review/validation. Returns the typed result stored
/// on the business command.
pub(super) fn apply_news_queue_reply(
    root: &Path,
    conn: &Connection,
    command_id: &str,
    command: &BusinessCommand,
    task_id: &str,
    reply_text: &str,
) -> anyhow::Result<Value> {
    match command.command_type.as_str() {
        NEWS_ISSUE_GENERATE => {
            apply_issue_reply(root, conn, command_id, command, task_id, reply_text)
        }
        NEWS_SOURCE_ADAPTER => {
            let source_id = first_string_field(&command.payload, &["source_id"])
                .or_else(|| command.record_id.clone())
                .unwrap_or_default();
            ensure!(
                valid_record_id(&source_id),
                "news.source.adapter is missing source_id"
            );
            let now = now_ms() as i64;
            let report = parse_source_reply(reply_text);
            let mut patch = json!({
                "status": report.status,
                "last_error": report.note,
                "last_run_at_ms": now,
            });
            if !report.account.is_empty() {
                patch["account_handle"] = json!(report.account);
            }
            patch_record(root, conn, "news_sources", &source_id, now, patch)?;
            Ok(json!({ "ok": true, "source_id": source_id, "status": report.status }))
        }
        other => anyhow::bail!("not a news queue command: {other}"),
    }
}

fn apply_issue_reply(
    root: &Path,
    conn: &Connection,
    command_id: &str,
    command: &BusinessCommand,
    task_id: &str,
    reply_text: &str,
) -> anyhow::Result<Value> {
    let topic_id = first_string_field(&command.payload, &["topic_id"])
        .or_else(|| command.record_id.clone())
        .unwrap_or_default();
    let issue_id = first_string_field(&command.payload, &["issue_id"]).unwrap_or_default();
    ensure!(
        valid_record_id(&topic_id),
        "news issue command is missing topic_id"
    );
    ensure!(
        valid_record_id(&issue_id),
        "news issue command is missing issue_id"
    );
    let mut parsed = parse_issue_reply(reply_text)?;
    let now = now_ms() as i64;
    let digest_mode =
        first_string_field(&command.payload, &["mode"]).as_deref() == Some("weekly_digest");
    let mut rejected: Vec<Value> = Vec::new();
    if !digest_mode {
        let cadence = command
            .payload
            .pointer("/record_snapshot/topic/cadence")
            .and_then(Value::as_str)
            .unwrap_or("daily")
            .to_string();
        let mut window = issue_window(root, &topic_id, &cadence, &issue_id, now)?;
        if let Some(start) = command
            .payload
            .get("window_start_ms")
            .and_then(Value::as_i64)
            .filter(|start| *start > 0)
        {
            window.start_ms = start;
        }
        rejected = enforce_window(&mut parsed, &window, now);
    }

    for (index, post) in parsed.posts.iter().enumerate() {
        let post_id = format!("{issue_id}_p{:02}", index + 1);
        let mut record = post.clone();
        let object = record
            .as_object_mut()
            .expect("normalized post is an object");
        object.insert("id".to_string(), json!(post_id));
        object.insert("issue_id".to_string(), json!(issue_id));
        object.insert("topic_id".to_string(), json!(topic_id));
        object.insert("rank".to_string(), json!(index + 1));
        object.insert("bookmarked".to_string(), json!(false));
        object.insert("payload".to_string(), json!({ "command_id": command_id }));
        object.insert("created_at_ms".to_string(), json!(now));
        object.insert("updated_at_ms".to_string(), json!(now));
        write_record(root, conn, "news_posts", &post_id, now, record)?;
    }

    let existing = load_rxdb_collection_record(root, "news_issues", &issue_id)?;
    let mut issue = existing.unwrap_or_else(|| {
        json!({
            "id": issue_id,
            "topic_id": topic_id,
            "title": "",
            "issue_date": first_string_field(&command.payload, &["issue_date"]).unwrap_or_default(),
            "command_id": command_id,
            "created_at_ms": now,
        })
    });
    let mut payload = issue
        .get("payload")
        .and_then(Value::as_object)
        .cloned()
        .unwrap_or_default();
    // Trends point at posts by index in the reply; the app links by post id.
    let trends: Vec<Value> = parsed
        .trends
        .iter()
        .map(|trend| {
            let mut trend = trend.clone();
            let index = trend.get("post_index").and_then(Value::as_u64);
            if let Some(object) = trend.as_object_mut() {
                object.remove("post_index");
                let post_id = index
                    .filter(|index| (*index as usize) < parsed.posts.len())
                    .map(|index| format!("{issue_id}_p{:02}", index + 1))
                    .unwrap_or_default();
                object.insert("post_id".to_string(), json!(post_id));
            }
            trend
        })
        .collect();
    payload.insert("trends".to_string(), Value::Array(trends));
    if !parsed.summary.is_empty() {
        payload.insert("summary".to_string(), json!(parsed.summary));
    }
    payload.insert("rejected_posts".to_string(), Value::Array(rejected.clone()));
    if digest_mode {
        payload.insert("kind".to_string(), json!("weekly_digest"));
    }
    if let Some(object) = issue.as_object_mut() {
        object.insert("status".to_string(), json!("ready"));
        object.insert("task_id".to_string(), json!(task_id));
        object.insert("command_id".to_string(), json!(command_id));
        object.insert("post_count".to_string(), json!(parsed.posts.len()));
        object.insert("generated_at_ms".to_string(), json!(now));
        object.insert("error".to_string(), json!(""));
        object.insert("payload".to_string(), Value::Object(payload));
        object.insert("updated_at_ms".to_string(), json!(now));
    }
    write_record(root, conn, "news_issues", &issue_id, now, issue)?;
    // Only sources of this topic may be touched by its issue run.
    for (source_id, status, note) in &parsed.source_report {
        let belongs =
            load_rxdb_collection_record(root, "news_sources", source_id)?.is_some_and(|source| {
                source.get("topic_id").and_then(Value::as_str) == Some(topic_id.as_str())
            });
        if belongs {
            patch_record(
                root,
                conn,
                "news_sources",
                source_id,
                now,
                json!({ "status": status, "last_error": note, "last_run_at_ms": now }),
            )?;
        }
    }
    patch_record(
        root,
        conn,
        "news_topics",
        &topic_id,
        now,
        json!({ "last_issue_at_ms": now }),
    )?;
    Ok(json!({
        "ok": true,
        "issue_id": issue_id,
        "topic_id": topic_id,
        "post_count": parsed.posts.len(),
        "dropped_posts": parsed.dropped,
        "rejected_posts": rejected.len(),
    }))
}

/// Drops posts outside the freshness window or already carried by an earlier
/// issue and re-points trends at the posts that stay. Returns why each
/// rejected post left, so the issue can show it.
fn enforce_window(parsed: &mut ParsedIssue, window: &IssueWindow, now: i64) -> Vec<Value> {
    let mut kept = Vec::new();
    let mut index_map: Vec<Option<u64>> = Vec::new();
    let mut rejected = Vec::new();
    let mut seen: Vec<String> = Vec::new();
    for post in parsed.posts.drain(..) {
        let primary = primary_story_url(&post).unwrap_or_default();
        let reason = freshness_rejection(&post, window, now).or_else(|| {
            (!primary.is_empty() && seen.contains(&primary)).then_some("duplicate_in_issue")
        });
        match reason {
            Some(reason) => {
                index_map.push(None);
                rejected.push(json!({
                    "reason": reason,
                    "url": primary,
                    "intro": clip(post.get("intro"), 160),
                    "published_at_ms": post.get("published_at_ms").cloned().unwrap_or(json!(0)),
                }));
            }
            None => {
                index_map.push(Some(kept.len() as u64));
                seen.push(primary);
                kept.push(post);
            }
        }
    }
    parsed.posts = kept;
    parsed.dropped += rejected.len();
    let trends = std::mem::take(&mut parsed.trends);
    parsed.trends = trends
        .into_iter()
        .filter_map(|mut trend| {
            let old = trend.get("post_index").and_then(Value::as_u64);
            let new = match old {
                Some(index) => index_map.get(index as usize).copied().flatten(),
                None => None,
            };
            if old.is_some() && new.is_none() {
                // The trend was carried by a rejected post.
                return None;
            }
            trend["post_index"] = json!(new);
            Some(trend)
        })
        .collect();
    rejected
}

/// "Gemini 4 Argon und Robotik (Hacker News, 1.403 Punkte)" → "Gemini 4 Argon und Robotik".
fn concise_trend_title(title: &str) -> String {
    let base = match title.find(" (") {
        Some(index) if index > 0 => &title[..index],
        _ => title,
    };
    let base = base.trim();
    if base.chars().count() <= MAX_TREND_TITLE {
        return base.to_string();
    }
    let clipped: String = base.chars().take(MAX_TREND_TITLE - 1).collect();
    let clipped = clipped
        .rsplit_once(' ')
        .map_or(clipped.as_str(), |(head, _)| head)
        .trim_end_matches([',', ';', ':', '–', '-'])
        .trim_end();
    format!("{clipped}…")
}

fn is_hacker_news(quote: &Value) -> bool {
    let url = quote.get("url").and_then(Value::as_str).unwrap_or_default();
    let source = quote
        .get("source")
        .and_then(Value::as_str)
        .unwrap_or_default();
    url.contains("news.ycombinator.com")
        || url.contains("hn.algolia.com")
        || source.to_ascii_lowercase().contains("hacker news")
}

/// Terminal failure of a news queue run: the issue or source shows the error
/// instead of spinning forever.
pub(super) fn mark_news_command_failed(
    root: &Path,
    conn: &Connection,
    command: &BusinessCommand,
    error: &str,
) -> anyhow::Result<()> {
    if !is_news_queue_command(command) {
        return Ok(());
    }
    let now = now_ms() as i64;
    let message: String = error.chars().take(500).collect();
    match command.command_type.as_str() {
        NEWS_ISSUE_GENERATE => {
            if let Some(issue_id) =
                first_string_field(&command.payload, &["issue_id"]).filter(|id| valid_record_id(id))
            {
                patch_record(
                    root,
                    conn,
                    "news_issues",
                    &issue_id,
                    now,
                    json!({ "status": "failed", "error": message }),
                )?;
            }
        }
        _ => {
            if let Some(source_id) = first_string_field(&command.payload, &["source_id"])
                .filter(|id| valid_record_id(id))
            {
                patch_record(
                    root,
                    conn,
                    "news_sources",
                    &source_id,
                    now,
                    json!({ "status": "failed", "last_error": message }),
                )?;
            }
        }
    }
    Ok(())
}

#[derive(Debug, PartialEq)]
struct SourceReport {
    status: &'static str,
    account: String,
    note: String,
}

/// `ctox.news.source.v1` from an account connection. A scraper setup replies
/// in prose; that stays the plain success it always was.
fn parse_source_reply(reply_text: &str) -> SourceReport {
    let Some(value) = extract_json_object(reply_text)
        .filter(|value| value.get("schema").and_then(Value::as_str) == Some(NEWS_SOURCE_CONTRACT))
    else {
        return SourceReport {
            status: "ok",
            account: String::new(),
            note: String::new(),
        };
    };
    let status = match value.get("status").and_then(Value::as_str) {
        Some("ok") | Some("connected") => "ok",
        Some("needs_auth") | Some("needs_owner") => "needs_auth",
        _ => "failed",
    };
    // Account names only; a reply never carries the password.
    let account = clip(value.get("account"), 120);
    SourceReport {
        status,
        account: if account.contains(char::is_whitespace) {
            String::new()
        } else {
            account
        },
        note: if status == "ok" {
            String::new()
        } else {
            clip(value.get("note"), 500)
        },
    }
}

#[derive(Debug)]
struct ParsedIssue {
    posts: Vec<Value>,
    trends: Vec<Value>,
    source_report: Vec<(String, String, String)>,
    summary: String,
    dropped: usize,
}

fn clip(value: Option<&Value>, max: usize) -> String {
    value
        .and_then(Value::as_str)
        .map(str::trim)
        .unwrap_or_default()
        .chars()
        .take(max)
        .collect()
}

fn http_url(value: Option<&Value>) -> Option<String> {
    let url = clip(value, 2_000);
    (url.starts_with("https://") || url.starts_with("http://")).then_some(url)
}

fn non_negative(value: Option<&Value>, max: f64) -> f64 {
    value
        .and_then(Value::as_f64)
        .filter(|number| number.is_finite())
        .map(|number| number.clamp(0.0, max).round())
        .unwrap_or(0.0)
}

fn published_at_ms(post: &Value) -> i64 {
    if let Some(ms) = post.get("published_at_ms").and_then(Value::as_i64) {
        return ms.max(0);
    }
    post.get("published_at")
        .and_then(Value::as_str)
        .and_then(|value| chrono::DateTime::parse_from_rfc3339(value).ok())
        .map(|value| value.timestamp_millis())
        .unwrap_or(0)
}

fn parse_issue_reply(reply_text: &str) -> anyhow::Result<ParsedIssue> {
    let reply = reply_text.trim();
    ensure!(!reply.is_empty(), "news issue reply was empty");
    let value =
        extract_json_object(reply).context("news issue reply did not contain a JSON object")?;
    if let Some(schema) = value.get("schema").and_then(Value::as_str) {
        ensure!(
            schema == NEWS_ISSUE_CONTRACT,
            "news issue reply uses schema `{schema}`, expected {NEWS_ISSUE_CONTRACT}"
        );
    }
    let raw_posts = value
        .get("posts")
        .and_then(Value::as_array)
        .context("news issue reply has no posts array")?;
    let mut posts = Vec::new();
    let mut dropped = 0;
    for raw in raw_posts.iter().take(MAX_POSTS) {
        match normalize_post(raw) {
            Some(post) => posts.push(post),
            None => dropped += 1,
        }
    }
    dropped += raw_posts.len().saturating_sub(MAX_POSTS);
    ensure!(
        !posts.is_empty(),
        "news issue reply has no post with intro and a linked source"
    );
    let trends = value
        .get("trends")
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .take(10)
                .filter_map(|trend| {
                    let title = concise_trend_title(&clip(trend.get("title"), 200));
                    (!title.is_empty()).then(|| {
                        json!({
                            "title": title,
                            "category": clip(trend.get("category"), 40),
                            "discussion_count": non_negative(trend.get("discussion_count"), 1.0e9),
                            "post_index": trend.get("post_index").and_then(Value::as_u64),
                        })
                    })
                })
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    let source_report = value
        .get("source_report")
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .take(200)
                .filter_map(|item| {
                    let source_id = clip(item.get("source_id"), 220);
                    let status = clip(item.get("status"), 20);
                    (valid_record_id(&source_id)
                        && matches!(status.as_str(), "ok" | "needs_auth" | "failed"))
                    .then(|| (source_id, status, clip(item.get("note"), 300)))
                })
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    Ok(ParsedIssue {
        posts,
        trends,
        source_report,
        summary: clip(value.get("summary"), 1_000),
        dropped,
    })
}

fn normalize_post(raw: &Value) -> Option<Value> {
    let intro = clip(raw.get("intro"), 600);
    if intro.is_empty() {
        return None;
    }
    let sources: Vec<Value> = raw
        .get("sources")
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .take(12)
                .filter_map(|source| {
                    let url = http_url(source.get("url"))?;
                    Some(json!({
                        "label": clip(source.get("label"), 120),
                        "url": url,
                        "kind": clip(source.get("kind"), 40),
                    }))
                })
                .collect()
        })
        .unwrap_or_default();
    if sources.is_empty() {
        return None;
    }
    let category = clip(raw.get("category"), 20);
    let category = if CATEGORIES.contains(&category.as_str()) {
        category
    } else {
        "other".to_string()
    };
    let author_kind = clip(raw.get("author_kind"), 20);
    let author_kind = if matches!(
        author_kind.as_str(),
        "lab" | "community" | "media" | "person"
    ) {
        author_kind
    } else {
        "media".to_string()
    };
    let mut post = Map::new();
    post.insert("category".to_string(), json!(category));
    post.insert(
        "author_name".to_string(),
        json!(clip(raw.get("author_name"), 120)),
    );
    post.insert(
        "author_handle".to_string(),
        json!(clip(raw.get("author_handle"), 80).trim_start_matches('@')),
    );
    post.insert("author_kind".to_string(), json!(author_kind));
    post.insert("intro".to_string(), json!(intro));
    post.insert("body".to_string(), json!(clip(raw.get("body"), MAX_TEXT)));
    post.insert(
        "hype_score".to_string(),
        json!(non_negative(raw.get("hype_score"), 100.0)),
    );
    post.insert(
        "discussion_count".to_string(),
        json!(non_negative(raw.get("discussion_count"), 1.0e9)),
    );
    post.insert("published_at_ms".to_string(), json!(published_at_ms(raw)));
    post.insert("sources".to_string(), Value::Array(sources));
    let link_card = raw.get("link_card").and_then(|card| {
        let url = http_url(card.get("url"))?;
        Some(json!({
            "url": url,
            "title": clip(card.get("title"), 200),
            "image_url": http_url(card.get("image_url")).unwrap_or_default(),
        }))
    });
    post.insert(
        "link_card".to_string(),
        link_card.unwrap_or_else(|| json!({})),
    );
    // Comments from the Hacker News thread are discussion noise, not substance.
    let quote = raw
        .get("quote")
        .filter(|quote| !is_hacker_news(quote))
        .and_then(|quote| {
            let text = clip(quote.get("text"), 600);
            (!text.is_empty()).then(|| {
                json!({
                    "text": text,
                    "source": clip(quote.get("source"), 120),
                    "meta": clip(quote.get("meta"), 120),
                    "url": http_url(quote.get("url")).unwrap_or_default(),
                })
            })
        });
    post.insert("quote".to_string(), quote.unwrap_or_else(|| json!({})));
    Some(Value::Object(post))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn issue_reply_keeps_only_sourced_posts_and_clamps_values() {
        let reply = r#"Fertig.
```json
{"schema":"ctox.news.issue.v1","summary":"Zwei Releases",
 "posts":[
  {"category":"model","author_name":"OpenAI","author_handle":"@openai","author_kind":"lab",
   "intro":"Neues Modell","body":"Lang","hype_score":140,"discussion_count":2400,
   "published_at":"2026-10-13T05:00:00Z",
   "link_card":{"url":"https://openai.com/x","title":"T","image_url":"javascript:alert(1)"},
   "sources":[{"label":"Blog","url":"https://openai.com/x"},{"label":"bad","url":"ftp://x"}]},
  {"category":"weird","intro":"Ohne Quelle","sources":[]},
  {"intro":"","sources":[{"url":"https://a.example"}]}
 ],
 "trends":[{"title":"Agent-Sandboxing","discussion_count":860},{"title":""}],
 "source_report":[{"source_id":"source_x","status":"needs_auth","note":"login wall"},{"source_id":"../x","status":"ok"},{"source_id":"source_y","status":"great"}]}
```"#;
        let parsed = parse_issue_reply(reply).expect("parses");
        assert_eq!(parsed.posts.len(), 1);
        assert_eq!(parsed.dropped, 2);
        let post = &parsed.posts[0];
        assert_eq!(post["hype_score"], json!(100.0));
        assert_eq!(post["author_handle"], json!("openai"));
        assert_eq!(post["sources"].as_array().unwrap().len(), 1);
        assert_eq!(post["link_card"]["image_url"], json!(""));
        assert_eq!(post["published_at_ms"], json!(1_791_867_600_000_i64));
        assert_eq!(parsed.trends.len(), 1);
        assert_eq!(parsed.summary, "Zwei Releases");
        assert_eq!(
            parsed.source_report,
            vec![(
                "source_x".to_string(),
                "needs_auth".to_string(),
                "login wall".to_string()
            )]
        );
    }

    #[test]
    fn issue_reply_rejects_wrong_schema_and_empty_feeds() {
        assert!(parse_issue_reply(r#"{"schema":"other","posts":[]}"#).is_err());
        assert!(parse_issue_reply(r#"{"posts":[{"intro":"x","sources":[]}]}"#).is_err());
        assert!(parse_issue_reply("kein json").is_err());
    }

    #[test]
    fn record_ids_are_restricted() {
        assert!(valid_record_id("topic_63ce100b-5c0e-46f4"));
        assert!(!valid_record_id("../etc"));
        assert!(!valid_record_id(""));
    }

    #[test]
    fn schedule_route_round_trips_through_the_prompt() {
        let route = ScheduleRoute {
            topic_id: "topic_1".into(),
            owner_user_id: "user_1".into(),
            cadence: "deadline".into(),
            deadline_date: "2026-11-01".into(),
        };
        let prompt = format!("{ROUTE_MARKER}{}", serde_json::to_string(&route).unwrap());
        let back: ScheduleRoute =
            serde_json::from_str(prompt.strip_prefix(ROUTE_MARKER).unwrap()).unwrap();
        assert_eq!(back, route);
        assert_eq!(schedule_name("topic_1"), "news:topic_1");
    }

    #[test]
    fn account_connection_reply_maps_onto_the_source_row() {
        let ok = parse_source_reply(
            r#"Verbunden. {"schema":"ctox.news.source.v1","source_id":"source_1","status":"connected","account":"ctox_news","note":"x"}"#,
        );
        assert_eq!(
            ok,
            SourceReport {
                status: "ok",
                account: "ctox_news".into(),
                note: String::new()
            }
        );
        let owner = parse_source_reply(
            r#"{"schema":"ctox.news.source.v1","status":"needs_owner","account":"a b","note":"CAPTCHA im Browser lösen"}"#,
        );
        assert_eq!(owner.status, "needs_auth");
        assert!(owner.account.is_empty());
        assert_eq!(owner.note, "CAPTCHA im Browser lösen");
        let unknown = parse_source_reply(r#"{"schema":"ctox.news.source.v1","status":"weird"}"#);
        assert_eq!(unknown.status, "failed");
        // A scraper setup answers in prose and stays a plain success.
        assert_eq!(parse_source_reply("Scraper läuft.").status, "ok");
        assert_eq!(account_auth_source("reddit"), Some("reddit.com"));
        assert_eq!(account_auth_source("lab"), None);
    }

    #[test]
    fn scheduled_issue_names_its_command_for_signed_in_sources() {
        let topic = json!({"id": "topic_1", "title": "KI", "prompt": "", "max_posts": 5});
        let reddit = json!({"id": "source_1", "kind": "reddit", "label": "Reddit", "items": ["r/LocalLLaMA"]});
        let lab = json!({"id": "source_2", "kind": "lab", "label": "OpenAI"});
        let window = IssueWindow {
            start_ms: 1_791_867_600_000,
            previous_issue_id: "issue_0".into(),
            covered_urls: vec!["openai.com/x".into()],
        };
        let with_account = generate_payload(
            &topic,
            &[reddit, lab.clone()],
            "issue_1",
            "2026-10-14",
            "cmd_1",
            &window,
        );
        assert_eq!(with_account["previous_issue_id"], "issue_0");
        assert_eq!(with_account["window_start"], "2026-10-13T05:00:00Z");
        assert_eq!(with_account["covered_urls"], json!(["openai.com/x"]));
        let instruction = with_account["instruction"].as_str().unwrap();
        assert!(instruction.contains("--task-id cmd_1"));
        assert_eq!(
            with_account["record_snapshot"]["sources"][0]["auth_source"],
            "reddit.com"
        );
        let plain = generate_payload(&topic, &[lab], "issue_1", "2026-10-14", "cmd_1", &window);
        assert!(!plain["instruction"].as_str().unwrap().contains("--task-id"));
    }

    fn post(url: &str, published: &str) -> Value {
        json!({"intro": "x", "published_at": published, "category": "model",
               "link_card": {"url": url}, "sources": [{"url": url, "kind": "primary"}]})
    }

    #[test]
    fn window_drops_stale_covered_and_undated_posts_and_repoints_trends() {
        let now = chrono::DateTime::parse_from_rfc3339("2026-10-15T07:00:00Z")
            .unwrap()
            .timestamp_millis();
        let reply = json!({
            "schema": NEWS_ISSUE_CONTRACT,
            "posts": [
                post("https://www.anthropic.com/research/self-replicating-ai/", "2026-10-12T19:30:00Z"),
                post("https://cohere.com/blog/new", "2026-10-14T16:48:00Z"),
                post("https://deepmind.google/models/model-cards/gemini-4-argon/", "2026-10-14T20:00:00Z"),
                {"intro": "ohne Datum", "sources": [{"url": "https://a.example/b"}]},
                post("https://cohere.com/blog/new#top", "2026-10-14T17:00:00Z"),
            ],
            "trends": [
                {"title": "KI-Würmer", "post_index": 0},
                {"title": "Frontier-Skalierung mit Safety-Vorbehalt (Gemini 4 Argon, Muse Wonder)", "post_index": 2},
                {"title": "Coding-Agenten", "post_index": 1},
            ],
        })
        .to_string();
        let mut parsed = parse_issue_reply(&reply).unwrap();
        let window = IssueWindow {
            start_ms: now - DAY_MS,
            previous_issue_id: String::new(),
            covered_urls: vec!["deepmind.google/models/model-cards/gemini-4-argon".into()],
        };
        let rejected = enforce_window(&mut parsed, &window, now);
        let reasons: Vec<&str> = rejected
            .iter()
            .map(|r| r["reason"].as_str().unwrap())
            .collect();
        assert_eq!(
            reasons,
            vec![
                "published_before_window",
                "already_covered",
                "missing_published_at",
                "duplicate_in_issue"
            ]
        );
        assert_eq!(parsed.posts.len(), 1);
        assert_eq!(parsed.trends.len(), 1);
        assert_eq!(parsed.trends[0]["title"], "Coding-Agenten");
        assert_eq!(parsed.trends[0]["post_index"], json!(0));
    }

    #[test]
    fn window_runs_since_previous_issue_and_remembers_its_stories() {
        let now = 100 * DAY_MS;
        let issues = vec![
            json!({"id": "i1", "status": "ready", "generated_at_ms": now - 9 * HOUR_MS}),
            json!({"id": "i0", "status": "ready", "generated_at_ms": now - 40 * DAY_MS}),
        ];
        let posts = vec![
            post("https://cohere.com/blog/new", "2026-10-14T16:48:00Z")
                .as_object()
                .cloned()
                .map(|mut p| {
                    p.insert("issue_id".into(), json!("i1"));
                    Value::Object(p)
                })
                .unwrap(),
            json!({"issue_id": "i0", "link_card": {"url": "https://old.example/a"}, "sources": []}),
        ];
        let window = window_from_history(&issues, &posts, "daily", now);
        assert_eq!(window.start_ms, now - 9 * HOUR_MS);
        assert_eq!(window.previous_issue_id, "i1");
        assert_eq!(window.covered_urls, vec!["cohere.com/blog/new".to_string()]);
        let empty = window_from_history(&[], &[], "weekly", now);
        assert_eq!(empty.start_ms, now - 7 * DAY_MS);
        assert_eq!(
            window_from_history(&[], &[], "daily", now).start_ms,
            now - DAY_MS
        );
    }

    #[test]
    fn hacker_news_quotes_are_dropped_and_trend_titles_stay_short() {
        let raw = json!({"intro": "x", "sources": [{"url": "https://a.example"}],
            "quote": {"source": "Hacker News", "text": "for $250?", "url": "https://news.ycombinator.com/item?id=1"}});
        assert_eq!(normalize_post(&raw).unwrap()["quote"], json!({}));
        let reddit = json!({"intro": "x", "sources": [{"url": "https://a.example"}],
            "quote": {"source": "Reddit", "text": "gut", "url": "https://reddit.com/r/x"}});
        assert_eq!(normalize_post(&reddit).unwrap()["quote"]["text"], "gut");
        assert_eq!(
            concise_trend_title("On-Device-Inferenz aus ungewöhnlichen Bausteinen (EmbeddingGemma-Transcribe, AirLLM, LFM2.5)"),
            "On-Device-Inferenz aus ungewöhnlichen Bausteinen"
        );
        assert!(concise_trend_title(&"Wort ".repeat(30)).chars().count() <= MAX_TREND_TITLE);
        assert_eq!(
            normalize_story_url("https://WWW.Cohere.com/blog/new/#x"),
            "cohere.com/blog/new"
        );
    }

    fn test_window() -> IssueWindow {
        window_from_history(&[], &[], "daily", 0)
    }

    #[test]
    fn run_skips_unusable_sources_and_caps_the_rest() {
        let mut sources = vec![
            json!({"label": "OpenAI", "kind": "lab", "status": "failed"}),
            json!({"label": "Reddit", "kind": "reddit", "status": "needs_auth"}),
        ];
        for index in 0..(MAX_SOURCES_PER_RUN + 2) {
            sources.push(json!({"label": format!("S{index}"), "kind": "lab", "status": "ok"}));
        }
        let (usable, skipped) = select_run_sources(&sources);
        assert_eq!(usable.len(), MAX_SOURCES_PER_RUN);
        assert_eq!(skipped[0], ("OpenAI".to_string(), "failed".to_string()));
        assert_eq!(skipped[1], ("Reddit".to_string(), "needs_auth".to_string()));
        assert_eq!(skipped.iter().filter(|(_, r)| r == "limit").count(), 2);
        let topic = json!({"id": "topic_1", "title": "KI", "prompt": "", "max_posts": 5});
        let payload = generate_payload(
            &topic,
            &sources,
            "issue_1",
            "2026-10-15",
            "cmd_1",
            &test_window(),
        );
        let instruction = payload["instruction"].as_str().unwrap();
        assert!(instruction.contains("OpenAI (failed)"));
        assert!(
            !instruction.contains("--task-id"),
            "skipped Reddit must not ask for a signed-in read"
        );
        assert_eq!(
            payload["record_snapshot"]["sources"]
                .as_array()
                .unwrap()
                .len(),
            MAX_SOURCES_PER_RUN
        );
    }

    #[test]
    fn checkpoint_is_stable_per_issue() {
        let root = std::env::temp_dir().join(format!("ctox-news-ckpt-{}", std::process::id()));
        let mut a = json!({"instruction": "x"});
        let mut b = json!({"instruction": "x"});
        attach_checkpoint(&mut a, &root, "issue_1");
        attach_checkpoint(&mut b, &root, "issue_1");
        assert_eq!(a["checkpoint_path"], b["checkpoint_path"]);
        assert!(a["checkpoint_path"]
            .as_str()
            .unwrap()
            .ends_with("checkpoints/issue_1.json"));
        assert!(a["instruction"].as_str().unwrap().contains("Zwischenstand"));
        let _ = std::fs::remove_dir_all(root);
    }
}
