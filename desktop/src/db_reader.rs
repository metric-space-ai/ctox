//! Read-only access to CTOX SQLite databases for the desktop visualizations.
//!
//! Desktop reads the local Codex UI state databases plus CTOX's single runtime
//! database at `<root>/runtime/ctox.sqlite3`.

use std::path::{Path, PathBuf};
use rusqlite::{Connection, OpenFlags, params};

// ---------------------------------------------------------------------------
// Database paths
// ---------------------------------------------------------------------------

const STATE_DB_NAME: &str = "state_5.sqlite";
// Try newest version first, fall back to older
const LOGS_DB_NAMES: &[&str] = &["logs_2.sqlite", "logs_1.sqlite"];
const LCM_DB_NAME: &str = "runtime/ctox.sqlite3";
const AGENT_DB_NAME: &str = "runtime/ctox.sqlite3";

fn codex_home() -> Option<PathBuf> {
    if let Ok(val) = std::env::var("CODEX_HOME") {
        let p = PathBuf::from(val);
        if p.is_dir() {
            return Some(p);
        }
    }
    dirs::home_dir().map(|h| h.join(".codex"))
}

pub fn state_db_path(root: &Path) -> Option<PathBuf> {
    // Try root-local .codex first, then global codex_home
    let local = root.join(".codex").join(STATE_DB_NAME);
    if local.is_file() {
        return Some(local);
    }
    let global = codex_home()?.join(STATE_DB_NAME);
    if global.is_file() {
        return Some(global);
    }
    None
}

pub fn logs_db_path(root: &Path) -> Option<PathBuf> {
    for name in LOGS_DB_NAMES {
        let local = root.join(".codex").join(name);
        if local.is_file() {
            return Some(local);
        }
    }
    let home = codex_home()?;
    for name in LOGS_DB_NAMES {
        let global = home.join(name);
        if global.is_file() {
            return Some(global);
        }
    }
    None
}

pub fn lcm_db_path(root: &Path) -> Option<PathBuf> {
    let p = root.join(LCM_DB_NAME);
    if p.is_file() { Some(p) } else { None }
}

pub fn agent_db_path(root: &Path) -> Option<PathBuf> {
    let p = root.join(AGENT_DB_NAME);
    if p.is_file() { Some(p) } else { None }
}

fn open_readonly(path: &Path) -> Option<Connection> {
    Connection::open_with_flags(path, OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX).ok()
}

// ---------------------------------------------------------------------------
// Data types
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub struct ThreadRow {
    pub id: String,
    pub title: String,
    pub source: String,
    pub model_provider: String,
    pub model: Option<String>,
    pub tokens_used: i64,
    pub archived: bool,
    pub created_at: i64,
    pub updated_at: i64,
    pub git_branch: Option<String>,
    pub agent_nickname: Option<String>,
    pub agent_role: Option<String>,
    pub first_user_message: Option<String>,
    pub cli_version: Option<String>,
    pub reasoning_effort: Option<String>,
}

#[derive(Debug, Clone)]
pub struct JobRow {
    pub kind: String,
    pub job_key: String,
    pub status: String,
    pub worker_id: Option<String>,
    pub retry_remaining: i64,
    pub last_error: Option<String>,
    pub started_at: Option<i64>,
    pub finished_at: Option<i64>,
}

#[derive(Debug, Clone)]
pub struct AgentJobRow {
    pub id: String,
    pub name: String,
    pub status: String,
    pub instruction: String,
    pub created_at: i64,
    pub updated_at: i64,
    pub started_at: Option<i64>,
    pub completed_at: Option<i64>,
    pub last_error: Option<String>,
    pub total_items: i64,
    pub completed_items: i64,
    pub failed_items: i64,
}

#[derive(Debug, Clone)]
pub struct AgentJobItemRow {
    pub item_id: String,
    pub row_index: i64,
    pub status: String,
    pub source_id: Option<String>,
    pub attempt_count: i64,
    pub last_error: Option<String>,
    pub result_json: Option<String>,
}

#[derive(Debug, Clone)]
pub struct LogRow {
    pub id: i64,
    pub ts: i64,
    pub ts_nanos: i64,
    pub level: String,
    pub target: String,
    pub message: Option<String>,
    pub thread_id: Option<String>,
    pub process_uuid: Option<String>,
}

#[derive(Debug, Clone)]
pub struct Stage1Row {
    pub thread_id: String,
    pub thread_title: Option<String>,
    pub rollout_slug: Option<String>,
    pub raw_memory: String,
    pub rollout_summary: String,
    pub generated_at: i64,
    pub usage_count: Option<i64>,
    pub last_usage: Option<i64>,
    pub selected_for_phase2: bool,
}

#[derive(Debug, Clone)]
pub struct DynamicToolRow {
    pub thread_id: String,
    pub thread_title: Option<String>,
    pub position: i64,
    pub name: String,
    pub description: String,
    pub input_schema: String,
}

// LCM types

#[derive(Debug, Clone)]
pub struct LcmMessageRow {
    pub message_id: i64,
    pub conversation_id: i64,
    pub seq: i64,
    pub role: String,
    pub content: String,
    pub token_count: i64,
    pub created_at: String,
}

#[derive(Debug, Clone)]
pub struct LcmSummaryRow {
    pub summary_id: String,
    pub conversation_id: i64,
    pub kind: String,
    pub depth: i64,
    pub content: String,
    pub token_count: i64,
    pub descendant_count: i64,
    pub descendant_token_count: i64,
    pub created_at: String,
}

#[derive(Debug, Clone)]
pub struct SummaryEdge {
    pub parent_summary_id: String,
    pub child_summary_id: String,
}

#[derive(Debug, Clone)]
pub struct SummaryMessageLink {
    pub summary_id: String,
    pub message_id: i64,
}

#[derive(Debug, Clone)]
pub struct MissionStateRow {
    pub conversation_id: i64,
    pub mission: String,
    pub mission_status: String,
    pub continuation_mode: String,
    pub trigger_intensity: String,
    pub blocker: String,
    pub next_slice: String,
    pub done_gate: String,
    pub closure_confidence: String,
    pub is_open: bool,
    pub allow_idle: bool,
    pub last_synced_at: String,
}

