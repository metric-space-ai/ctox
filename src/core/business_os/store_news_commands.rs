// Origin: CTOX
// License: Apache-2.0
//
// News app (modules/news): native side of its four commands.
//
// - `news.topic.schedule` (control): owns the topic's `ctox schedule` entry.
// - `news.source.auth_assist` (control): hands the X sign-in to the owner's
//   CTOX browser profile, the same path Outbound uses for LinkedIn/XING.
// - `news.issue.generate` (queue): the harness researches the sources with the
//   `news-aggregation` skill and replies with a `ctox.news.issue.v1` object;
//   this file validates it and writes news_issues/news_posts.
// - `news.source.adapter` (queue): a `universal-scraping` run; its terminal
//   state is projected onto the source row.
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
const X_LOGIN_URL: &str = "https://x.com/i/flow/login";
const MAX_POSTS: usize = 40;
const MAX_TEXT: usize = 8_000;
const CATEGORIES: [&str; 4] = ["model", "tool", "research", "debate"];

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
    ensure!(
        source.get("kind").and_then(Value::as_str) == Some("x"),
        "only X sources sign in through the browser"
    );
    let owner_user_id = session_user_id(session)?;
    let requesting_task_id = command
        .id
        .as_deref()
        .filter(|value| !value.trim().is_empty())
        .unwrap_or(&source_id)
        .to_string();
    let effect = crate::service::business_os::enqueue_web_stack_auth_assist_request(
        root,
        "x.com",
        Some(X_LOGIN_URL),
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
            "status": "needs_auth",
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
            "payload": generate_payload(&topic, &sources, &issue_id, &issue_date),
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

fn generate_payload(topic: &Value, sources: &[Value], issue_id: &str, issue_date: &str) -> Value {
    let field = |key: &str| {
        topic
            .get(key)
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string()
    };
    let topic_id = field("id");
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
    let instruction = [
        format!("Erstelle die News-Ausgabe für das Thema \"{}\" mit dem Skill {NEWS_SKILL}.", field("title")),
        format!("Fokus: {}", field("prompt")),
        format!(
            "Höchstens {} Beiträge. Quellen ({}): {}.",
            topic.get("max_posts").and_then(Value::as_u64).unwrap_or(12),
            sources.len(),
            if source_line.is_empty() { "keine konfiguriert, nutze die Websuche".to_string() } else { source_line }
        ),
        format!("Liefere das Ergebnis im Vertrag {NEWS_ISSUE_CONTRACT}; jeder Beitrag braucht mindestens eine Quelle mit Link."),
    ]
    .join("\n");
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
            patch_record(
                root,
                conn,
                "news_sources",
                &source_id,
                now,
                json!({ "status": "ok", "last_error": "", "last_run_at_ms": now }),
            )?;
            Ok(json!({ "ok": true, "source_id": source_id, "status": "ok" }))
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
    let parsed = parse_issue_reply(reply_text)?;
    let now = now_ms() as i64;

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
    }))
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

#[derive(Debug)]
struct ParsedIssue {
    posts: Vec<Value>,
    trends: Vec<Value>,
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
                    let title = clip(trend.get("title"), 120);
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
    Ok(ParsedIssue {
        posts,
        trends,
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
    let quote = raw.get("quote").and_then(|quote| {
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
 "trends":[{"title":"Agent-Sandboxing","discussion_count":860},{"title":""}]}
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
}
