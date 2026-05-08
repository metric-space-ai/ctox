//! SQLite schema for the deep-research backend.
//!
//! Modelled on the Förderantrag agent's workspace state plus a few
//! report-specific extensions (per-run `report_runs`, separate
//! `report_skill_runs` provenance, `report_research_log`, `report_check_runs`
//! gate). All tables are created with `CREATE TABLE IF NOT EXISTS`; a single
//! [`ensure_schema`] entry point is the only public surface for migration.
//!
//! The DB path is the same consolidated core store every other CTOX
//! subsystem uses: [`crate::paths::core_db`]. Connections are opened with
//! the WAL + busy_timeout PRAGMA pattern shared with `persistence.rs` and
//! `mission/plan.rs`.

use std::path::Path;

use anyhow::{anyhow, Context, Result};
use rusqlite::Connection;

use crate::paths::core_db;
use crate::persistence::{sqlite_busy_timeout_duration, sqlite_busy_timeout_millis};

/// Maximum number of instance_ids the manager may pass into a single
/// `write_with_skill` or `revise_with_skill` call. Schema-enforced.
pub const MAX_BLOCKS_PER_SKILL_CALL: usize = 6;

/// Maximum number of blocking_questions a sub-skill may emit per call.
pub const MAX_BLOCKING_QUESTIONS: usize = 3;

/// Schema version. Bumped when a non-additive migration is required.
pub const SCHEMA_VERSION: &str = "v1";

/// Run lifecycle. The transitions are forward-only with a single allowed
/// loop: `Reviewing <-> Revising`. See [`crate::report::state::transition_to`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RunStatus {
    Created,
    Scoping,
    Researching,
    Drafting,
    Reviewing,
    Revising,
    Checking,
    Rendering,
    Finalised,
    Aborted,
}

impl RunStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            RunStatus::Created => "created",
            RunStatus::Scoping => "scoping",
            RunStatus::Researching => "researching",
            RunStatus::Drafting => "drafting",
            RunStatus::Reviewing => "reviewing",
            RunStatus::Revising => "revising",
            RunStatus::Checking => "checking",
            RunStatus::Rendering => "rendering",
            RunStatus::Finalised => "finalised",
            RunStatus::Aborted => "aborted",
        }
    }

    pub fn parse(value: &str) -> Result<Self> {
        match value {
            "created" => Ok(RunStatus::Created),
            "scoping" => Ok(RunStatus::Scoping),
            "researching" => Ok(RunStatus::Researching),
            "drafting" => Ok(RunStatus::Drafting),
            "reviewing" => Ok(RunStatus::Reviewing),
            "revising" => Ok(RunStatus::Revising),
            "checking" => Ok(RunStatus::Checking),
            "rendering" => Ok(RunStatus::Rendering),
            "finalised" => Ok(RunStatus::Finalised),
            "aborted" => Ok(RunStatus::Aborted),
            other => Err(anyhow!("unknown report run status: {other}")),
        }
    }

    /// Numeric rank for forward-progress checks. Equal-rank transitions
    /// (e.g. `Reviewing` <-> `Revising`) are allowed by design.
    pub fn rank(self) -> u8 {
        match self {
            RunStatus::Created => 0,
            RunStatus::Scoping => 1,
            RunStatus::Researching => 2,
            RunStatus::Drafting => 3,
            // Review and Revise share a rank — they are explicitly allowed
            // to oscillate while the manager iterates.
            RunStatus::Reviewing => 4,
            RunStatus::Revising => 4,
            RunStatus::Checking => 5,
            RunStatus::Rendering => 6,
            RunStatus::Finalised => 7,
            RunStatus::Aborted => 8,
        }
    }
}

/// Open the consolidated core DB and apply the standard PRAGMA. Foreign
/// keys are turned on so the per-run `ON DELETE CASCADE` clauses work.
pub fn open(root: &Path) -> Result<Connection> {
    let path = core_db(root);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("failed to create report db parent {}", parent.display()))?;
    }
    let conn = Connection::open(&path)
        .with_context(|| format!("failed to open report db {}", path.display()))?;
    conn.busy_timeout(sqlite_busy_timeout_duration())
        .context("failed to configure SQLite busy_timeout for report")?;
    let busy_timeout_ms = sqlite_busy_timeout_millis();
    conn.execute_batch(&format!(
        "PRAGMA journal_mode=WAL;
         PRAGMA busy_timeout={busy_timeout_ms};
         PRAGMA foreign_keys=ON;"
    ))
    .context("failed to apply core PRAGMA for report db")?;
    Ok(conn)
}