#[derive(Debug, Clone)]
pub struct VerificationRunRow {
    pub run_id: String,
    pub conversation_id: i64,
    pub source_label: String,
    pub goal: String,
    pub preview: String,
    pub result_excerpt: String,
    pub blocker: Option<String>,
    pub review_verdict: String,
    pub review_score: i64,
    pub claim_count: i64,
    pub open_claim_count: i64,
    pub created_at: String,
}

#[derive(Debug, Clone)]
pub struct MissionClaimRow {
    pub claim_key: String,
    pub conversation_id: i64,
    pub claim_kind: String,
    pub claim_status: String,
    pub blocks_closure: bool,
    pub subject: String,
    pub summary: String,
    pub evidence_summary: String,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone)]
pub struct ContinuityDocRow {
    pub document_id: String,
    pub conversation_id: i64,
    pub kind: String,
    pub head_commit_id: String,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone)]
pub struct ContinuityCommitRow {
    pub commit_id: String,
    pub document_id: String,
    pub parent_commit_id: Option<String>,
    pub rendered_text: String,
    pub created_at: String,
}

#[derive(Debug, Clone)]
pub struct SecretRewriteRow {
    pub rewrite_id: String,
    pub conversation_id: i64,
    pub secret_scope: String,
    pub secret_name: String,
    pub message_rows_updated: i64,
    pub summary_rows_updated: i64,
    pub created_at: String,
}

#[derive(Debug, Clone)]
pub struct ContextItemRow {
    pub conversation_id: i64,
    pub ordinal: i64,
    pub item_type: String,
    pub message_id: Option<i64>,
    pub summary_id: Option<String>,
    pub created_at: String,
}

// ---------------------------------------------------------------------------
// Query functions — State DB
// ---------------------------------------------------------------------------

pub fn query_threads(root: &Path) -> Vec<ThreadRow> {
    let Some(path) = state_db_path(root) else { return Vec::new() };
    let Some(conn) = open_readonly(&path) else { return Vec::new() };
    // Filter threads to this installation by matching cwd against root path
    let root_str = root.to_string_lossy().to_string();
    let mut stmt = match conn.prepare(
        "SELECT id, title, source, model_provider, model, tokens_used, archived,
                created_at, updated_at, git_branch, agent_nickname, agent_role,
                first_user_message, cli_version, reasoning_effort
         FROM threads WHERE cwd LIKE ?1 ORDER BY updated_at DESC"
    ) {
        Ok(s) => s,
        Err(_) => return Vec::new(),
    };
    let pattern = format!("{}%", root_str);
    stmt.query_map(params![pattern], |row| {
        Ok(ThreadRow {
            id: row.get(0)?,
            title: row.get(1)?,
            source: row.get(2)?,
            model_provider: row.get(3)?,
            model: row.get(4)?,
            tokens_used: row.get(5)?,
            archived: row.get::<_, i64>(6)? != 0,
            created_at: row.get(7)?,
            updated_at: row.get(8)?,
            git_branch: row.get(9)?,
            agent_nickname: row.get(10)?,
            agent_role: row.get(11)?,
            first_user_message: row.get(12)?,
            cli_version: row.get(13)?,
            reasoning_effort: row.get(14)?,
        })
    })
    .ok()
    .map(|rows| rows.filter_map(|r| r.ok()).collect())
    .unwrap_or_default()
}

pub fn query_jobs(root: &Path) -> Vec<JobRow> {
    let Some(path) = state_db_path(root) else { return Vec::new() };
    let Some(conn) = open_readonly(&path) else { return Vec::new() };
    let mut stmt = match conn.prepare(
        "SELECT kind, job_key, status, worker_id, retry_remaining, last_error, started_at, finished_at
         FROM jobs ORDER BY kind, status"
    ) {
        Ok(s) => s,
        Err(_) => return Vec::new(),
    };
    stmt.query_map([], |row| {
        Ok(JobRow {
            kind: row.get(0)?,
            job_key: row.get(1)?,
            status: row.get(2)?,
            worker_id: row.get(3)?,
            retry_remaining: row.get(4)?,
            last_error: row.get(5)?,
            started_at: row.get(6)?,
            finished_at: row.get(7)?,
        })
    })
    .ok()
    .map(|rows| rows.filter_map(|r| r.ok()).collect())
    .unwrap_or_default()
}

pub fn query_agent_jobs(root: &Path) -> Vec<AgentJobRow> {
    let Some(path) = state_db_path(root) else { return Vec::new() };
    let Some(conn) = open_readonly(&path) else { return Vec::new() };
    let mut stmt = match conn.prepare(
        "SELECT aj.id, aj.name, aj.status, aj.instruction, aj.created_at, aj.updated_at,
                aj.started_at, aj.completed_at, aj.last_error,
                (SELECT COUNT(*) FROM agent_job_items WHERE job_id = aj.id) AS total,
                (SELECT COUNT(*) FROM agent_job_items WHERE job_id = aj.id AND status = 'completed') AS done,
                (SELECT COUNT(*) FROM agent_job_items WHERE job_id = aj.id AND status = 'failed') AS failed
         FROM agent_jobs aj ORDER BY aj.updated_at DESC"
    ) {
        Ok(s) => s,
        Err(_) => return Vec::new(),
    };
    stmt.query_map([], |row| {
        Ok(AgentJobRow {
            id: row.get(0)?,
            name: row.get(1)?,
            status: row.get(2)?,
            instruction: row.get(3)?,
            created_at: row.get(4)?,
            updated_at: row.get(5)?,
            started_at: row.get(6)?,
            completed_at: row.get(7)?,
            last_error: row.get(8)?,
            total_items: row.get(9)?,
            completed_items: row.get(10)?,
            failed_items: row.get(11)?,
        })
    })
    .ok()
    .map(|rows| rows.filter_map(|r| r.ok()).collect())
    .unwrap_or_default()
}

