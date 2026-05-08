//! Test suite for `src/report/`. Built in Wave 6 of the deep-research
//! skill rebuild. The tests use `StaticInferenceCallable` and small
//! scripted fixtures for every LLM-shaped call so they never make
//! network requests; they validate the full pipeline structurally.

mod asset_pack_roundtrip;
mod checks_smoke;
mod cli_smoke;
mod rascon_replay;
mod release_guard_lints;
mod workspace_smoke;

pub mod fixtures {
    use anyhow::Result;
    use tempfile::TempDir;

    use crate::report::patch::{
        record_skill_run, stage_pending_blocks, SkillRunKind, SkillRunRecord, StagedBlock,
    };
    use crate::report::schema::{ensure_schema, new_id, now_iso, open};
    use rusqlite::params;

    /// A throwaway CTOX root for one test. Owns a `TempDir` so cleanup
    /// happens when the fixture drops.
    pub struct TestRoot {
        pub dir: TempDir,
    }

    impl TestRoot {
        pub fn new() -> Result<Self> {
            // Defensive: the production `paths::runtime_dir` reads
            // `CTOX_STATE_ROOT` if set. Strip it for the test process so
            // the tempdir really is the run's root.
            // Safety: env-var mutation is process-wide; tests share the
            // process but all of our tests run with the same intent.
            std::env::remove_var("CTOX_STATE_ROOT");
            let dir = tempfile::tempdir()?;
            Ok(TestRoot { dir })
        }
        pub fn path(&self) -> &std::path::Path {
            self.dir.path()
        }
    }

    /// Build a default `CreateRunParams` for the RASCON-style fixture.
    pub fn rascon_create_params() -> crate::report::state::CreateRunParams {
        crate::report::state::CreateRunParams {
            report_type_id: "feasibility_study".into(),
            domain_profile_id: "ndt_aerospace".into(),
            depth_profile_id: "decision_grade".into(),
            style_profile_id: "scientific_engineering_dossier".into(),
            language: "de".into(),
            raw_topic: "Kontaktlose Pruefung des LSP-Kupfergitters in CFRP-Strukturen".into(),
            package_summary: None,
        }
    }

    /// Insert a committed block directly into `report_blocks` for tests
    /// that need a populated workspace without going through the full
    /// write_with_skill -> apply_block_patch dance. Returns the
    /// `instance_id` actually written.
    #[allow(clippy::too_many_arguments)]
    pub fn insert_committed_block(
        root: &std::path::Path,
        run_id: &str,
        doc_id: &str,
        block_id: &str,
        title: &str,
        ord: i64,
        markdown: &str,
        used_reference_ids: &[&str],
    ) -> Result<String> {
        let conn = open(root)?;
        ensure_schema(&conn)?;
        let instance_id = format!("{doc_id}__{block_id}");
        let used_refs: Vec<String> = used_reference_ids.iter().map(|s| s.to_string()).collect();
        let used_refs_json = serde_json::to_string(&used_refs)?;
        let now = now_iso();
        // Drop any prior committed row for the same (run_id, instance_id)
        // so re-insertion in tests is idempotent.
        conn.execute(
            "DELETE FROM report_blocks WHERE run_id = ?1 AND instance_id = ?2",
            params![run_id, instance_id],
        )?;
        conn.execute(
            "INSERT INTO report_blocks (
                 run_id, instance_id, doc_id, block_id, block_template_id,
                 title, ord, markdown, reason, used_skill_ids_json,
                 used_research_ids_json, used_reference_ids_json, committed_at
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)",
            params![
                run_id,
                instance_id,
                doc_id,
                block_id,
                block_id,
                title,
                ord,
                markdown,
                "test fixture",
                "[]",
                "[]",
                used_refs_json,
                now,
            ],
        )?;
        Ok(instance_id)
    }

    /// Insert a synthetic evidence-register row for tests.
    #[allow(clippy::too_many_arguments)]
    pub fn insert_evidence(
        root: &std::path::Path,
        run_id: &str,
        evidence_id: &str,
        kind: &str,
        canonical_id: Option<&str>,
        title: Option<&str>,
        authors: &[&str],
        year: Option<i64>,
    ) -> Result<()> {
        let conn = open(root)?;
        ensure_schema(&conn)?;
        let authors_vec: Vec<String> = authors.iter().map(|s| s.to_string()).collect();
        let authors_json = serde_json::to_string(&authors_vec)?;
        let now = now_iso();
        // Both schemas (state.rs and sources/mod.rs) maintain
        // `report_evidence_register`; pick the lowest-common-denominator
        // column set that the workspace reader needs.
        conn.execute(
            "DELETE FROM report_evidence_register WHERE run_id = ?1 AND evidence_id = ?2",
            params![run_id, evidence_id],
        )?;
        // The state-side schema does not have NOT NULL on every column,
        // so a partial insert is fine. We use INSERT OR IGNORE to absorb
        // any UNIQUE-constraint complaints from the sources-side schema.
        conn.execute(
            "INSERT OR REPLACE INTO report_evidence_register (
                 evidence_id, run_id, kind, canonical_id, title, authors_json,
                 venue, year, publisher, url_canonical, url_full_text,
                 license, abstract_md, snippet_md, retrieved_at,
                 resolver_used, integrity_hash, citations_count
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, NULL, ?7, NULL, NULL, NULL,
                       NULL, NULL, NULL, ?8, 'manual', NULL, 0)",
            params![
                evidence_id,
                run_id,
                kind,
                canonical_id,
                title,
                authors_json,
                year,
                now,
            ],
        )
        .or_else(|_| {
            // Fallback path: if the resolver-side schema is the one in
            // play (different column set), we try a slimmer insert that
            // matches it. We ignore success/failure here — the second
            // call is best-effort.
            conn.execute(
                "INSERT OR IGNORE INTO report_evidence_register (
                     evidence_id, run_id, kind, canonical_id, title,
                     authors_json, year, resolver_used, raw_payload_json,
                     citations_count, created_at, updated_at
                 ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, 'manual', '{}',
                           0, ?8, ?8)",
                params![
                    evidence_id,
                    run_id,
                    kind,
                    canonical_id,
                    title,
                    authors_json,
                    year,
                    now,
                ],
            )
        })?;
        Ok(())
    }

    /// Stage one block via `record_skill_run` + `stage_pending_blocks`.
    /// Returns the `skill_run_id` of the staged write.
    pub fn stage_one_write(
        root: &std::path::Path,
        run_id: &str,
        doc_id: &str,
        block_id: &str,
        title: &str,
        ord: i64,
        markdown: &str,
    ) -> Result<String> {
        let conn = open(root)?;
        ensure_schema(&conn)?;
        let skill_run_id = new_id("skill_write");
        let staged = StagedBlock {
            instance_id: format!("{doc_id}__{block_id}"),
            doc_id: doc_id.to_string(),
            block_id: block_id.to_string(),
            block_template_id: block_id.to_string(),
            title: title.to_string(),
            ord,
            markdown: markdown.to_string(),
            reason: "test fixture".to_string(),
            used_reference_ids: Vec::new(),
        };
        let record = SkillRunRecord {
            skill_run_id: skill_run_id.clone(),
            run_id: run_id.to_string(),
            kind: SkillRunKind::Write,
            summary: "test write".to_string(),
            blocking_reason: None,
            blocking_questions: Vec::new(),
            blocks: vec![staged.clone()],
            raw_output: serde_json::Value::Null,
        };
        record_skill_run(&conn, &record)?;
        stage_pending_blocks(&conn, run_id, &skill_run_id, SkillRunKind::Write, &[staged])?;
        Ok(skill_run_id)
    }
}
