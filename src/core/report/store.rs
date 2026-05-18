//! SQLite schema and connection helpers for `ctox report`.
//!
//! All durable state for report runs lives in the consolidated
//! `runtime/ctox.sqlite3` core DB under tables prefixed with `report_`.
//! Schema bootstrap is idempotent: call [`open`] from any subcommand entry
//! point, no migration step is required.

use anyhow::Context;
use anyhow::Result;
use rusqlite::Connection;
use std::path::Path;

use crate::paths;
use crate::persistence;

/// Open the core SQLite DB with the report schema ensured.
///
/// Safe to call from every subcommand. Sets WAL + busy-timeout to match the
/// rest of the runtime store. Schema is `CREATE TABLE IF NOT EXISTS`.
pub fn open(root: &Path) -> Result<Connection> {
    let path = paths::core_db(root);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("failed to create runtime dir {}", parent.display()))?;
    }
    let conn = Connection::open(&path)
        .with_context(|| format!("failed to open core db {}", path.display()))?;
    conn.busy_timeout(persistence::sqlite_busy_timeout_duration())
        .context("failed to configure SQLite busy_timeout")?;
    let busy_ms = persistence::sqlite_busy_timeout_millis();
    conn.execute_batch(&format!(
        "PRAGMA journal_mode = WAL;
         PRAGMA busy_timeout = {busy_ms};
         PRAGMA foreign_keys = ON;"
    ))
    .context("failed to set SQLite pragmas for report")?;
    ensure_schema(&conn).context("failed to ensure report schema")?;
    Ok(conn)
}