pub fn query_agent_job_items(root: &Path, job_id: &str) -> Vec<AgentJobItemRow> {
    let Some(path) = state_db_path(root) else { return Vec::new() };
    let Some(conn) = open_readonly(&path) else { return Vec::new() };
    let mut stmt = match conn.prepare(
        "SELECT item_id, row_index, status, source_id, attempt_count, last_error, result_json
         FROM agent_job_items WHERE job_id = ?1 ORDER BY row_index"
    ) {
        Ok(s) => s,
        Err(_) => return Vec::new(),
    };
    stmt.query_map(params![job_id], |row| {
        Ok(AgentJobItemRow {
            item_id: row.get(0)?,
            row_index: row.get(1)?,
            status: row.get(2)?,
            source_id: row.get(3)?,
            attempt_count: row.get(4)?,
            last_error: row.get(5)?,
            result_json: row.get(6)?,
        })
    })
    .ok()
    .map(|rows| rows.filter_map(|r| r.ok()).collect())
    .unwrap_or_default()
}

pub fn query_stage1_outputs(root: &Path) -> Vec<Stage1Row> {
    let Some(path) = state_db_path(root) else { return Vec::new() };
    let Some(conn) = open_readonly(&path) else { return Vec::new() };
    let mut stmt = match conn.prepare(
        "SELECT s.thread_id, t.title, s.rollout_slug, s.raw_memory, s.rollout_summary,
                s.generated_at, s.usage_count, s.last_usage, s.selected_for_phase2
         FROM stage1_outputs s
         LEFT JOIN threads t ON t.id = s.thread_id
         ORDER BY s.generated_at DESC"
    ) {
        Ok(s) => s,
        Err(_) => return Vec::new(),
    };
    stmt.query_map([], |row| {
        Ok(Stage1Row {
            thread_id: row.get(0)?,
            thread_title: row.get(1)?,
            rollout_slug: row.get(2)?,
            raw_memory: row.get(3)?,
            rollout_summary: row.get(4)?,
            generated_at: row.get(5)?,
            usage_count: row.get(6)?,
            last_usage: row.get(7)?,
            selected_for_phase2: row.get::<_, i64>(8).unwrap_or(0) != 0,
        })
    })
    .ok()
    .map(|rows| rows.filter_map(|r| r.ok()).collect())
    .unwrap_or_default()
}

pub fn query_dynamic_tools(root: &Path) -> Vec<DynamicToolRow> {
    let Some(path) = state_db_path(root) else { return Vec::new() };
    let Some(conn) = open_readonly(&path) else { return Vec::new() };
    let mut stmt = match conn.prepare(
        "SELECT dt.thread_id, t.title, dt.position, dt.name, dt.description, dt.input_schema
         FROM thread_dynamic_tools dt
         LEFT JOIN threads t ON t.id = dt.thread_id
         ORDER BY dt.thread_id, dt.position"
    ) {
        Ok(s) => s,
        Err(_) => return Vec::new(),
    };
    stmt.query_map([], |row| {
        Ok(DynamicToolRow {
            thread_id: row.get(0)?,
            thread_title: row.get(1)?,
            position: row.get(2)?,
            name: row.get(3)?,
            description: row.get(4)?,
            input_schema: row.get(5)?,
        })
    })
    .ok()
    .map(|rows| rows.filter_map(|r| r.ok()).collect())
    .unwrap_or_default()
}

// ---------------------------------------------------------------------------
// Query functions — Logs DB
// ---------------------------------------------------------------------------

pub fn query_logs(root: &Path, level_filter: Option<&str>, thread_filter: Option<&str>, limit: usize) -> Vec<LogRow> {
    let Some(path) = logs_db_path(root) else { return Vec::new() };
    let Some(conn) = open_readonly(&path) else { return Vec::new() };

    // Detect column name: logs_2 uses feedback_log_body, logs_1 uses message
    let msg_col = if conn.prepare("SELECT feedback_log_body FROM logs LIMIT 0").is_ok() {
        "feedback_log_body"
    } else {
        "message"
    };

    let mut sql = format!(
        "SELECT id, ts, ts_nanos, level, target, {msg_col}, thread_id, process_uuid FROM logs"
    );
    let mut conditions = Vec::new();
    if level_filter.is_some() {
        conditions.push("level = ?1".to_owned());
    }
    if thread_filter.is_some() {
        conditions.push("thread_id = ?2".to_owned());
    }
    if !conditions.is_empty() {
        sql.push_str(" WHERE ");
        sql.push_str(&conditions.join(" AND "));
    }
    sql.push_str(" ORDER BY ts DESC, ts_nanos DESC, id DESC LIMIT ?3");

    let mut stmt = match conn.prepare(&sql) {
        Ok(s) => s,
        Err(_) => return Vec::new(),
    };

    let level_val = level_filter.unwrap_or("");
    let thread_val = thread_filter.unwrap_or("");
    let limit_i64 = limit as i64;

    stmt.query_map(params![level_val, thread_val, limit_i64], |row| {
        Ok(LogRow {
            id: row.get(0)?,
            ts: row.get(1)?,
            ts_nanos: row.get(2)?,
            level: row.get(3)?,
            target: row.get(4)?,
            message: row.get(5)?,
            thread_id: row.get(6)?,
            process_uuid: row.get(7)?,
        })
    })
    .ok()
    .map(|rows| rows.filter_map(|r| r.ok()).collect())
    .unwrap_or_default()
}

// ---------------------------------------------------------------------------
// Query functions — LCM DB
// ---------------------------------------------------------------------------

pub fn query_lcm_messages(root: &Path, conversation_id: i64) -> Vec<LcmMessageRow> {
    let Some(path) = lcm_db_path(root) else { return Vec::new() };
    let Some(conn) = open_readonly(&path) else { return Vec::new() };
    // conversation_id 0 = load all conversations
    let sql = if conversation_id == 0 {
        "SELECT message_id, conversation_id, seq, role, content, token_count, created_at
         FROM messages ORDER BY conversation_id, seq"
    } else {
        "SELECT message_id, conversation_id, seq, role, content, token_count, created_at
         FROM messages WHERE conversation_id = ?1 ORDER BY seq"
    };
    let mut stmt = match conn.prepare(sql) {
        Ok(s) => s,
        Err(_) => return Vec::new(),
    };
    let mapper = |row: &rusqlite::Row| {
        Ok(LcmMessageRow {
            message_id: row.get(0)?,
            conversation_id: row.get(1)?,
            seq: row.get(2)?,
            role: row.get(3)?,
            content: row.get(4)?,
            token_count: row.get(5)?,
            created_at: row.get(6)?,
        })
    };
    if conversation_id == 0 {
        stmt.query_map([], mapper)
    } else {
        stmt.query_map(params![conversation_id], mapper)
    }
    .ok()
    .map(|rows| rows.filter_map(|r| r.ok()).collect())
    .unwrap_or_default()
}

