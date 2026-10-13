// Origin: CTOX
// License: AGPL-3.0-only

//! Project feed (ctox.workjet.project_feed.v1): append-only, supervisor-narrated
//! posts for the Workjet project overview. Browsers only read; the bound
//! project supervisor writes through `business_os.project_feed`.
use super::store::BusinessCommand;
use super::workjet_project_feed_contract as wire;
use anyhow::{ensure, Context};
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::path::Path;

pub(super) const CONTRACT: &str = wire::CONTRACT_SCHEMA;
pub(super) const READ_COMMAND: &str = "ctox.workjet.project.feed.read";
const TABLE: &str = "workjet_project_feed_posts";
pub(super) const SCHEMA: &str = "
CREATE TABLE IF NOT EXISTS workjet_project_feed_posts (
    post_id TEXT PRIMARY KEY,
    project_id TEXT NOT NULL,
    owner_user_id TEXT NOT NULL,
    post_json TEXT NOT NULL,
    occurred_at_ms INTEGER NOT NULL,
    created_at_ms INTEGER NOT NULL
);
CREATE INDEX IF NOT EXISTS workjet_project_feed_by_project
    ON workjet_project_feed_posts(project_id, occurred_at_ms DESC);";

pub(super) fn is_command(kind: &str) -> bool {
    kind == READ_COMMAND
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct FeedSource {
    #[serde(rename = "type")]
    pub kind: String,
    pub label: String,
    #[serde(rename = "ref")]
    pub reference: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct FeedAction {
    #[serde(rename = "type")]
    pub kind: String,
    pub label_de: String,
    pub label_en: String,
    #[serde(rename = "ref")]
    pub reference: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct FeedPost {
    pub post_id: String,
    pub project_id: String,
    pub kind: String,
    pub author_kind: String,
    pub author_thread: Option<String>,
    pub author_label: String,
    pub harness: Option<String>,
    pub headline: String,
    pub body: String,
    pub open_items: Option<Vec<String>>,
    pub sources: Vec<FeedSource>,
    pub action: Option<FeedAction>,
    pub thread_ref: Option<String>,
    pub occurred_at_ms: i64,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct ReadFeedRequest {
    pub project_id: String,
    #[serde(default)]
    pub before_ms: Option<i64>,
    #[serde(default)]
    pub limit: Option<u32>,
}

fn text(value: &str, max: usize, field: &str) -> anyhow::Result<()> {
    ensure!(!value.trim().is_empty(), "{field} must not be empty");
    ensure!(value.chars().count() <= max, "{field} exceeds {max} characters");
    ensure!(
        !value.chars().any(|c| c.is_control() && c != '\n'),
        "{field} contains control characters"
    );
    Ok(())
}

fn safe_ref(value: &str) -> anyhow::Result<()> {
    text(value, wire::MAX_REF, "ref")?;
    ensure!(!value.starts_with('/') && !value.contains(":\\"), "ref must not be a local absolute path");
    let lower = value.to_ascii_lowercase();
    ensure!(
        !["token=", "password", "secret", "api_key", "apikey"].iter().any(|word| lower.contains(word)),
        "ref must not carry credentials"
    );
    Ok(())
}

impl ReadFeedRequest {
    pub(crate) fn validate(&self) -> anyhow::Result<()> {
        text(&self.project_id, wire::MAX_ID, "project_id")?;
        if let Some(limit) = self.limit {
            ensure!((1..=wire::MAX_PAGE).contains(&limit), "limit must be 1..{}", wire::MAX_PAGE);
        }
        Ok(())
    }
}

impl FeedPost {
    pub(crate) fn validate(&self, now_ms: i64) -> anyhow::Result<()> {
        text(&self.post_id, 64, "post_id")?;
        text(&self.project_id, wire::MAX_ID, "project_id")?;
        ensure!(wire::KINDS.contains(&self.kind.as_str()), "unknown feed kind");
        ensure!(wire::AUTHOR_KINDS.contains(&self.author_kind.as_str()), "unknown author kind");
        if let Some(thread) = &self.author_thread {
            text(thread, wire::MAX_ID, "author_thread")?;
        }
        text(&self.author_label, wire::MAX_AUTHOR_LABEL, "author_label")?;
        if let Some(harness) = &self.harness {
            text(harness, wire::MAX_HARNESS, "harness")?;
        }
        text(&self.headline, wire::MAX_HEADLINE, "headline")?;
        text(&self.body, wire::MAX_BODY, "body")?;
        if let Some(items) = &self.open_items {
            ensure!(items.len() <= wire::MAX_OPEN_ITEMS, "too many open items");
            for item in items {
                text(item, wire::MAX_HEADLINE, "open_items[]")?;
            }
        }
        ensure!(self.sources.len() <= wire::MAX_SOURCES, "too many sources");
        for source in &self.sources {
            ensure!(wire::SOURCE_TYPES.contains(&source.kind.as_str()), "unknown source type");
            text(&source.label, wire::MAX_SOURCE_LABEL, "sources[].label")?;
            safe_ref(&source.reference)?;
        }
        if let Some(action) = &self.action {
            ensure!(wire::ACTION_TYPES.contains(&action.kind.as_str()), "unknown action type");
            text(&action.label_de, wire::MAX_ACTION_LABEL, "action.label_de")?;
            text(&action.label_en, wire::MAX_ACTION_LABEL, "action.label_en")?;
            safe_ref(&action.reference)?;
        }
        if let Some(thread) = &self.thread_ref {
            text(thread, wire::MAX_ID, "thread_ref")?;
        }
        ensure!(self.occurred_at_ms > 0, "occurred_at_ms must be positive");
        ensure!(
            self.occurred_at_ms <= now_ms + wire::MAX_FUTURE_MS,
            "occurred_at_ms lies too far in the future"
        );
        Ok(())
    }
}

pub(super) fn ensure_schema(conn: &Connection) -> anyhow::Result<()> {
    conn.execute_batch(SCHEMA)?;
    Ok(())
}

fn table_exists(conn: &Connection) -> anyhow::Result<bool> {
    Ok(conn.query_row(
        "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type='table' AND name=?1)",
        [TABLE],
        |row| row.get(0),
    )?)
}

/// Newest first; an absent table is an empty feed, never an error.
pub(super) fn read_posts(
    conn: &Connection,
    project_id: &str,
    owner: &str,
    before_ms: Option<i64>,
    limit: u32,
) -> anyhow::Result<(Vec<FeedPost>, Option<i64>)> {
    if !table_exists(conn)? {
        return Ok((vec![], None));
    }
    let mut statement = conn.prepare(
        "SELECT owner_user_id, post_json FROM workjet_project_feed_posts
          WHERE project_id=?1 AND occurred_at_ms < ?2
          ORDER BY occurred_at_ms DESC, post_id DESC LIMIT ?3",
    )?;
    let rows = statement.query_map(
        params![project_id, before_ms.unwrap_or(i64::MAX), i64::from(limit) + 1],
        |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)),
    )?;
    let mut posts = Vec::new();
    for row in rows {
        let (stored_owner, data) = row?;
        ensure!(stored_owner == owner, "feed owner conflicts with current project ownership");
        ensure!(data.len() <= 16 * 1024, "feed post exceeds read budget");
        let post: FeedPost = serde_json::from_str(&data)?;
        ensure!(post.project_id == project_id, "feed post belongs to another project");
        posts.push(post);
    }
    let next = if posts.len() > limit as usize {
        posts.truncate(limit as usize);
        posts.last().map(|post| post.occurred_at_ms)
    } else {
        None
    };
    Ok((posts, next))
}

/// Idempotent append: the same post_id with the same payload is a no-op,
/// a different payload fails. Keeps the newest posts per project.
pub(super) fn append_post(
    conn: &Connection,
    owner: &str,
    post: &FeedPost,
    now_ms: i64,
) -> anyhow::Result<bool> {
    post.validate(now_ms)?;
    ensure_schema(conn)?;
    let data = serde_json::to_string(post)?;
    let existing: Option<String> = conn
        .query_row(
            "SELECT post_json FROM workjet_project_feed_posts WHERE post_id=?1",
            [&post.post_id],
            |row| row.get(0),
        )
        .optional()?;
    if let Some(existing) = existing {
        ensure!(existing == data, "post_id was already used for a different post");
        return Ok(false);
    }
    conn.execute(
        "INSERT INTO workjet_project_feed_posts
           (post_id, project_id, owner_user_id, post_json, occurred_at_ms, created_at_ms)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        params![post.post_id, post.project_id, owner, data, post.occurred_at_ms, now_ms],
    )?;
    conn.execute(
        "DELETE FROM workjet_project_feed_posts WHERE project_id=?1 AND post_id NOT IN (
            SELECT post_id FROM workjet_project_feed_posts WHERE project_id=?1
             ORDER BY occurred_at_ms DESC, post_id DESC LIMIT ?2)",
        params![post.project_id, wire::RETENTION as i64],
    )?;
    Ok(true)
}

fn payload(command: &BusinessCommand) -> anyhow::Result<Value> {
    let mut value = command.payload.clone();
    let object = value.as_object_mut().context("feed payload must be an object")?;
    if let Some(channel) = object.remove("inbound_channel") {
        let channel = channel.as_str().context("inbound_channel must be a string")?;
        ensure!(
            !channel.trim().is_empty() && channel.chars().count() <= 256,
            "invalid inbound_channel"
        );
    }
    Ok(value)
}

pub(super) fn handle_command(root: &Path, command: &BusinessCommand, actor: &str) -> anyhow::Result<Value> {
    ensure!(command.command_type == READ_COMMAND, "unsupported project feed command");
    let request: ReadFeedRequest = serde_json::from_value(payload(command)?)?;
    request.validate()?;
    if let Some(record_id) = command.record_id.as_deref() {
        ensure!(record_id == request.project_id, "feed record_id must match the typed project_id");
    }
    let mut reader = Connection::open_with_flags(
        super::store::business_os_store_path(root),
        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY | rusqlite::OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )?;
    reader.busy_timeout(crate::persistence::sqlite_busy_timeout_duration())?;
    // Ownership and posts share one read snapshot; a read never creates the table.
    let snapshot = reader.transaction()?;
    let owner = super::workjet_project_kpis::require_project(&snapshot, actor, &request.project_id)?;
    let (posts, next) = read_posts(
        &snapshot,
        &request.project_id,
        &owner,
        request.before_ms,
        request.limit.unwrap_or(wire::DEFAULT_PAGE),
    )?;
    Ok(json!({"ok":true,"contract":CONTRACT,"posts":posts,"next_before_ms":next}))
}

#[cfg(test)]
#[path = "workjet_project_feed/tests.rs"]
mod tests;