fn ensure_schema(conn: &Connection) -> Result<()> {
    conn.execute_batch(
        r#"
        CREATE TABLE IF NOT EXISTS report_runs (
            run_id TEXT PRIMARY KEY,
            preset TEXT NOT NULL,
            blueprint_version TEXT NOT NULL,
            topic TEXT NOT NULL,
            language TEXT NOT NULL DEFAULT 'en',
            locale_hints TEXT,
            status TEXT NOT NULL,
            last_stage TEXT,
            next_stage TEXT,
            state_machine_version INTEGER NOT NULL DEFAULT 1,
            created_at TEXT NOT NULL,
            updated_at TEXT NOT NULL
        );

        CREATE INDEX IF NOT EXISTS idx_report_runs_status
            ON report_runs(status, updated_at DESC);

        CREATE TABLE IF NOT EXISTS report_stage_runs (
            stage_run_id TEXT PRIMARY KEY,
            run_id TEXT NOT NULL REFERENCES report_runs(run_id) ON DELETE CASCADE,
            stage TEXT NOT NULL,
            iteration INTEGER NOT NULL,
            status TEXT NOT NULL,
            input_payload_json TEXT,
            output_payload_json TEXT,
            failure_reason TEXT,
            started_at TEXT NOT NULL,
            finished_at TEXT
        );

        CREATE INDEX IF NOT EXISTS idx_report_stage_runs_run
            ON report_stage_runs(run_id, stage, iteration DESC);

        CREATE TABLE IF NOT EXISTS report_scope (
            run_id TEXT PRIMARY KEY REFERENCES report_runs(run_id) ON DELETE CASCADE,
            leading_questions_json TEXT NOT NULL,
            out_of_scope_json TEXT NOT NULL,
            assumptions_json TEXT NOT NULL,
            disclaimer_md TEXT NOT NULL,
            success_criteria_json TEXT NOT NULL,
            updated_at TEXT NOT NULL
        );

        CREATE TABLE IF NOT EXISTS report_requirements (
            requirement_id TEXT PRIMARY KEY,
            run_id TEXT NOT NULL REFERENCES report_runs(run_id) ON DELETE CASCADE,
            code TEXT NOT NULL,
            title TEXT NOT NULL,
            description_md TEXT,
            must_have INTEGER NOT NULL DEFAULT 1,
            derived_from_question_idx INTEGER,
            created_at TEXT NOT NULL,
            UNIQUE(run_id, code)
        );

        CREATE TABLE IF NOT EXISTS report_options (
            option_id TEXT PRIMARY KEY,
            run_id TEXT NOT NULL REFERENCES report_runs(run_id) ON DELETE CASCADE,
            code TEXT NOT NULL,
            label TEXT NOT NULL,
            summary_md TEXT,
            synonyms_json TEXT,
            created_at TEXT NOT NULL,
            UNIQUE(run_id, code)
        );

        CREATE TABLE IF NOT EXISTS report_evidence (
            evidence_id TEXT PRIMARY KEY,
            run_id TEXT NOT NULL REFERENCES report_runs(run_id) ON DELETE CASCADE,
            citation_kind TEXT NOT NULL,
            canonical_id TEXT NOT NULL,
            title TEXT,
            authors_json TEXT,
            venue TEXT,
            year INTEGER,
            publisher TEXT,
            landing_url TEXT,
            full_text_url TEXT,
            abstract_md TEXT,
            snippet_md TEXT,
            retrieved_at TEXT,
            resolver TEXT,
            license TEXT,
            integrity_hash TEXT,
            created_at TEXT NOT NULL
        );

        CREATE INDEX IF NOT EXISTS idx_report_evidence_run
            ON report_evidence(run_id, citation_kind);
        CREATE INDEX IF NOT EXISTS idx_report_evidence_canonical
            ON report_evidence(canonical_id);

        CREATE TABLE IF NOT EXISTS report_scoring_rubrics (
            rubric_id TEXT PRIMARY KEY,
            run_id TEXT NOT NULL REFERENCES report_runs(run_id) ON DELETE CASCADE,
            axis_code TEXT NOT NULL,
            level_code TEXT NOT NULL,
            level_definition_md TEXT NOT NULL,
            numeric_value REAL,
            created_at TEXT NOT NULL,
            UNIQUE(run_id, axis_code, level_code)
        );

        CREATE TABLE IF NOT EXISTS report_matrix_cells (
            cell_id TEXT PRIMARY KEY,
            run_id TEXT NOT NULL REFERENCES report_runs(run_id) ON DELETE CASCADE,
            matrix_kind TEXT NOT NULL,
            matrix_label TEXT,
            option_id TEXT NOT NULL REFERENCES report_options(option_id) ON DELETE CASCADE,
            axis_code TEXT NOT NULL,
            value_label TEXT NOT NULL,
            value_numeric REAL,
            rubric_anchor TEXT,
            rationale_md TEXT NOT NULL,
            evidence_ids_json TEXT NOT NULL,
            assumption_note_md TEXT,
            created_at TEXT NOT NULL,
            UNIQUE(run_id, matrix_kind, matrix_label, option_id, axis_code)
        );

        CREATE INDEX IF NOT EXISTS idx_report_matrix_cells_run
            ON report_matrix_cells(run_id, matrix_kind);

        CREATE TABLE IF NOT EXISTS report_scenarios (
            scenario_id TEXT PRIMARY KEY,
            run_id TEXT NOT NULL REFERENCES report_runs(run_id) ON DELETE CASCADE,
            code TEXT NOT NULL,
            label TEXT NOT NULL,
            description_md TEXT NOT NULL,
            impact_summary_md TEXT,
            created_at TEXT NOT NULL,
            UNIQUE(run_id, code)
        );

        CREATE TABLE IF NOT EXISTS report_risks (
            risk_id TEXT PRIMARY KEY,
            run_id TEXT NOT NULL REFERENCES report_runs(run_id) ON DELETE CASCADE,
            code TEXT NOT NULL,
            title TEXT NOT NULL,
            description_md TEXT NOT NULL,
            likelihood TEXT,
            impact TEXT,
            mitigation_md TEXT NOT NULL,
            evidence_ids_json TEXT,
            created_at TEXT NOT NULL,
            UNIQUE(run_id, code)
        );

        CREATE TABLE IF NOT EXISTS report_claims (
            claim_id TEXT PRIMARY KEY,
            run_id TEXT NOT NULL REFERENCES report_runs(run_id) ON DELETE CASCADE,
            section_id TEXT NOT NULL,
            position INTEGER NOT NULL,
            text_md TEXT NOT NULL,
            claim_kind TEXT NOT NULL,
            confidence TEXT,
            evidence_ids_json TEXT NOT NULL,
            assumption_note_md TEXT,
            rubric_anchor TEXT,
            primary_recommendation INTEGER NOT NULL DEFAULT 0,
            scenario_code TEXT,
            created_at TEXT NOT NULL
        );

        CREATE INDEX IF NOT EXISTS idx_report_claims_section
            ON report_claims(run_id, section_id, position);

        CREATE TABLE IF NOT EXISTS report_versions (
            version_id TEXT PRIMARY KEY,
            run_id TEXT NOT NULL REFERENCES report_runs(run_id) ON DELETE CASCADE,
            version_number INTEGER NOT NULL,
            parent_version_id TEXT,
            manuscript_json TEXT NOT NULL,
            body_hash TEXT NOT NULL,
            produced_by TEXT NOT NULL,
            notes_md TEXT,
            created_at TEXT NOT NULL,
            UNIQUE(run_id, version_number)
        );

        CREATE INDEX IF NOT EXISTS idx_report_versions_run
            ON report_versions(run_id, version_number DESC);

        CREATE TABLE IF NOT EXISTS report_critiques (
            critique_id TEXT PRIMARY KEY,
            run_id TEXT NOT NULL REFERENCES report_runs(run_id) ON DELETE CASCADE,
            version_id TEXT NOT NULL REFERENCES report_versions(version_id) ON DELETE CASCADE,
            findings_json TEXT NOT NULL,
            summary_md TEXT,
            created_at TEXT NOT NULL
        );

        CREATE INDEX IF NOT EXISTS idx_report_critiques_run
            ON report_critiques(run_id, created_at DESC);

        CREATE TABLE IF NOT EXISTS report_renders (
            render_id TEXT PRIMARY KEY,
            run_id TEXT NOT NULL REFERENCES report_runs(run_id) ON DELETE CASCADE,
            version_id TEXT NOT NULL REFERENCES report_versions(version_id) ON DELETE CASCADE,
            format TEXT NOT NULL,
            output_path TEXT,
            file_size_bytes INTEGER,
            sha256 TEXT,
            renderer_version TEXT,
            created_at TEXT NOT NULL,
            UNIQUE(run_id, version_id, format)
        );

        CREATE TABLE IF NOT EXISTS report_check_reports (
            check_id TEXT PRIMARY KEY,
            run_id TEXT NOT NULL REFERENCES report_runs(run_id) ON DELETE CASCADE,
            version_id TEXT NOT NULL REFERENCES report_versions(version_id) ON DELETE CASCADE,
            overall_pass INTEGER NOT NULL,
            validators_json TEXT NOT NULL,
            created_at TEXT NOT NULL
        );

        CREATE INDEX IF NOT EXISTS idx_report_check_reports_run
            ON report_check_reports(run_id, created_at DESC);
        "#,
    )
    .context("failed to create report schema")?;
    Ok(())
}

/// Return the current UTC timestamp in RFC 3339 form, used for `*_at` columns.
pub fn now_iso() -> String {
    chrono::Utc::now().to_rfc3339()
}

/// Generate a fresh run/stage/etc. identifier. Uses UUID v4 — the rest of
/// the codebase uses UUIDs for similar purposes.
pub fn new_id(prefix: &str) -> String {
    let uuid = uuid::Uuid::new_v4();
    format!("{prefix}_{}", uuid.simple())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn schema_bootstraps_idempotently() {
        let dir = tempdir().unwrap();
        let _conn = open(dir.path()).unwrap();
        let _conn = open(dir.path()).unwrap();
        let conn = open(dir.path()).unwrap();
        let count: i64 = conn
            .query_row(
                "SELECT count(*) FROM sqlite_master WHERE type='table' AND name LIKE 'report_%'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert!(count >= 13, "expected >=13 report tables, got {count}");
    }

    #[test]
    fn new_id_is_unique_and_prefixed() {
        let a = new_id("run");
        let b = new_id("run");
        assert_ne!(a, b);
        assert!(a.starts_with("run_"));
    }
}