pub fn query_lcm_summaries(root: &Path, conversation_id: i64) -> Vec<LcmSummaryRow> {
    let Some(path) = lcm_db_path(root) else { return Vec::new() };
    let Some(conn) = open_readonly(&path) else { return Vec::new() };
    let mut stmt = match conn.prepare(
        "SELECT summary_id, conversation_id, kind, depth, content, token_count,
                descendant_count, descendant_token_count, created_at
         FROM summaries WHERE conversation_id = ?1 ORDER BY depth DESC, created_at"
    ) {
        Ok(s) => s,
        Err(_) => return Vec::new(),
    };
    stmt.query_map(params![conversation_id], |row| {
        Ok(LcmSummaryRow {
            summary_id: row.get(0)?,
            conversation_id: row.get(1)?,
            kind: row.get(2)?,
            depth: row.get(3)?,
            content: row.get(4)?,
            token_count: row.get(5)?,
            descendant_count: row.get(6)?,
            descendant_token_count: row.get(7)?,
            created_at: row.get(8)?,
        })
    })
    .ok()
    .map(|rows| rows.filter_map(|r| r.ok()).collect())
    .unwrap_or_default()
}

pub fn query_summary_edges(root: &Path) -> Vec<SummaryEdge> {
    let Some(path) = lcm_db_path(root) else { return Vec::new() };
    let Some(conn) = open_readonly(&path) else { return Vec::new() };
    let mut stmt = match conn.prepare(
        "SELECT parent_summary_id, child_summary_id FROM summary_edges"
    ) {
        Ok(s) => s,
        Err(_) => return Vec::new(),
    };
    stmt.query_map([], |row| {
        Ok(SummaryEdge {
            parent_summary_id: row.get(0)?,
            child_summary_id: row.get(1)?,
        })
    })
    .ok()
    .map(|rows| rows.filter_map(|r| r.ok()).collect())
    .unwrap_or_default()
}

pub fn query_summary_messages(root: &Path) -> Vec<SummaryMessageLink> {
    let Some(path) = lcm_db_path(root) else { return Vec::new() };
    let Some(conn) = open_readonly(&path) else { return Vec::new() };
    let mut stmt = match conn.prepare(
        "SELECT summary_id, message_id FROM summary_messages"
    ) {
        Ok(s) => s,
        Err(_) => return Vec::new(),
    };
    stmt.query_map([], |row| {
        Ok(SummaryMessageLink {
            summary_id: row.get(0)?,
            message_id: row.get(1)?,
        })
    })
    .ok()
    .map(|rows| rows.filter_map(|r| r.ok()).collect())
    .unwrap_or_default()
}

pub fn query_mission_state(root: &Path) -> Vec<MissionStateRow> {
    let Some(path) = lcm_db_path(root) else { return Vec::new() };
    let Some(conn) = open_readonly(&path) else { return Vec::new() };
    let mut stmt = match conn.prepare(
        "SELECT conversation_id, mission, mission_status, continuation_mode, trigger_intensity,
                blocker, next_slice, done_gate, closure_confidence, is_open, allow_idle, last_synced_at
         FROM mission_states"
    ) {
        Ok(s) => s,
        Err(_) => return Vec::new(),
    };
    stmt.query_map([], |row| {
        Ok(MissionStateRow {
            conversation_id: row.get(0)?,
            mission: row.get(1)?,
            mission_status: row.get(2)?,
            continuation_mode: row.get(3)?,
            trigger_intensity: row.get(4)?,
            blocker: row.get(5)?,
            next_slice: row.get(6)?,
            done_gate: row.get(7)?,
            closure_confidence: row.get(8)?,
            is_open: row.get::<_, i64>(9)? != 0,
            allow_idle: row.get::<_, i64>(10)? != 0,
            last_synced_at: row.get(11)?,
        })
    })
    .ok()
    .map(|rows| rows.filter_map(|r| r.ok()).collect())
    .unwrap_or_default()
}

pub fn query_verification_runs(root: &Path) -> Vec<VerificationRunRow> {
    let Some(path) = lcm_db_path(root) else { return Vec::new() };
    let Some(conn) = open_readonly(&path) else { return Vec::new() };
    let mut stmt = match conn.prepare(
        "SELECT run_id, conversation_id, source_label, goal, preview, result_excerpt,
                blocker, review_verdict, review_score, claim_count, open_claim_count, created_at
         FROM verification_runs ORDER BY created_at DESC"
    ) {
        Ok(s) => s,
        Err(_) => return Vec::new(),
    };
    stmt.query_map([], |row| {
        Ok(VerificationRunRow {
            run_id: row.get(0)?,
            conversation_id: row.get(1)?,
            source_label: row.get(2)?,
            goal: row.get(3)?,
            preview: row.get(4)?,
            result_excerpt: row.get(5)?,
            blocker: row.get(6)?,
            review_verdict: row.get(7)?,
            review_score: row.get(8)?,
            claim_count: row.get(9)?,
            open_claim_count: row.get(10)?,
            created_at: row.get(11)?,
        })
    })
    .ok()
    .map(|rows| rows.filter_map(|r| r.ok()).collect())
    .unwrap_or_default()
}

pub fn query_mission_claims(root: &Path) -> Vec<MissionClaimRow> {
    let Some(path) = lcm_db_path(root) else { return Vec::new() };
    let Some(conn) = open_readonly(&path) else { return Vec::new() };
    let mut stmt = match conn.prepare(
        "SELECT claim_key, conversation_id, claim_kind, claim_status, blocks_closure,
                subject, summary, evidence_summary, created_at, updated_at
         FROM mission_claims ORDER BY updated_at DESC"
    ) {
        Ok(s) => s,
        Err(_) => return Vec::new(),
    };
    stmt.query_map([], |row| {
        Ok(MissionClaimRow {
            claim_key: row.get(0)?,
            conversation_id: row.get(1)?,
            claim_kind: row.get(2)?,
            claim_status: row.get(3)?,
            blocks_closure: row.get::<_, i64>(4)? != 0,
            subject: row.get(5)?,
            summary: row.get(6)?,
            evidence_summary: row.get(7)?,
            created_at: row.get(8)?,
            updated_at: row.get(9)?,
        })
    })
    .ok()
    .map(|rows| rows.filter_map(|r| r.ok()).collect())
    .unwrap_or_default()
}