/// Idempotent schema creation. Safe to call on every process start.
pub fn ensure_schema(conn: &Connection) -> Result<()> {
    conn.execute_batch(SCHEMA_SQL)
        .context("failed to ensure deep-research report schema")?;
    // Soft migrations for tables created by an older binary that did not
    // know about the canonical column set. Each ALTER TABLE is idempotent
    // (we swallow "duplicate column" errors).
    migrate_add_column(conn, "report_evidence_register", "raw_payload_json", "TEXT");
    migrate_add_column(conn, "report_evidence_register", "created_at", "TEXT");
    migrate_add_column(conn, "report_evidence_register", "updated_at", "TEXT");
    migrate_add_column(conn, "report_evidence_register", "full_text_md", "TEXT");
    migrate_add_column(conn, "report_evidence_register", "full_text_source", "TEXT");
    migrate_add_column(conn, "report_evidence_register", "full_text_chars", "INTEGER");
    // Storyline lives directly on the run row — single-source-of-truth
    // narrative spine.
    migrate_add_column(conn, "report_runs", "storyline_md", "TEXT");
    migrate_add_column(conn, "report_runs", "storyline_set_at", "TEXT");
    // arc_position: dramatic position of the block in the document arc
    // ('tension_open' | 'tension_deepen' | 'complication' | 'turning_point'
    //  | 'resolution_construct' | 'resolution_ratify' | 'support').
    migrate_add_column(conn, "report_blocks", "arc_position", "TEXT");
    migrate_add_column(conn, "report_pending_blocks", "arc_position", "TEXT");
    Ok(())
}

fn migrate_add_column(conn: &Connection, table: &str, column: &str, decl: &str) {
    let sql = format!("ALTER TABLE {table} ADD COLUMN {column} {decl}");
    let _ = conn.execute(&sql, []);
}

/// Current UTC timestamp in RFC3339, used as the canonical clock for
/// every `*_at` column.
pub fn now_iso() -> String {
    chrono::Utc::now().to_rfc3339()
}

/// Allocate an opaque id with the given short prefix. The body is a
/// simple uuid v4 with hyphens stripped — collisions are vanishingly
/// unlikely and the format matches what `mission/plan.rs` and the rest
/// of CTOX use for opaque ids.
pub fn new_id(prefix: &str) -> String {
    let body = uuid::Uuid::new_v4().simple().to_string();
    if prefix.is_empty() {
        body
    } else {
        format!("{prefix}_{body}")
    }
}

const SCHEMA_SQL: &str = r#"
CREATE TABLE IF NOT EXISTS report_runs (
    run_id              TEXT PRIMARY KEY,
    report_type_id      TEXT NOT NULL,
    domain_profile_id   TEXT NOT NULL,
    depth_profile_id    TEXT NOT NULL,
    style_profile_id    TEXT NOT NULL,
    language            TEXT NOT NULL,
    status              TEXT NOT NULL,
    started_at          TEXT NOT NULL,
    finished_at         TEXT,
    raw_topic           TEXT NOT NULL,
    package_summary_json TEXT
);

CREATE INDEX IF NOT EXISTS idx_report_runs_status
    ON report_runs(status, started_at DESC);
CREATE INDEX IF NOT EXISTS idx_report_runs_type
    ON report_runs(report_type_id, started_at DESC);