pub fn query_continuity_documents(root: &Path) -> Vec<ContinuityDocRow> {
    let Some(path) = lcm_db_path(root) else { return Vec::new() };
    let Some(conn) = open_readonly(&path) else { return Vec::new() };
    let mut stmt = match conn.prepare(
        "SELECT document_id, conversation_id, kind, head_commit_id, created_at, updated_at
         FROM continuity_documents ORDER BY updated_at DESC"
    ) {
        Ok(s) => s,
        Err(_) => return Vec::new(),
    };
    stmt.query_map([], |row| {
        Ok(ContinuityDocRow {
            document_id: row.get(0)?,
            conversation_id: row.get(1)?,
            kind: row.get(2)?,
            head_commit_id: row.get(3)?,
            created_at: row.get(4)?,
            updated_at: row.get(5)?,
        })
    })
    .ok()
    .map(|rows| rows.filter_map(|r| r.ok()).collect())
    .unwrap_or_default()
}

pub fn query_continuity_commits(root: &Path, document_id: &str) -> Vec<ContinuityCommitRow> {
    let Some(path) = lcm_db_path(root) else { return Vec::new() };
    let Some(conn) = open_readonly(&path) else { return Vec::new() };
    let mut stmt = match conn.prepare(
        "SELECT commit_id, document_id, parent_commit_id, rendered_text, created_at
         FROM continuity_commits WHERE document_id = ?1 ORDER BY created_at DESC"
    ) {
        Ok(s) => s,
        Err(_) => return Vec::new(),
    };
    stmt.query_map(params![document_id], |row| {
        Ok(ContinuityCommitRow {
            commit_id: row.get(0)?,
            document_id: row.get(1)?,
            parent_commit_id: row.get(2)?,
            rendered_text: row.get(3)?,
            created_at: row.get(4)?,
        })
    })
    .ok()
    .map(|rows| rows.filter_map(|r| r.ok()).collect())
    .unwrap_or_default()
}

pub fn query_secret_rewrites(root: &Path) -> Vec<SecretRewriteRow> {
    let Some(path) = lcm_db_path(root) else { return Vec::new() };
    let Some(conn) = open_readonly(&path) else { return Vec::new() };
    let mut stmt = match conn.prepare(
        "SELECT rewrite_id, conversation_id, secret_scope, secret_name,
                message_rows_updated, summary_rows_updated, created_at
         FROM secret_rewrites ORDER BY created_at DESC"
    ) {
        Ok(s) => s,
        Err(_) => return Vec::new(),
    };
    stmt.query_map([], |row| {
        Ok(SecretRewriteRow {
            rewrite_id: row.get(0)?,
            conversation_id: row.get(1)?,
            secret_scope: row.get(2)?,
            secret_name: row.get(3)?,
            message_rows_updated: row.get(4)?,
            summary_rows_updated: row.get(5)?,
            created_at: row.get(6)?,
        })
    })
    .ok()
    .map(|rows| rows.filter_map(|r| r.ok()).collect())
    .unwrap_or_default()
}

pub fn query_context_items(root: &Path, conversation_id: i64) -> Vec<ContextItemRow> {
    let Some(path) = lcm_db_path(root) else { return Vec::new() };
    let Some(conn) = open_readonly(&path) else { return Vec::new() };
    let mut stmt = match conn.prepare(
        "SELECT conversation_id, ordinal, item_type, message_id, summary_id, created_at
         FROM context_items WHERE conversation_id = ?1 ORDER BY ordinal"
    ) {
        Ok(s) => s,
        Err(_) => return Vec::new(),
    };
    stmt.query_map(params![conversation_id], |row| {
        Ok(ContextItemRow {
            conversation_id: row.get(0)?,
            ordinal: row.get(1)?,
            item_type: row.get(2)?,
            message_id: row.get(3)?,
            summary_id: row.get(4)?,
            created_at: row.get(5)?,
        })
    })
    .ok()
    .map(|rows| rows.filter_map(|r| r.ok()).collect())
    .unwrap_or_default()
}

// ---------------------------------------------------------------------------
// Runtime DB types and queries (ctox.sqlite3)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub struct TicketItemRow {
    pub ticket_key: String,
    pub source_system: String,
    pub remote_ticket_id: String,
    pub title: String,
    pub body_text: String,
    pub remote_status: String,
    pub priority: Option<String>,
    pub requester: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone)]
pub struct TicketCaseRow {
    pub case_id: String,
    pub ticket_key: String,
    pub label: String,
    pub bundle_label: String,
    pub state: String,
    pub approval_mode: String,
    pub autonomy_level: String,
    pub support_mode: String,
    pub risk_level: String,
    pub opened_at: String,
    pub updated_at: String,
    pub closed_at: Option<String>,
}

#[derive(Debug, Clone)]
pub struct CommMessageRow {
    pub message_key: String,
    pub channel: String,
    pub subject: String,
    pub body_text: String,
    pub sender_display: Option<String>,
    pub direction: String,
    pub route_status: Option<String>,
    pub observed_at: String,
}

pub fn query_ticket_items(root: &Path) -> Vec<TicketItemRow> {
    let Some(path) = agent_db_path(root) else { return Vec::new() };
    let Some(conn) = open_readonly(&path) else { return Vec::new() };
    let mut stmt = match conn.prepare(
        "SELECT ticket_key, source_system, remote_ticket_id, title, body_text,
                remote_status, priority, requester, created_at, updated_at
         FROM ticket_items ORDER BY updated_at DESC"
    ) {
        Ok(s) => s,
        Err(_) => return Vec::new(),
    };
    stmt.query_map([], |row| {
        Ok(TicketItemRow {
            ticket_key: row.get(0)?,
            source_system: row.get(1)?,
            remote_ticket_id: row.get(2)?,
            title: row.get(3)?,
            body_text: row.get(4)?,
            remote_status: row.get(5)?,
            priority: row.get(6)?,
            requester: row.get(7)?,
            created_at: row.get(8)?,
            updated_at: row.get(9)?,
        })
    })
    .ok()
    .map(|rows| rows.filter_map(|r| r.ok()).collect())
    .unwrap_or_default()
}

pub fn query_ticket_cases(root: &Path) -> Vec<TicketCaseRow> {
    let Some(path) = agent_db_path(root) else { return Vec::new() };
    let Some(conn) = open_readonly(&path) else { return Vec::new() };
    let mut stmt = match conn.prepare(
        "SELECT case_id, ticket_key, label, bundle_label, state, approval_mode,
                autonomy_level, support_mode, risk_level, opened_at, updated_at, closed_at
         FROM ticket_cases ORDER BY updated_at DESC"
    ) {
        Ok(s) => s,
        Err(_) => return Vec::new(),
    };
    stmt.query_map([], |row| {
        Ok(TicketCaseRow {
            case_id: row.get(0)?,
            ticket_key: row.get(1)?,
            label: row.get(2)?,
            bundle_label: row.get(3)?,
            state: row.get(4)?,
            approval_mode: row.get(5)?,
            autonomy_level: row.get(6)?,
            support_mode: row.get(7)?,
            risk_level: row.get(8)?,
            opened_at: row.get(9)?,
            updated_at: row.get(10)?,
            closed_at: row.get(11)?,
        })
    })
    .ok()
    .map(|rows| rows.filter_map(|r| r.ok()).collect())
    .unwrap_or_default()
}

pub fn query_comm_messages(root: &Path) -> Vec<CommMessageRow> {
    let Some(path) = agent_db_path(root) else { return Vec::new() };
    let Some(conn) = open_readonly(&path) else { return Vec::new() };
    let mut stmt = match conn.prepare(
        "SELECT cm.message_key, cm.channel, cm.subject, cm.body_text,
                cm.sender_display, cm.direction,
                cr.route_status, cm.observed_at
         FROM communication_messages cm
         LEFT JOIN communication_routing_state cr ON cm.message_key = cr.message_key
         ORDER BY cm.observed_at DESC"
    ) {
        Ok(s) => s,
        Err(_) => return Vec::new(),
    };
    stmt.query_map([], |row| {
        Ok(CommMessageRow {
            message_key: row.get(0)?,
            channel: row.get(1)?,
            subject: row.get(2)?,
            body_text: row.get(3)?,
            sender_display: row.get(4)?,
            direction: row.get(5)?,
            route_status: row.get(6)?,
            observed_at: row.get(7)?,
        })
    })
    .ok()
    .map(|rows| rows.filter_map(|r| r.ok()).collect())
    .unwrap_or_default()
}

#[derive(Debug, Clone)]
pub struct ExecutionActionRow {
    pub action_id: String,
    pub case_id: String,
    pub ticket_key: String,
    pub summary: String,
    pub created_at: String,
}

pub fn query_execution_actions(root: &Path) -> Vec<ExecutionActionRow> {
    let Some(path) = agent_db_path(root) else { return Vec::new() };
    let Some(conn) = open_readonly(&path) else { return Vec::new() };
    let mut stmt = match conn.prepare(
        "SELECT action_id, case_id, ticket_key, summary, created_at
         FROM ticket_execution_actions ORDER BY created_at DESC"
    ) {
        Ok(s) => s,
        Err(_) => return Vec::new(),
    };
    stmt.query_map([], |row| {
        Ok(ExecutionActionRow {
            action_id: row.get(0)?,
            case_id: row.get(1)?,
            ticket_key: row.get(2)?,
            summary: row.get(3)?,
            created_at: row.get(4)?,
        })
    })
    .ok()
    .map(|rows| rows.filter_map(|r| r.ok()).collect())
    .unwrap_or_default()
}

#[derive(Debug, Clone)]
struct HarnessFlowMessage {
    message_key: String,
    channel: String,
    direction: String,
    thread_key: String,
    subject: String,
    preview: String,
    body_text: String,
    sender_display: String,
    observed_at: String,
    route_status: Option<String>,
    acked_at: Option<String>,
    updated_at: Option<String>,
}

#[derive(Debug, Clone)]
struct HarnessFlowWork {
    work_id: String,
    kind: String,
    title: String,
    body_text: String,
    state: String,
    created_at: String,
    updated_at: String,
}