CREATE TABLE IF NOT EXISTS report_blocks (
    block_pk            INTEGER PRIMARY KEY AUTOINCREMENT,
    run_id              TEXT NOT NULL,
    instance_id         TEXT NOT NULL,
    doc_id              TEXT NOT NULL,
    block_id            TEXT NOT NULL,
    block_template_id   TEXT,
    title               TEXT NOT NULL,
    ord                 INTEGER NOT NULL,
    markdown            TEXT NOT NULL,
    reason              TEXT,
    used_skill_ids_json     TEXT,
    used_research_ids_json  TEXT,
    used_reference_ids_json TEXT,
    committed_at        TEXT NOT NULL,
    FOREIGN KEY (run_id) REFERENCES report_runs(run_id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_report_blocks_run
    ON report_blocks(run_id, ord ASC);
CREATE INDEX IF NOT EXISTS idx_report_blocks_run_instance
    ON report_blocks(run_id, instance_id);
CREATE INDEX IF NOT EXISTS idx_report_blocks_run_doc
    ON report_blocks(run_id, doc_id, ord ASC);

CREATE TABLE IF NOT EXISTS report_pending_blocks (
    pending_pk          INTEGER PRIMARY KEY AUTOINCREMENT,
    run_id              TEXT NOT NULL,
    skill_run_id        TEXT NOT NULL,
    kind                TEXT NOT NULL,
    instance_id         TEXT NOT NULL,
    doc_id              TEXT NOT NULL,
    block_id            TEXT NOT NULL,
    block_template_id   TEXT,
    title               TEXT NOT NULL,
    ord                 INTEGER NOT NULL,
    markdown            TEXT NOT NULL,
    reason              TEXT,
    used_skill_ids_json     TEXT,
    used_research_ids_json  TEXT,
    used_reference_ids_json TEXT,
    committed_at        TEXT NOT NULL,
    FOREIGN KEY (run_id) REFERENCES report_runs(run_id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_report_pending_blocks_run
    ON report_pending_blocks(run_id, skill_run_id);
CREATE INDEX IF NOT EXISTS idx_report_pending_blocks_skill
    ON report_pending_blocks(skill_run_id);

CREATE TABLE IF NOT EXISTS report_evidence_register (
    evidence_id         TEXT PRIMARY KEY,
    run_id              TEXT NOT NULL,
    kind                TEXT NOT NULL,
    canonical_id        TEXT,
    title               TEXT,
    authors_json        TEXT,
    venue               TEXT,
    year                INTEGER,
    publisher           TEXT,
    url_canonical       TEXT,
    url_full_text       TEXT,
    license             TEXT,
    abstract_md         TEXT,
    snippet_md          TEXT,
    full_text_md        TEXT,
    full_text_source    TEXT,
    full_text_chars     INTEGER,
    retrieved_at        TEXT,
    resolver_used       TEXT,
    integrity_hash      TEXT,
    raw_payload_json    TEXT,
    created_at          TEXT,
    updated_at          TEXT,
    citations_count     INTEGER NOT NULL DEFAULT 0,
    FOREIGN KEY (run_id) REFERENCES report_runs(run_id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_report_evidence_register_run
    ON report_evidence_register(run_id, retrieved_at DESC);
CREATE INDEX IF NOT EXISTS idx_report_evidence_register_canonical
    ON report_evidence_register(run_id, kind, canonical_id);

CREATE TABLE IF NOT EXISTS report_skill_runs (
    skill_run_id        TEXT PRIMARY KEY,
    run_id              TEXT NOT NULL,
    kind                TEXT NOT NULL,
    invoked_at          TEXT NOT NULL,
    finished_at         TEXT,
    summary             TEXT,
    blocking_reason     TEXT,
    blocking_questions_json TEXT,
    raw_output_json     TEXT,
    FOREIGN KEY (run_id) REFERENCES report_runs(run_id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_report_skill_runs_run
    ON report_skill_runs(run_id, invoked_at DESC);
CREATE INDEX IF NOT EXISTS idx_report_skill_runs_kind
    ON report_skill_runs(run_id, kind, invoked_at DESC);

CREATE TABLE IF NOT EXISTS report_research_log (
    research_id         TEXT PRIMARY KEY,
    run_id              TEXT NOT NULL,
    question            TEXT NOT NULL,
    focus               TEXT,
    asked_at            TEXT NOT NULL,
    resolver            TEXT,
    summary             TEXT,
    sources_count       INTEGER NOT NULL DEFAULT 0,
    raw_payload_json    TEXT,
    FOREIGN KEY (run_id) REFERENCES report_runs(run_id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_report_research_log_run
    ON report_research_log(run_id, asked_at DESC);

CREATE TABLE IF NOT EXISTS report_questions (
    question_id         TEXT PRIMARY KEY,
    run_id              TEXT NOT NULL,
    section             TEXT,
    reason              TEXT,
    questions_json      TEXT NOT NULL,
    allow_fallback      INTEGER NOT NULL DEFAULT 0,
    raised_at           TEXT NOT NULL,
    answered_at         TEXT,
    answer_text         TEXT,
    FOREIGN KEY (run_id) REFERENCES report_runs(run_id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_report_questions_run
    ON report_questions(run_id, raised_at DESC);
CREATE INDEX IF NOT EXISTS idx_report_questions_open
    ON report_questions(run_id, answered_at);

CREATE TABLE IF NOT EXISTS report_provenance (
    prov_id             TEXT PRIMARY KEY,
    run_id              TEXT NOT NULL,
    kind                TEXT NOT NULL,
    occurred_at         TEXT NOT NULL,
    instance_id         TEXT,
    skill_run_id        TEXT,
    research_id         TEXT,
    payload_json        TEXT,
    FOREIGN KEY (run_id) REFERENCES report_runs(run_id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_report_provenance_run
    ON report_provenance(run_id, occurred_at DESC);
CREATE INDEX IF NOT EXISTS idx_report_provenance_instance
    ON report_provenance(run_id, instance_id, occurred_at DESC);

CREATE TABLE IF NOT EXISTS report_check_runs (
    check_id            TEXT PRIMARY KEY,
    run_id              TEXT NOT NULL,
    check_kind          TEXT NOT NULL,
    checked_at          TEXT NOT NULL,
    ready_to_finish     INTEGER NOT NULL DEFAULT 0,
    needs_revision      INTEGER NOT NULL DEFAULT 0,
    payload_json        TEXT,
    FOREIGN KEY (run_id) REFERENCES report_runs(run_id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_report_check_runs_run
    ON report_check_runs(run_id, check_kind, checked_at DESC);

CREATE TABLE IF NOT EXISTS report_review_feedback (
    feedback_id         TEXT PRIMARY KEY,
    run_id              TEXT NOT NULL,
    source_file         TEXT,
    instance_id         TEXT,
    form_only           INTEGER NOT NULL DEFAULT 0,
    body                TEXT NOT NULL,
    imported_at         TEXT NOT NULL,
    FOREIGN KEY (run_id) REFERENCES report_runs(run_id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_report_review_feedback_run
    ON report_review_feedback(run_id, imported_at DESC);
CREATE INDEX IF NOT EXISTS idx_report_review_feedback_instance
    ON report_review_feedback(run_id, instance_id);

-- Figures: schematic drawings, charts, diagrams attached to a run.
-- The DOCX renderer embeds these as native images; markdown emits
-- ![](path). Cross-references use {{fig:<figure_id>}} tokens in block
-- markdown that the renderer resolves to the auto-numbered figure.
CREATE TABLE IF NOT EXISTS report_figures (
    figure_id           TEXT PRIMARY KEY,
    run_id              TEXT NOT NULL,
    fig_number          INTEGER,           -- assigned at render time
    kind                TEXT NOT NULL,     -- 'schematic'|'chart'|'photo'|'extracted'
    instance_id         TEXT,              -- block this figure belongs to
    image_path          TEXT NOT NULL,     -- absolute path on disk
    caption             TEXT NOT NULL,
    source_label        TEXT NOT NULL,     -- e.g. 'eigene Darstellung' or DOI
    code_kind           TEXT,              -- 'mermaid'|'matplotlib'|'graphviz'|null
    code_md             TEXT,              -- source code if generated
    width_px            INTEGER,
    height_px           INTEGER,
    created_at          TEXT NOT NULL,
    FOREIGN KEY (run_id) REFERENCES report_runs(run_id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_report_figures_run
    ON report_figures(run_id, created_at ASC);
CREATE INDEX IF NOT EXISTS idx_report_figures_instance
    ON report_figures(run_id, instance_id);

-- Real Word tables: structured rows + header + caption + legend. The
-- DOCX renderer emits a native Word table with the asset_pack's matrix
-- style. Markdown renderer emits a GFM pipe table. Cross-refs use
-- {{tbl:<table_id>}} tokens.
CREATE TABLE IF NOT EXISTS report_tables (
    table_id            TEXT PRIMARY KEY,
    run_id              TEXT NOT NULL,
    tbl_number          INTEGER,           -- assigned at render time
    kind                TEXT NOT NULL,     -- 'matrix'|'scenario'|'defect_catalog'|'risk_register'|'abbreviations'|'generic'
    instance_id         TEXT,              -- block this table belongs to
    caption             TEXT NOT NULL,
    legend              TEXT,
    header_json         TEXT NOT NULL,     -- ["col1","col2",...]
    rows_json           TEXT NOT NULL,     -- [["v11","v12",...], ...]
    created_at          TEXT NOT NULL,
    FOREIGN KEY (run_id) REFERENCES report_runs(run_id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_report_tables_run
    ON report_tables(run_id, created_at ASC);
CREATE INDEX IF NOT EXISTS idx_report_tables_instance
    ON report_tables(run_id, instance_id);
"#;