pub fn query_harness_flow_text(root: &Path, width: usize) -> String {
    let Some(path) = agent_db_path(root) else {
        return "Harness flow unavailable.\n\nNo runtime/ctox.sqlite3 database found.".to_string();
    };
    let Some(conn) = open_readonly(&path) else {
        return format!("Harness flow unavailable.\n\nCould not open {}.", path.display());
    };
    let Some(message) = latest_harness_flow_message(&conn) else {
        return render_flow_boxes(
            width,
            vec![FlowBlock {
                title: "NO FLOW SOURCE FOUND".to_string(),
                lines: vec![
                    "No communication message exists in runtime/ctox.sqlite3 yet.".to_string(),
                    "Once queue or ticket activity is present, this view becomes the live flow.".to_string(),
                ],
                branches: Vec::new(),
            }],
        );
    };

    let related_work = related_harness_flow_work(&conn, &message.message_key);
    let review_summary = optional_string(
        &conn,
        "SELECT review_summary FROM communication_founder_reply_reviews
         WHERE inbound_message_key = ?1 ORDER BY approved_at DESC LIMIT 1",
        &message.message_key,
    );
    let ticket_count = optional_count(&conn, "SELECT COUNT(*) FROM ticket_items");
    let self_work_count = optional_count(&conn, "SELECT COUNT(*) FROM ticket_self_work_items");
    let knowledge_entries = optional_count(&conn, "SELECT COUNT(*) FROM ticket_knowledge_entries");
    let continuity_docs = optional_count(&conn, "SELECT COUNT(*) FROM continuity_documents");
    let continuity_commits = optional_count(&conn, "SELECT COUNT(*) FROM continuity_commits");
    let verification_runs = optional_count(&conn, "SELECT COUNT(*) FROM verification_runs");
    let ticket_verifications = optional_count(&conn, "SELECT COUNT(*) FROM ticket_verifications");

    let preview = first_non_empty(&[&message.preview, &message.body_text]).unwrap_or("");
    let mut blocks = Vec::new();
    blocks.push(FlowBlock {
        title: "TASK".to_string(),
        lines: vec![
            format!(
                "{} from {}",
                sentence_case(&message.direction),
                non_empty(&message.sender_display, "unknown sender")
            ),
            format!("Subject: {}", clip(non_empty(&message.subject, "(no subject)"), 82)),
            format!("What CTOX has to handle: {}", clip(preview, 82)),
            format!(
                "Source: {} · thread {} · observed {}",
                message.channel,
                clip(&message.thread_key, 38),
                short_time(&message.observed_at)
            ),
        ],
        branches: vec![
            FlowBranch {
                title: "QUEUE PICKUP".to_string(),
                lines: vec![
                    format!("Current queue state: {}", message.route_status.as_deref().unwrap_or("unknown")),
                    format!("Acknowledged: {}", message.acked_at.as_deref().map(short_time).unwrap_or_else(|| "not yet".to_string())),
                    format!("Last queue update: {}", message.updated_at.as_deref().map(short_time).unwrap_or_else(|| "unknown".to_string())),
                ],
                returns_to_spine: true,
            },
            FlowBranch {
                title: "CONTEXT".to_string(),
                lines: vec![
                    format!("Continuity docs: {continuity_docs} · commits: {continuity_commits}"),
                    "Purpose: keep the worker on the current task context.".to_string(),
                ],
                returns_to_spine: true,
            },
            FlowBranch {
                title: "KNOWLEDGE".to_string(),
                lines: vec![
                    format!("Captured knowledge entries: {knowledge_entries}"),
                    "Shown here when ticket knowledge is written into the runtime DB.".to_string(),
                ],
                returns_to_spine: true,
            },
        ],
    });

    let mut attempt_branches = Vec::new();
    if let Some(summary) = review_summary.filter(|s| !s.trim().is_empty()) {
        attempt_branches.push(FlowBranch {
            title: "REVIEW".to_string(),
            lines: vec![
                "Result: send allowed.".to_string(),
                format!("Review summary: {}", clip(&summary, 76)),
            ],
            returns_to_spine: true,
        });
    } else if let Some(work) = related_work.first() {
        attempt_branches.push(FlowBranch {
            title: "REVIEW".to_string(),
            lines: vec![
                "Result: not finished; durable rework exists.".to_string(),
                format!("Rework item: {}", clip(&work.title, 76)),
                format!("Reason/work requested: {}", clip(&work.body_text, 76)),
            ],
            returns_to_spine: false,
        });
        attempt_branches.push(FlowBranch {
            title: "TICKET BACKLOG".to_string(),
            lines: vec![
                format!("Created: {} · {}", clip(&work.work_id, 24), clip(&work.title, 56)),
                format!("State: {} · kind: {}", work.state, work.kind),
                format!("Runtime totals: tickets {ticket_count} · self-work {self_work_count}"),
            ],
            returns_to_spine: false,
        });
    } else {
        attempt_branches.push(FlowBranch {
            title: "REVIEW".to_string(),
            lines: vec![
                "No persisted review result found for this source.".to_string(),
                "If a review happened, the flow needs that outcome captured durably.".to_string(),
            ],
            returns_to_spine: true,
        });
    }

    blocks.push(FlowBlock {
        title: "ATTEMPT 1".to_string(),
        lines: vec![
            "CTOX works on the first answer or slice.".to_string(),
            format!("Input: {}", clip(preview, 82)),
            "Work metrics: not instrumented yet (files/line deltas need a turn diff ledger).".to_string(),
        ],
        branches: attempt_branches,
    });

    if let Some(work) = related_work.first() {
        blocks.push(FlowBlock {
            title: "ATTEMPT 2".to_string(),
            lines: vec![
                "CTOX resumes from durable rework and continues the same task.".to_string(),
                format!("Picked up: {} ({})", clip(&work.title, 70), work.kind),
                format!("Backlog state: {} · updated {}", work.state, short_time(&work.updated_at)),
            ],
            branches: vec![FlowBranch {
                title: "SOURCE FROM TICKET BACKLOG".to_string(),
                lines: vec![
                    format!("Picked up work item: {}", clip(&work.work_id, 48)),
                    format!("State: {} · created {}", work.state, short_time(&work.created_at)),
                ],
                returns_to_spine: true,
            }],
        });
    }

    blocks.push(FlowBlock {
        title: "FINISH / CURRENT STATE".to_string(),
        lines: vec![
            format!("Original queue state: {}", message.route_status.as_deref().unwrap_or("unknown")),
            format!("Runtime totals: tickets {ticket_count} · self-work {self_work_count}"),
        ],
        branches: vec![
            FlowBranch {
                title: "SEND / CLOSE GUARD".to_string(),
                lines: vec![
                    "Core transition proof details are shown when linked to this source.".to_string(),
                    "Rejected transitions and state violations should branch here.".to_string(),
                ],
                returns_to_spine: true,
            },
            FlowBranch {
                title: "VERIFICATION".to_string(),
                lines: vec![
                    format!("Verification runs in runtime: {verification_runs}"),
                    format!("Ticket verification records: {ticket_verifications}"),
                ],
                returns_to_spine: true,
            },
        ],
    });

    render_flow_boxes(width, blocks)
}

fn latest_harness_flow_message(conn: &Connection) -> Option<HarnessFlowMessage> {
    conn.query_row(
        "SELECT cm.message_key, cm.channel, cm.direction, cm.thread_key, cm.subject,
                cm.preview, cm.body_text, cm.sender_display, cm.observed_at,
                cr.route_status, cr.acked_at, cr.updated_at
         FROM communication_messages cm
         LEFT JOIN communication_routing_state cr ON cm.message_key = cr.message_key
         ORDER BY cm.observed_at DESC LIMIT 1",
        [],
        |row| {
            Ok(HarnessFlowMessage {
                message_key: row.get(0)?,
                channel: row.get(1)?,
                direction: row.get(2)?,
                thread_key: row.get(3)?,
                subject: row.get(4)?,
                preview: row.get(5)?,
                body_text: row.get(6)?,
                sender_display: row.get(7)?,
                observed_at: row.get(8)?,
                route_status: row.get(9)?,
                acked_at: row.get(10)?,
                updated_at: row.get(11)?,
            })
        },
    )
    .ok()
}

fn related_harness_flow_work(conn: &Connection, message_key: &str) -> Vec<HarnessFlowWork> {
    let mut stmt = match conn.prepare(
        "SELECT work_id, kind, title, body_text, state, created_at, updated_at
         FROM ticket_self_work_items
         WHERE json_extract(metadata_json, '$.parent_message_key') = ?1
            OR json_extract(metadata_json, '$.inbound_message_key') = ?1
            OR metadata_json LIKE '%' || ?1 || '%'
         ORDER BY created_at ASC LIMIT 4",
    ) {
        Ok(stmt) => stmt,
        Err(_) => return Vec::new(),
    };
    stmt.query_map(params![message_key], |row| {
        Ok(HarnessFlowWork {
            work_id: row.get(0)?,
            kind: row.get(1)?,
            title: row.get(2)?,
            body_text: row.get(3)?,
            state: row.get(4)?,
            created_at: row.get(5)?,
            updated_at: row.get(6)?,
        })
    })
    .ok()
    .map(|rows| rows.filter_map(|row| row.ok()).collect())
    .unwrap_or_default()
}

fn optional_string(conn: &Connection, sql: &str, param: &str) -> Option<String> {
    conn.query_row(sql, params![param], |row| row.get(0)).ok()
}

fn optional_count(conn: &Connection, sql: &str) -> i64 {
    conn.query_row(sql, [], |row| row.get(0)).unwrap_or(0)
}

#[derive(Debug)]
struct FlowBlock {
    title: String,
    lines: Vec<String>,
    branches: Vec<FlowBranch>,
}

#[derive(Debug)]
struct FlowBranch {
    title: String,
    lines: Vec<String>,
    returns_to_spine: bool,
}

fn render_flow_boxes(width: usize, blocks: Vec<FlowBlock>) -> String {
    let width = width.clamp(92, 180);
    let main_width = (width * 54 / 100).clamp(50, 82);
    let branch_width = width.saturating_sub(main_width + 8).clamp(34, 86);
    let mut out = String::new();
    for (index, block) in blocks.iter().enumerate() {
        render_box(&mut out, "", main_width, &block.title, &block.lines);
        for branch in &block.branches {
            let stem_pad = " ".repeat(main_width / 2);
            out.push_str(&format!("{stem_pad}│\n"));
            render_branch_box(&mut out, &stem_pad, branch_width, &branch.title, &branch.lines);
            if branch.returns_to_spine {
                out.push_str(&format!("{stem_pad}│\n"));
            }
        }
        if index + 1 < blocks.len() {
            out.push_str(&format!("{}│\n{}▼\n", " ".repeat(main_width / 2), " ".repeat(main_width / 2)));
        }
    }
    out.trim_end().to_string()
}

fn render_box(out: &mut String, prefix: &str, width: usize, title: &str, lines: &[String]) {
    let inner = width.saturating_sub(2);
    out.push_str(prefix);
    out.push('┌');
    out.push_str(&"─".repeat(inner));
    out.push_str("┐\n");
    render_box_line(out, prefix, inner, title);
    for line in lines {
        for wrapped in wrap_line(line, inner.saturating_sub(2)) {
            render_box_line(out, prefix, inner, &format!("  {wrapped}"));
        }
    }
    out.push_str(prefix);
    out.push('└');
    out.push_str(&"─".repeat(inner));
    out.push_str("┘\n");
}

fn render_branch_box(out: &mut String, stem_pad: &str, width: usize, title: &str, lines: &[String]) {
    let mut rendered = String::new();
    render_box(&mut rendered, "", width, title, lines);
    for (idx, line) in rendered.lines().enumerate() {
        out.push_str(stem_pad);
        if idx == 0 {
            out.push_str("├──►");
        } else {
            out.push_str("│   ");
        }
        out.push_str(line);
        out.push('\n');
    }
}

fn render_box_line(out: &mut String, prefix: &str, inner: usize, text: &str) {
    let clipped = clip(text, inner);
    out.push_str(prefix);
    out.push('│');
    out.push_str(&clipped);
    out.push_str(&" ".repeat(inner.saturating_sub(clipped.chars().count())));
    out.push_str("│\n");
}

fn wrap_line(text: &str, width: usize) -> Vec<String> {
    if text.chars().count() <= width {
        return vec![text.to_string()];
    }
    let mut lines = Vec::new();
    let mut current = String::new();
    for word in text.split_whitespace() {
        let next_len = current.chars().count() + if current.is_empty() { 0 } else { 1 } + word.chars().count();
        if next_len > width && !current.is_empty() {
            lines.push(current);
            current = word.to_string();
        } else {
            if !current.is_empty() {
                current.push(' ');
            }
            current.push_str(word);
        }
    }
    if !current.is_empty() {
        lines.push(current);
    }
    if lines.is_empty() {
        vec![clip(text, width)]
    } else {
        lines
    }
}

fn first_non_empty<'a>(values: &[&'a str]) -> Option<&'a str> {
    values.iter().copied().find(|value| !value.trim().is_empty())
}

fn non_empty<'a>(value: &'a str, fallback: &'a str) -> &'a str {
    if value.trim().is_empty() { fallback } else { value }
}

fn sentence_case(value: &str) -> String {
    let mut chars = value.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
        None => String::new(),
    }
}

fn short_time(value: &str) -> String {
    value
        .split('T')
        .nth(1)
        .map(|time| time.trim_end_matches('Z').to_string())
        .unwrap_or_else(|| value.to_string())
}

fn clip(value: &str, max: usize) -> String {
    let cleaned = value.split_whitespace().collect::<Vec<_>>().join(" ");
    if cleaned.chars().count() <= max {
        cleaned
    } else {
        let take = max.saturating_sub(3);
        format!("{}...", cleaned.chars().take(take).collect::<String>())
    }
}
