//! Workspace state for a single deep-research run.
//!
//! Reads the `report_*` tables and materialises the JSON payloads that the
//! manager loop and sub-skills consume. Function shapes mirror the
//! Förderantrag agent (`Foerdervorhaben-Agent.html`):
//!
//! - [`Workspace::workspace_snapshot`] -> `buildWorkspaceSnapshotPayload`
//! - [`Workspace::asset_lookup`]       -> `buildAssetLookupResult`
//! - [`Workspace::skill_input`]        -> `buildSkillInput`
//! - [`Workspace::narrative_flow_input`] -> `buildNarrativeFlowInput`
//! - [`Workspace::manager_input`]      -> `buildManagerInput`
//! - [`Workspace::style_guide_payload`] -> `buildStyleGuidePayload`
//!
//! The `compute_completeness` helper here is intentionally a stub — the
//! sophisticated check tool comes in Wave 4. Snapshot consumers only need
//! the boolean `ready_to_finish` plus per-instance status.

use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::Path;

use anyhow::{anyhow, Context, Result};
use rusqlite::{params, Connection, OptionalExtension};
use serde_json::{json, Value};

use crate::report::asset_pack::{
    AssetPack, BlockLibraryEntry, DocumentBlueprint, OptionalModule, ReferenceResource,
};
use crate::report::schema::{ensure_schema, open};
use crate::report::state::{load_run_with, RunRecord};

/// Sub-skill mode the workspace builds an input bundle for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SkillMode {
    Write,
    Revision,
    FlowReview,
}

impl SkillMode {
    pub fn as_str(self) -> &'static str {
        match self {
            SkillMode::Write => "write",
            SkillMode::Revision => "revision",
            SkillMode::FlowReview => "flow_review",
        }
    }
}

/// Character-budget summary for the run (mirrors Förderantrag's payload).
#[derive(Debug, Clone)]
pub struct CharacterBudget {
    pub target_chars: usize,
    pub actual_chars: usize,
    pub delta_chars: i64,
    pub within_tolerance: bool,
    pub severely_off_target: bool,
    pub reference_average_chars: usize,
    pub status: String,
}

/// Run metadata flattened into the shape the snapshot exposes.
#[derive(Debug, Clone)]
pub struct RunMetadata {
    pub run_id: String,
    pub report_type_id: String,
    pub domain_profile_id: String,
    pub depth_profile_id: String,
    pub style_profile_id: String,
    pub language: String,
    pub status: String,
    pub raw_topic: String,
}

#[derive(Debug, Clone)]
pub struct EvidenceEntry {
    pub evidence_id: String,
    pub kind: String,
    pub canonical_id: Option<String>,
    pub title: Option<String>,
    pub authors: Vec<String>,
    pub venue: Option<String>,
    pub year: Option<i64>,
    pub publisher: Option<String>,
    pub url_canonical: Option<String>,
    pub url_full_text: Option<String>,
    pub license: Option<String>,
    pub abstract_md: Option<String>,
    pub snippet_md: Option<String>,
    pub retrieved_at: Option<String>,
    pub resolver_used: Option<String>,
    pub integrity_hash: Option<String>,
    pub citations_count: i64,
    pub content_chars: i64,
}

#[derive(Debug, Clone)]
pub struct ResearchLogEntry {
    pub research_id: String,
    pub question: String,
    pub focus: Option<String>,
    pub resolver: Option<String>,
    pub summary: Option<String>,
    pub sources_count: i64,
}

#[derive(Debug, Clone)]
pub struct BlockRecord {
    pub instance_id: String,
    pub doc_id: String,
    pub block_id: String,
    pub block_template_id: Option<String>,
    pub title: String,
    pub ord: i64,
    pub markdown: String,
    pub reason: Option<String>,
    pub used_skill_ids: Vec<String>,
    pub used_research_ids: Vec<String>,
    pub used_reference_ids: Vec<String>,
    pub committed_at: String,
    pub kind: Option<String>, // populated for pending_blocks ("write" | "revision")
    pub skill_run_id: Option<String>, // populated for pending_blocks
}

/// Figure row exposed by [`Workspace::figures`].
#[derive(Debug, Clone)]
pub struct FigureRow {
    pub figure_id: String,
    pub kind: String,
    pub instance_id: Option<String>,
    pub image_path: String,
    pub caption: String,
    pub source_label: String,
    pub code_kind: Option<String>,
    pub width_px: Option<i64>,
    pub height_px: Option<i64>,
}

/// Table row exposed by [`Workspace::tables`].
#[derive(Debug, Clone)]
pub struct TableRow {
    pub table_id: String,
    pub kind: String,
    pub instance_id: Option<String>,
    pub caption: String,
    pub legend: Option<String>,
    pub headers: Vec<String>,
    pub rows: Vec<Vec<String>>,
}

/// Owned handle on a single run. Holds a connection plus the cached
/// asset pack reference; all builders read off these.
///
/// Note: the asset pack is a process-static `OnceLock`; the workspace
/// just borrows it for `'static`. The optional `'a` lifetime parameter
/// is preserved so callers can still spell `Workspace<'a>` in trait
/// bounds, but every constructor binds it to `'static` in practice.
pub struct Workspace<'a> {
    conn: Connection,
    asset_pack: &'a AssetPack,
    run: RunRecord,
}

impl Workspace<'static> {
    /// Open the consolidated DB, ensure schema, and load the run row.
    pub fn load(root: &Path, run_id: &str) -> Result<Workspace<'static>> {
        let conn = open(root)?;
        ensure_schema(&conn)?;
        let run = load_run_with(&conn, run_id)?;
        let asset_pack = AssetPack::load()?;
        Ok(Workspace {
            conn,
            asset_pack,
            run,
        })
    }
}

impl<'a> Workspace<'a> {
    pub fn run_metadata(&self) -> Result<RunMetadata> {
        Ok(RunMetadata {
            run_id: self.run.run_id.clone(),
            report_type_id: self.run.report_type_id.clone(),
            domain_profile_id: self.run.domain_profile_id.clone(),
            depth_profile_id: self.run.depth_profile_id.clone(),
            style_profile_id: self.run.style_profile_id.clone(),
            language: self.run.language.clone(),
            status: self.run.status.clone(),
            raw_topic: self.run.raw_topic.clone(),
        })
    }

    pub fn committed_blocks(&self) -> Result<Vec<BlockRecord>> {
        load_blocks(&self.conn, &self.run.run_id, false)
    }

    pub fn pending_blocks(&self) -> Result<Vec<BlockRecord>> {
        load_blocks(&self.conn, &self.run.run_id, true)
    }

    /// Figures registered via `ctox report figure-add`. Ordered by
    /// insertion time so the renderer can assign deterministic
    /// `fig_number` values.
    pub fn figures(&self) -> Result<Vec<FigureRow>> {
        let mut stmt = self.conn.prepare(
            "SELECT figure_id, kind, instance_id, image_path, caption, source_label, \
                    code_kind, width_px, height_px \
             FROM report_figures WHERE run_id = ?1 ORDER BY created_at ASC",
        )?;
        let rows = stmt.query_map(params![self.run.run_id], |row| {
            Ok(FigureRow {
                figure_id: row.get(0)?,
                kind: row.get(1)?,
                instance_id: row.get(2)?,
                image_path: row.get(3)?,
                caption: row.get(4)?,
                source_label: row.get(5)?,
                code_kind: row.get(6)?,
                width_px: row.get(7)?,
                height_px: row.get(8)?,
            })
        })?;
        rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
    }

    /// Tables registered via `ctox report table-add`.
    pub fn tables(&self) -> Result<Vec<TableRow>> {
        let mut stmt = self.conn.prepare(
            "SELECT table_id, kind, instance_id, caption, legend, header_json, rows_json \
             FROM report_tables WHERE run_id = ?1 ORDER BY created_at ASC",
        )?;
        let rows = stmt.query_map(params![self.run.run_id], |row| {
            let header_json: String = row.get(5)?;
            let rows_json: String = row.get(6)?;
            let headers: Vec<String> = serde_json::from_str(&header_json).unwrap_or_default();
            let data: Vec<Vec<String>> = serde_json::from_str(&rows_json).unwrap_or_default();
            Ok(TableRow {
                table_id: row.get(0)?,
                kind: row.get(1)?,
                instance_id: row.get(2)?,
                caption: row.get(3)?,
                legend: row.get(4)?,
                headers,
                rows: data,
            })
        })?;
        rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
    }

    pub fn evidence_register(&self) -> Result<Vec<EvidenceEntry>> {
        let mut stmt = self.conn.prepare(
            "SELECT evidence_id, kind, canonical_id, title, authors_json, venue, year,
                    publisher, url_canonical, url_full_text, license, abstract_md,
                    snippet_md, retrieved_at, resolver_used, integrity_hash, citations_count,
                    length(coalesce(full_text_md, abstract_md, snippet_md, ''))
             FROM report_evidence_register WHERE run_id = ?1
             ORDER BY retrieved_at DESC",
        )?;
        let rows = stmt.query_map(params![self.run.run_id], |row| {
            let authors_json: Option<String> = row.get(4)?;
            let authors = authors_json
                .as_deref()
                .and_then(|s| serde_json::from_str::<Vec<String>>(s).ok())
                .unwrap_or_default();
            Ok(EvidenceEntry {
                evidence_id: row.get(0)?,
                kind: row.get(1)?,
                canonical_id: row.get(2)?,
                title: row.get(3)?,
                authors,
                venue: row.get(5)?,
                year: row.get(6)?,
                publisher: row.get(7)?,
                url_canonical: row.get(8)?,
                url_full_text: row.get(9)?,
                license: row.get(10)?,
                abstract_md: row.get(11)?,
                snippet_md: row.get(12)?,
                retrieved_at: row.get(13)?,
                resolver_used: row.get(14)?,
                integrity_hash: row.get(15)?,
                citations_count: row.get(16)?,
                content_chars: row.get(17)?,
            })
        })?;
        let mut out = Vec::new();
        for row in rows {
            out.push(row?);
        }
        Ok(out)
    }

    pub fn research_log_entries(&self) -> Result<Vec<ResearchLogEntry>> {
        let mut stmt = self.conn.prepare(
            "SELECT research_id, question, focus, resolver, summary, sources_count
             FROM report_research_log
             WHERE run_id = ?1
             ORDER BY asked_at ASC",
        )?;
        let rows = stmt.query_map(params![self.run.run_id], |row| {
            Ok(ResearchLogEntry {
                research_id: row.get(0)?,
                question: row.get(1)?,
                focus: row.get(2)?,
                resolver: row.get(3)?,
                summary: row.get(4)?,
                sources_count: row.get(5)?,
            })
        })?;
        let mut out = Vec::new();
        for row in rows {
            out.push(row?);
        }
        Ok(out)
    }

    /// Total character count target (the report type's `typical_chars`).
    pub fn character_budget(&self) -> Result<CharacterBudget> {
        let report_type = self.asset_pack.report_type(&self.run.report_type_id)?;
        let target = report_type.typical_chars as usize;
        let committed = self.committed_blocks()?;
        let block_chars: usize = committed.iter().map(|b| b.markdown.chars().count()).sum();
        let table_chars: usize = self
            .tables()
            .unwrap_or_default()
            .iter()
            .map(|table| {
                let caption = table.caption.trim().chars().count();
                let legend = table
                    .legend
                    .as_deref()
                    .unwrap_or_default()
                    .trim()
                    .chars()
                    .count();
                let headers: usize = table
                    .headers
                    .iter()
                    .map(|cell| cell.trim().chars().count())
                    .sum();
                let rows: usize = table
                    .rows
                    .iter()
                    .flatten()
                    .map(|cell| cell.trim().chars().count())
                    .sum();
                caption + legend + headers + rows
            })
            .sum();
        let actual = block_chars + table_chars;
        let delta: i64 = actual as i64 - target as i64;
        // Default tolerance: 20% of target unless target == 0 (no committed
        // blocks yet); kept consistent with the snapshot's tolerance field.
        let tolerance: f64 = 0.20;
        let within_tolerance = if target == 0 {
            actual == 0
        } else {
            (delta.unsigned_abs() as f64) <= (target as f64 * tolerance)
        };
        // Severely off: > 50 % delta
        let severely_off_target = if target == 0 {
            false
        } else {
            (delta.unsigned_abs() as f64) > (target as f64 * 0.50)
        };
        let status = if actual == 0 {
            "not_started"
        } else if within_tolerance {
            "within"
        } else if delta < 0 {
            if severely_off_target {
                "severely_off"
            } else {
                "low"
            }
        } else if severely_off_target {
            "severely_off"
        } else {
            "high"
        };
        let reference_average_chars = self.asset_pack.reference_length_stats.average_chars as usize;
        Ok(CharacterBudget {
            target_chars: target,
            actual_chars: actual,
            delta_chars: delta,
            within_tolerance,
            severely_off_target,
            reference_average_chars,
            status: status.to_string(),
        })
    }

    /// Workspace snapshot payload. Field set mirrors `check_contracts.md`
    /// with a couple of extra fields the manager wants for its own logging.
    pub fn workspace_snapshot(&self) -> Result<Value> {
        let metadata = self.run_metadata()?;
        let report_type = self.asset_pack.report_type(&self.run.report_type_id)?;
        let blueprint = self
            .asset_pack
            .document_blueprint(&report_type.document_blueprint_id)?;
        let modules: HashMap<String, &OptionalModule> = self
            .asset_pack
            .optional_modules
            .iter()
            .map(|m| (m.id.clone(), m))
            .collect();

        let committed = self.committed_blocks()?;
        let pending = self.pending_blocks()?;
        let committed_by_instance: HashMap<String, &BlockRecord> = committed
            .iter()
            .map(|b| (b.instance_id.clone(), b))
            .collect();

        let expected_blocks = build_expected_blocks(
            self.asset_pack,
            report_type.id.as_str(),
            &blueprint,
            &committed_by_instance,
            &modules,
        )?;
        let existing_blocks = build_existing_blocks(&committed);
        let pending_blocks_payload = build_pending_blocks_payload(&pending);
        let completeness = compute_completeness(self.asset_pack, &expected_blocks)?;
        let character_budget = self.character_budget()?;

        let answered_questions = self.answered_questions()?;
        let open_questions = self.open_questions()?;
        let blocking_open_questions: Vec<&Value> = open_questions
            .iter()
            .filter(|q| {
                !q.get("allow_fallback")
                    .and_then(Value::as_bool)
                    .unwrap_or(false)
            })
            .collect();

        let user_notes = self.user_notes()?;
        let review_feedback = self.review_feedback_payload(&existing_blocks)?;

        let available_research_ids: Vec<String> = self.research_log_ids()?;
        let available_skill_ids: Vec<String> = self.skill_run_ids_with_pending()?;

        let evidence_register = self.evidence_register()?;
        let depth = self.asset_pack.depth_profile(&self.run.depth_profile_id)?;
        let min_evidence_count = depth
            .min_evidence_count
            .or_else(|| {
                depth
                    .evidence_floor
                    .get("min_sources")
                    .and_then(Value::as_u64)
                    .map(|v| v as u32)
            })
            .unwrap_or(0);

        let report_type_obj = report_type_to_value(report_type);
        let domain_profile_obj = serde_json::to_value(
            self.asset_pack
                .domain_profile(&self.run.domain_profile_id)?,
        )?;
        let style_profile_obj =
            serde_json::to_value(self.asset_pack.style_profile(&self.run.style_profile_id)?)?;
        let depth_profile_obj = serde_json::to_value(depth)?;

        let package_summary = self.run.package_summary.clone().unwrap_or_else(|| {
            json!({
                "report_type_id": metadata.report_type_id,
                "domain_profile_id": metadata.domain_profile_id,
                "depth_profile_id": metadata.depth_profile_id,
                "style_profile_id": metadata.style_profile_id,
                "docs": blueprint
                    .base_docs
                    .iter()
                    .map(|d| json!({"id": d.id, "title": d.title}))
                    .collect::<Vec<_>>(),
                "modules": report_type.default_modules,
            })
        });

        Ok(json!({
            "run_metadata": {
                "run_id": metadata.run_id,
                "report_type_id": metadata.report_type_id,
                "domain_profile_id": metadata.domain_profile_id,
                "depth_profile_id": metadata.depth_profile_id,
                "style_profile_id": metadata.style_profile_id,
                "language": metadata.language,
                "status": metadata.status,
                "raw_topic": metadata.raw_topic,
            },
            "topic": metadata.raw_topic,
            "language": metadata.language,
            "depth_profile": depth_profile_obj,
            "domain_profile": domain_profile_obj,
            "style_profile": style_profile_obj,
            "report_type_id": metadata.report_type_id,
            "report_type": report_type_obj,
            "current_date": chrono::Utc::now().format("%Y-%m-%d").to_string(),
            "package_summary": package_summary,
            "expected_blocks": expected_blocks,
            "existing_blocks": existing_blocks,
            "pending_blocks": pending_blocks_payload,
            "completeness": completeness,
            "character_budget": character_budget_to_value(&character_budget),
            "min_evidence_count": min_evidence_count,
            "evidence_axes": Value::Array(Vec::new()),
            "answered_questions": answered_questions,
            "open_questions": open_questions,
            "blocking_open_questions": blocking_open_questions,
            "review_feedback": review_feedback,
            "user_notes": user_notes,
            "available_research_ids": available_research_ids,
            "available_skill_ids": available_skill_ids,
            "evidence_register_size": evidence_register.len(),
        }))
    }

    /// Asset-lookup result for a list of instance_ids. Mirrors the manager
    /// tool of the same name. Empty `instance_ids` means "every block in
    /// the resolved blueprint".
    pub fn asset_lookup(
        &self,
        instance_ids: &[String],
        include_report_type: bool,
    ) -> Result<Value> {
        let report_type = self.asset_pack.report_type(&self.run.report_type_id)?;
        let blueprint = self
            .asset_pack
            .document_blueprint(&report_type.document_blueprint_id)?;
        let modules: HashMap<String, &OptionalModule> = self
            .asset_pack
            .optional_modules
            .iter()
            .map(|m| (m.id.clone(), m))
            .collect();

        let committed = self.committed_blocks()?;
        let committed_by_instance: HashMap<String, &BlockRecord> = committed
            .iter()
            .map(|b| (b.instance_id.clone(), b))
            .collect();

        let expected_blocks = build_expected_blocks(
            self.asset_pack,
            report_type.id.as_str(),
            &blueprint,
            &committed_by_instance,
            &modules,
        )?;

        let allowed_block_ids: HashSet<&str> = report_type
            .block_library_keys
            .iter()
            .map(String::as_str)
            .collect();

        let wanted_instance_ids: HashSet<&str> = instance_ids.iter().map(String::as_str).collect();

        let mut block_defs: Vec<Value> = Vec::new();
        let mut reference_template_ids: Vec<String> = Vec::new();
        for block in &expected_blocks {
            let instance_id = block
                .get("instance_id")
                .and_then(Value::as_str)
                .unwrap_or_default();
            let block_id = block
                .get("block_id")
                .and_then(Value::as_str)
                .unwrap_or_default();
            if !instance_ids.is_empty() && !wanted_instance_ids.contains(instance_id) {
                continue;
            }
            if !allowed_block_ids.contains(block_id) {
                continue;
            }
            let entry = self
                .asset_pack
                .block_library_entry(block_id)
                .with_context(|| format!("asset_lookup block_id {block_id}"))?;
            reference_template_ids.push(block_id.to_string());
            block_defs.push(json!({
                "instance_id": instance_id,
                "block_id": block_id,
                "template_id": block.get("template_id").cloned().unwrap_or(Value::Null),
                "title": entry.title,
                "doc_id": block.get("doc_id").cloned().unwrap_or(Value::Null),
                "order": block.get("order").cloned().unwrap_or(Value::Null),
                "required": block.get("required").cloned().unwrap_or(Value::Bool(false)),
                "min_chars": entry.min_chars,
                "rubric": {
                    "description": entry.goal,
                    "must_cover": entry.must_have,
                    "reference_ids": entry.reference_ids,
                    "style_guide_keys": entry.style_rules,
                }
            }));
        }

        let references_payload = references_to_payload(self.asset_pack, &reference_template_ids);
        let style_guide = self.style_guide_payload()?;
        let document_flow = self.document_flow_value(&blueprint, &expected_blocks);

        let mut out = json!({
            "block_defs": block_defs,
            "references": references_payload,
            "style_guide": style_guide,
            "document_flow": document_flow,
            "reference_length_stats": {
                "mean_chars": self.asset_pack.reference_length_stats.average_chars,
                "median_chars": self.asset_pack.reference_length_stats.average_chars,
                "p10_chars": self.asset_pack.reference_length_stats.min_chars,
                "p90_chars": self.asset_pack.reference_length_stats.max_chars,
            },
        });
        if include_report_type {
            out["report_type"] = report_type_to_value(report_type);
        }
        Ok(out)
    }

    /// Build the input bundle for a sub-skill run.
    pub fn skill_input(
        &self,
        mode: SkillMode,
        instance_ids: &[String],
        brief: Option<&str>,
        goals: &[String],
    ) -> Result<Value> {
        let snapshot = self.workspace_snapshot()?;
        let asset = self.asset_lookup(instance_ids, false)?;
        let style_guide = self.style_guide_payload()?;
        let report_type = self.asset_pack.report_type(&self.run.report_type_id)?;
        let blueprint = self
            .asset_pack
            .document_blueprint(&report_type.document_blueprint_id)?;
        let modules: HashMap<String, &OptionalModule> = self
            .asset_pack
            .optional_modules
            .iter()
            .map(|m| (m.id.clone(), m))
            .collect();
        let committed = self.committed_blocks()?;
        let committed_by_instance: HashMap<String, &BlockRecord> = committed
            .iter()
            .map(|b| (b.instance_id.clone(), b))
            .collect();
        let expected_blocks = build_expected_blocks(
            self.asset_pack,
            report_type.id.as_str(),
            &blueprint,
            &committed_by_instance,
            &modules,
        )?;
        let document_flow = self.document_flow_value(&blueprint, &expected_blocks);

        let package_context = json!({
            "report_type_id": self.run.report_type_id,
            "report_type": report_type_to_value(report_type),
            "domain_profile_id": self.run.domain_profile_id,
            "domain_profile": serde_json::to_value(
                self.asset_pack.domain_profile(&self.run.domain_profile_id)?
            )?,
            "style_profile_id": self.run.style_profile_id,
            "style_profile": serde_json::to_value(
                self.asset_pack.style_profile(&self.run.style_profile_id)?
            )?,
            "depth_profile_id": self.run.depth_profile_id,
            "language": self.run.language,
            "current_date": chrono::Utc::now().format("%Y-%m-%d").to_string(),
        });

        let character_budget = character_budget_to_value(&self.character_budget()?);
        let existing_blocks = build_existing_blocks(&committed);
        let answered = self.answered_questions()?;
        let open = self.open_questions()?;
        let user_notes = self.user_notes()?;
        let review_feedback = self.review_feedback_payload(&existing_blocks)?;
        let research_notes = self.recent_research_notes(8)?;

        let mut bundle = json!({
            "mode": mode.as_str(),
            "package_context": package_context,
            "character_budget": character_budget,
            "style_guide": style_guide,
            "document_flow": document_flow,
            "workspace_snapshot": snapshot,
            "selected_blocks": asset.get("block_defs").cloned().unwrap_or(Value::Array(Vec::new())),
            "selected_references": asset
                .get("references")
                .cloned()
                .unwrap_or(Value::Array(Vec::new())),
            "existing_blocks": existing_blocks,
            "answered_questions": answered,
            "open_questions": open,
            "review_feedback": review_feedback,
            "user_notes": user_notes,
            "research_notes": research_notes,
        });
        match mode {
            SkillMode::Write => {
                bundle["brief"] = Value::String(brief.unwrap_or_default().to_string());
                bundle["goals"] = Value::Array(Vec::new());
            }
            SkillMode::Revision => {
                bundle["brief"] = Value::String(brief.unwrap_or_default().to_string());
                bundle["goals"] = Value::Array(goals.iter().cloned().map(Value::String).collect());
            }
            SkillMode::FlowReview => {
                // No brief or goals; flow_review does not write blocks.
            }
        }
        Ok(bundle)
    }

    /// Input bundle for the narrative-flow sub-skill. Identical to
    /// `skill_input(FlowReview, ...)` plus an explicit, ordered
    /// `current_blocks[]` list of every committed block in document order.
    pub fn narrative_flow_input(&self) -> Result<Value> {
        let mut input = self.skill_input(SkillMode::FlowReview, &[], None, &[])?;
        let mut current = self.committed_blocks()?;
        current.sort_by_key(|b| (b.doc_id.clone(), b.ord));
        let current_value = current
            .iter()
            .map(|b| {
                json!({
                    "instance_id": b.instance_id,
                    "title": b.title,
                    "doc_id": b.doc_id,
                    "order": b.ord,
                    "required": true,
                    "markdown": b.markdown,
                })
            })
            .collect::<Vec<_>>();
        input["current_blocks"] = Value::Array(current_value);
        Ok(input)
    }

    /// Input bundle the manager loop sees on every turn. Slim version of
    /// the workspace snapshot plus the four most recent check results.
    pub fn manager_input(&self) -> Result<Value> {
        let snapshot = self.workspace_snapshot()?;
        let last_completeness = self.last_check_payload("completeness")?;
        let last_character_budget = self.last_check_payload("character_budget")?;
        let last_release_guard = self.last_check_payload("release_guard")?;
        let last_narrative_flow = self.last_check_payload("narrative_flow")?;
        Ok(json!({
            "workspace_snapshot": snapshot,
            "last_completeness": last_completeness,
            "last_character_budget": last_character_budget,
            "last_release_guard": last_release_guard,
            "last_narrative_flow": last_narrative_flow,
        }))
    }

    /// Document flow scoped to specific instance_ids (or all, if empty).
    pub fn document_flow(&self, instance_ids: &[String]) -> Result<Value> {
        let report_type = self.asset_pack.report_type(&self.run.report_type_id)?;
        let blueprint = self
            .asset_pack
            .document_blueprint(&report_type.document_blueprint_id)?;
        let modules: HashMap<String, &OptionalModule> = self
            .asset_pack
            .optional_modules
            .iter()
            .map(|m| (m.id.clone(), m))
            .collect();
        let committed = self.committed_blocks()?;
        let committed_by_instance: HashMap<String, &BlockRecord> = committed
            .iter()
            .map(|b| (b.instance_id.clone(), b))
            .collect();
        let mut expected_blocks = build_expected_blocks(
            self.asset_pack,
            report_type.id.as_str(),
            &blueprint,
            &committed_by_instance,
            &modules,
        )?;
        if !instance_ids.is_empty() {
            let wanted: HashSet<&str> = instance_ids.iter().map(String::as_str).collect();
            expected_blocks.retain(|b| {
                b.get("instance_id")
                    .and_then(Value::as_str)
                    .map(|s| wanted.contains(s))
                    .unwrap_or(false)
            });
        }
        Ok(self.document_flow_value(&blueprint, &expected_blocks))
    }

    /// Style-guide payload: the full 20-list bundle plus the active
    /// profile's directives.
    pub fn style_guide_payload(&self) -> Result<Value> {
        let style_profile = self.asset_pack.style_profile(&self.run.style_profile_id)?;
        let guidance = self.asset_pack.style_guidance();
        Ok(json!({
            "active_profile": {
                "id": style_profile.id,
                "label": style_profile.label,
                "use_when": style_profile.use_when,
                "directives": style_profile.directives,
            },
            "reader_effect": guidance.reader_effect,
            "preferred_moves": guidance.preferred_moves,
            "document_arc": guidance.document_arc,
            "section_bridging": guidance.section_bridging,
            "reference_handling": guidance.reference_handling,
            "dossier_story_model": guidance.dossier_story_model,
            "section_role_guidance": guidance.section_role_guidance,
            "no_reference_strategy": guidance.no_reference_strategy,
            "internal_perspective_rules": guidance.internal_perspective_rules,
            "evidence_gap_policy": guidance.evidence_gap_policy,
            "domain_tone_rules": guidance.domain_tone_rules,
            "terminology_consistency_rules": guidance.terminology_consistency_rules,
            "numbers_freshness_rules": guidance.numbers_freshness_rules,
            "consultant_phrases_to_soften": guidance.consultant_phrases_to_soften,
            "dead_phrases_to_avoid": guidance.dead_phrases_to_avoid,
            "forbidden_meta_phrases": guidance.forbidden_meta_phrases,
            "revision_checklist": guidance.revision_checklist,
            "micro_examples": guidance.micro_examples,
            "report_type_arc": report_type_arc(&self.run.report_type_id),
        }))
    }

    // ---- private helpers ----

    fn answered_questions(&self) -> Result<Value> {
        let mut stmt = self.conn.prepare(
            "SELECT question_id, section, reason, questions_json, allow_fallback,
                    raised_at, answered_at, answer_text
             FROM report_questions
             WHERE run_id = ?1 AND answered_at IS NOT NULL
             ORDER BY answered_at DESC",
        )?;
        let rows = stmt.query_map(params![self.run.run_id], |row| {
            let questions_json: String = row.get(3)?;
            let questions: Value =
                serde_json::from_str(&questions_json).unwrap_or(Value::Array(Vec::new()));
            let allow_fallback: i64 = row.get(4)?;
            Ok(json!({
                "id": row.get::<_, String>(0)?,
                "section": row.get::<_, Option<String>>(1)?,
                "reason": row.get::<_, Option<String>>(2)?,
                "questions": questions,
                "allow_fallback": allow_fallback != 0,
                "raised_at": row.get::<_, String>(5)?,
                "answered_at": row.get::<_, Option<String>>(6)?,
                "answer": row.get::<_, Option<String>>(7)?,
            }))
        })?;
        let mut out = Vec::new();
        for row in rows {
            out.push(row?);
        }
        Ok(Value::Array(out))
    }

    fn open_questions(&self) -> Result<Vec<Value>> {
        let mut stmt = self.conn.prepare(
            "SELECT question_id, section, reason, questions_json, allow_fallback, raised_at
             FROM report_questions
             WHERE run_id = ?1 AND answered_at IS NULL
             ORDER BY raised_at ASC",
        )?;
        let rows = stmt.query_map(params![self.run.run_id], |row| {
            let questions_json: String = row.get(3)?;
            let questions: Value =
                serde_json::from_str(&questions_json).unwrap_or(Value::Array(Vec::new()));
            let allow_fallback: i64 = row.get(4)?;
            Ok(json!({
                "id": row.get::<_, String>(0)?,
                "section": row.get::<_, Option<String>>(1)?,
                "reason": row.get::<_, Option<String>>(2)?,
                "questions": questions,
                "allow_fallback": allow_fallback != 0,
                "raised_at": row.get::<_, String>(5)?,
            }))
        })?;
        let mut out = Vec::new();
        for row in rows {
            out.push(row?);
        }
        Ok(out)
    }

    fn user_notes(&self) -> Result<Vec<Value>> {
        // User notes are recorded as `report_provenance` rows with kind="note".
        // The richer notes table is built in a later wave; this helper is a
        // forward-compatible stub that returns an empty list when no rows
        // exist.
        let mut stmt = self.conn.prepare(
            "SELECT prov_id, occurred_at, payload_json
             FROM report_provenance
             WHERE run_id = ?1 AND kind = 'note'
             ORDER BY occurred_at ASC",
        )?;
        let rows = stmt.query_map(params![self.run.run_id], |row| {
            let payload_text: Option<String> = row.get(2)?;
            let payload: Value = payload_text
                .as_deref()
                .and_then(|s| serde_json::from_str(s).ok())
                .unwrap_or(Value::Null);
            Ok(json!({
                "id": row.get::<_, String>(0)?,
                "at": row.get::<_, String>(1)?,
                "payload": payload,
            }))
        })?;
        let mut out = Vec::new();
        for row in rows {
            out.push(row?);
        }
        Ok(out)
    }

    fn review_feedback_payload(&self, existing_blocks: &[Value]) -> Result<Value> {
        let mut stmt = self.conn.prepare(
            "SELECT feedback_id, source_file, instance_id, form_only, body, imported_at
             FROM report_review_feedback
             WHERE run_id = ?1
             ORDER BY imported_at DESC
             LIMIT 50",
        )?;
        let rows = stmt.query_map(params![self.run.run_id], |row| {
            let form_only: i64 = row.get(3)?;
            Ok(json!({
                "id": row.get::<_, String>(0)?,
                "source_file": row.get::<_, Option<String>>(1)?,
                "instance_id": row.get::<_, Option<String>>(2)?,
                "form_only": form_only != 0,
                "body": row.get::<_, String>(4)?,
                "imported_at": row.get::<_, String>(5)?,
            }))
        })?;
        let mut all: Vec<Value> = Vec::new();
        for row in rows {
            all.push(row?);
        }
        if all.is_empty() {
            return Ok(Value::Null);
        }
        let existing_ids: HashSet<String> = existing_blocks
            .iter()
            .filter_map(|b| b.get("instance_id").and_then(Value::as_str))
            .map(str::to_string)
            .collect();
        let matched_blocks: Vec<&Value> = all
            .iter()
            .filter(|fb| {
                fb.get("instance_id")
                    .and_then(Value::as_str)
                    .map(|i| existing_ids.contains(i))
                    .unwrap_or(false)
            })
            .collect();
        let general_count = all.len() - matched_blocks.len();
        Ok(json!({
            "matched_blocks": matched_blocks,
            "active_form_revision": null,
            "general_count": general_count,
            "recent_notes": all.iter().take(8).collect::<Vec<_>>(),
        }))
    }

    fn research_log_ids(&self) -> Result<Vec<String>> {
        let mut stmt = self.conn.prepare(
            "SELECT research_id FROM report_research_log WHERE run_id = ?1 ORDER BY asked_at ASC",
        )?;
        let rows = stmt.query_map(params![self.run.run_id], |row| row.get::<_, String>(0))?;
        let mut out = Vec::new();
        for row in rows {
            out.push(row?);
        }
        Ok(out)
    }

    fn recent_research_notes(&self, limit: usize) -> Result<Vec<Value>> {
        let mut stmt = self.conn.prepare(
            "SELECT research_id, question, focus, asked_at, summary, sources_count, raw_payload_json
             FROM report_research_log
             WHERE run_id = ?1
             ORDER BY asked_at DESC
             LIMIT ?2",
        )?;
        let limit_i = limit as i64;
        let rows = stmt.query_map(params![self.run.run_id, limit_i], |row| {
            let raw: Option<String> = row.get(6)?;
            let raw_payload: Value = raw
                .as_deref()
                .and_then(|s| serde_json::from_str(s).ok())
                .unwrap_or(Value::Null);
            Ok(json!({
                "research_id": row.get::<_, String>(0)?,
                "question": row.get::<_, String>(1)?,
                "focus": row.get::<_, Option<String>>(2)?,
                "asked_at": row.get::<_, String>(3)?,
                "summary": row.get::<_, Option<String>>(4)?,
                "sources_count": row.get::<_, i64>(5)?,
                "raw_payload": raw_payload,
            }))
        })?;
        let mut out = Vec::new();
        for row in rows {
            out.push(row?);
        }
        Ok(out)
    }

    fn skill_run_ids_with_pending(&self) -> Result<Vec<String>> {
        let mut stmt = self
            .conn
            .prepare("SELECT DISTINCT skill_run_id FROM report_pending_blocks WHERE run_id = ?1")?;
        let rows = stmt.query_map(params![self.run.run_id], |row| row.get::<_, String>(0))?;
        let mut out = Vec::new();
        for row in rows {
            out.push(row?);
        }
        Ok(out)
    }

    fn last_check_payload(&self, kind: &str) -> Result<Value> {
        let value = self
            .conn
            .query_row(
                "SELECT payload_json, ready_to_finish, needs_revision, checked_at
                 FROM report_check_runs
                 WHERE run_id = ?1 AND check_kind = ?2
                 ORDER BY checked_at DESC LIMIT 1",
                params![self.run.run_id, kind],
                |row| {
                    let payload_text: Option<String> = row.get(0)?;
                    let payload: Value = payload_text
                        .as_deref()
                        .and_then(|s| serde_json::from_str(s).ok())
                        .unwrap_or(Value::Null);
                    let ready: i64 = row.get(1)?;
                    let needs: i64 = row.get(2)?;
                    let checked_at: String = row.get(3)?;
                    Ok(json!({
                        "ready_to_finish": ready != 0,
                        "needs_revision": needs != 0,
                        "checked_at": checked_at,
                        "payload": payload,
                    }))
                },
            )
            .optional()?
            .unwrap_or(Value::Null);
        Ok(value)
    }

    fn document_flow_value(
        &self,
        blueprint: &DocumentBlueprint,
        expected_blocks: &[Value],
    ) -> Value {
        let mut grouped: BTreeMap<String, Vec<&Value>> = BTreeMap::new();
        for block in expected_blocks {
            if let Some(doc_id) = block.get("doc_id").and_then(Value::as_str) {
                grouped.entry(doc_id.to_string()).or_default().push(block);
            }
        }
        let mut docs: Vec<Value> = Vec::with_capacity(blueprint.base_docs.len());
        for doc in &blueprint.base_docs {
            let mut blocks = grouped.remove(&doc.id).unwrap_or_default();
            blocks.sort_by_key(|b| b.get("order").and_then(Value::as_i64).unwrap_or(i64::MAX));
            let blocks_payload: Vec<Value> = blocks
                .into_iter()
                .map(|b| {
                    json!({
                        "instance_id": b.get("instance_id").cloned().unwrap_or(Value::Null),
                        "title": b.get("title").cloned().unwrap_or(Value::Null),
                        "order": b.get("order").cloned().unwrap_or(Value::Null),
                        "required": b.get("required").cloned().unwrap_or(Value::Bool(false)),
                    })
                })
                .collect();
            docs.push(json!({
                "doc_id": doc.id,
                "doc_title": doc.title,
                "blocks": blocks_payload,
            }));
        }
        Value::Array(docs)
    }
}

/// Compute completeness across the resolved expected_blocks. A block is
/// `done` if its committed markdown reaches 65% of `min_chars`,
/// `thin` if it is committed but below that floor, `missing` if not
/// committed at all. `ready_to_finish` is `total_required > 0 && all
/// required blocks are done`.
pub fn compute_completeness(asset_pack: &AssetPack, expected_blocks: &[Value]) -> Result<Value> {
    let mut total_required = 0usize;
    let mut done_required = 0usize;
    let mut missing_required: Vec<String> = Vec::new();
    let mut thin_required: Vec<String> = Vec::new();
    let mut missing_optional: Vec<String> = Vec::new();
    for block in expected_blocks {
        let required = block
            .get("required")
            .and_then(Value::as_bool)
            .unwrap_or(false);
        let status = block
            .get("status")
            .and_then(Value::as_str)
            .unwrap_or("missing");
        let instance_id = block
            .get("instance_id")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string();
        if required {
            total_required += 1;
            match status {
                "done" => done_required += 1,
                "thin" => thin_required.push(instance_id),
                _ => missing_required.push(instance_id),
            }
        } else if status == "missing" {
            missing_optional.push(instance_id);
        }
    }
    let _ = asset_pack;
    let ready_to_finish =
        total_required > 0 && done_required == total_required && thin_required.is_empty();
    Ok(json!({
        "total_required": total_required,
        "done_required": done_required,
        "missing_required": missing_required,
        "thin_required": thin_required,
        "missing_optional": missing_optional,
        "ready_to_finish": ready_to_finish,
    }))
}

// ----- helpers -----

fn load_blocks(conn: &Connection, run_id: &str, pending: bool) -> Result<Vec<BlockRecord>> {
    if pending {
        let mut stmt = conn.prepare(
            "SELECT instance_id, doc_id, block_id, block_template_id, title, ord,
                    markdown, reason, used_skill_ids_json, used_research_ids_json,
                    used_reference_ids_json, committed_at, kind, skill_run_id
             FROM report_pending_blocks WHERE run_id = ?1
             ORDER BY committed_at ASC",
        )?;
        let rows = stmt.query_map(params![run_id], |row| Ok(row_to_block(row, true)))?;
        let mut out = Vec::new();
        for row in rows {
            out.push(row??);
        }
        Ok(out)
    } else {
        let mut stmt = conn.prepare(
            "SELECT instance_id, doc_id, block_id, block_template_id, title, ord,
                    markdown, reason, used_skill_ids_json, used_research_ids_json,
                    used_reference_ids_json, committed_at, NULL, NULL
             FROM report_blocks WHERE run_id = ?1
             ORDER BY ord ASC",
        )?;
        let rows = stmt.query_map(params![run_id], |row| Ok(row_to_block(row, false)))?;
        let mut out = Vec::new();
        for row in rows {
            out.push(row??);
        }
        Ok(out)
    }
}

fn row_to_block(row: &rusqlite::Row<'_>, pending: bool) -> Result<BlockRecord> {
    let used_skill_ids: Vec<String> = decode_json_string_list(row, 8);
    let used_research_ids: Vec<String> = decode_json_string_list(row, 9);
    let used_reference_ids: Vec<String> = decode_json_string_list(row, 10);
    let kind: Option<String> = if pending { row.get(12).ok() } else { None };
    let skill_run_id: Option<String> = if pending { row.get(13).ok() } else { None };
    Ok(BlockRecord {
        instance_id: row.get(0)?,
        doc_id: row.get(1)?,
        block_id: row.get(2)?,
        block_template_id: row.get(3)?,
        title: row.get(4)?,
        ord: row.get(5)?,
        markdown: row.get(6)?,
        reason: row.get(7)?,
        used_skill_ids,
        used_research_ids,
        used_reference_ids,
        committed_at: row.get(11)?,
        kind,
        skill_run_id,
    })
}

fn decode_json_string_list(row: &rusqlite::Row<'_>, idx: usize) -> Vec<String> {
    let raw: Option<String> = row.get(idx).unwrap_or(None);
    raw.as_deref()
        .and_then(|s| serde_json::from_str::<Vec<String>>(s).ok())
        .unwrap_or_default()
}

fn build_expected_blocks(
    pack: &AssetPack,
    report_type_id: &str,
    blueprint: &DocumentBlueprint,
    committed_by_instance: &HashMap<String, &BlockRecord>,
    modules: &HashMap<String, &OptionalModule>,
) -> Result<Vec<Value>> {
    let report_type = pack.report_type(report_type_id)?;
    let allowed: HashSet<&str> = report_type
        .block_library_keys
        .iter()
        .map(String::as_str)
        .collect();
    let active_modules: HashSet<&str> = report_type
        .default_modules
        .iter()
        .map(String::as_str)
        .collect();

    let mut out: Vec<Value> = Vec::new();
    for entry in &blueprint.sequence {
        if !allowed.contains(entry.block_id.as_str()) {
            // Cross-type block in the blueprint — skip silently. The
            // `validate()` call at bootstrap should have caught this.
            continue;
        }
        // If this slot is gated by a module and the module is inactive,
        // it does not appear in the expected list.
        if let Some(module_id) = &entry.module {
            if !active_modules.contains(module_id.as_str())
                && !modules.contains_key(module_id.as_str())
            {
                // Module not declared at all -> skip
                continue;
            }
            if !active_modules.contains(module_id.as_str()) {
                // Declared but not active -> still skip; it's optional.
                continue;
            }
        }
        let instance_id = format!("{}__{}", entry.doc_id, entry.block_id);
        let entry_def = pack
            .block_library_entry(&entry.block_id)
            .with_context(|| format!("blueprint references unknown block {}", entry.block_id))?;
        let status = block_status(committed_by_instance.get(&instance_id), &entry_def);
        out.push(json!({
            "instance_id": instance_id,
            "block_id": entry.block_id,
            "template_id": entry.block_id,
            "title": entry_def.title,
            "doc_id": entry.doc_id,
            "order": entry.order,
            "required": entry.required,
            "module": entry.module,
            "status": status,
            "min_chars": entry_def.min_chars,
        }));
    }
    Ok(out)
}

fn block_status(record: Option<&&BlockRecord>, entry: &BlockLibraryEntry) -> &'static str {
    match record {
        None => "missing",
        Some(block) => {
            let chars = block.markdown.trim().chars().count();
            let floor = ((entry.min_chars as f64) * 0.65).ceil() as usize;
            if entry.min_chars == 0 || chars >= floor {
                "done"
            } else if chars == 0 {
                "missing"
            } else {
                "thin"
            }
        }
    }
}

fn build_existing_blocks(committed: &[BlockRecord]) -> Vec<Value> {
    committed
        .iter()
        .map(|b| {
            json!({
                "instance_id": b.instance_id,
                "block_id": b.block_id,
                "title": b.title,
                "doc_id": b.doc_id,
                "chars": b.markdown.chars().count(),
                "reason": b.reason,
            })
        })
        .collect()
}

fn build_pending_blocks_payload(pending: &[BlockRecord]) -> Vec<Value> {
    pending
        .iter()
        .map(|b| {
            json!({
                "instance_id": b.instance_id,
                "block_id": b.block_id,
                "title": b.title,
                "doc_id": b.doc_id,
                "chars": b.markdown.chars().count(),
                "reason": b.reason,
                "skill_run_id": b.skill_run_id,
                "kind": b.kind,
            })
        })
        .collect()
}

fn references_to_payload(pack: &AssetPack, block_template_ids: &[String]) -> Vec<Value> {
    let resources: Vec<&ReferenceResource> = pack.references_for_blocks(block_template_ids);
    resources
        .into_iter()
        .map(|r| {
            json!({
                "id": r.id,
                "kind": "reference_resource",
                "title": r.story_role,
                "citation": r.source_file,
                "excerpt": r.excerpt,
                "source_url": null,
                "doi": null,
                "license": null,
                "usage_rights": null,
                "block_template_ids": r.block_template_ids,
                "why_it_works": r.why_it_works,
                "reuse_moves": r.reuse_moves,
            })
        })
        .collect()
}

fn report_type_to_value(report_type: &crate::report::asset_pack::ReportType) -> Value {
    json!({
        "id": report_type.id,
        "label": report_type.label,
        "purpose": report_type.purpose,
        "verdict_line_pattern": report_type.verdict_line_pattern,
        "verdict_vocabulary": report_type.verdict_vocabulary,
        "typical_chars": report_type.typical_chars,
        "min_sections": report_type.min_sections,
        "block_library_keys": report_type.block_library_keys,
        "document_blueprint_id": report_type.document_blueprint_id,
        "default_modules": report_type.default_modules,
        "reference_archetype_ids": report_type.reference_archetype_ids,
    })
}

fn character_budget_to_value(budget: &CharacterBudget) -> Value {
    json!({
        "target_chars": budget.target_chars,
        "actual_chars": budget.actual_chars,
        "delta_chars": budget.delta_chars,
        "tolerance": 0.20_f64,
        "within_tolerance": budget.within_tolerance,
        "severely_off_target": budget.severely_off_target,
        "reference_average_chars": budget.reference_average_chars,
        "status": budget.status,
    })
}

fn report_type_arc(report_type_id: &str) -> Vec<&'static str> {
    match report_type_id {
        "feasibility_study" => vec![
            "Frage",
            "Domänenmodell",
            "Anforderungen",
            "Optionsraum",
            "Bewertungslogik",
            "Matrix",
            "Szenarien",
            "Detailbewertung",
            "Risiken",
            "Empfehlung",
        ],
        "market_research" => vec![
            "Markt",
            "Segmente",
            "Treiber",
            "Wettbewerb",
            "Eintrittsoptionen",
            "Empfehlung",
        ],
        "competitive_analysis" => vec![
            "Scope",
            "Wettbewerber",
            "Capability-Matrix",
            "Positionierung",
            "Lücken",
            "Empfehlung",
        ],
        "technology_screening" => vec![
            "Screening-Frage",
            "Longlist",
            "Kriterien",
            "Matrix",
            "Shortlist",
            "nächste Schritte",
        ],
        "whitepaper" => vec!["These", "Kontext", "Argumentationskette", "Implikationen"],
        "literature_review" => vec![
            "Scope",
            "Methode",
            "Themen-Synthese",
            "themenübergreifende Integration",
            "offene Forschungsfragen",
        ],
        "decision_brief" => vec!["Entscheidungsfrage", "Optionen", "Bewertung", "Empfehlung"],
        "project_description" => vec![
            "Unternehmen",
            "Problem",
            "Innovationsvorhaben",
            "Zielbild",
            "Marktabgrenzung",
            "Umsetzung",
            "Umfang",
            "wirtschaftlicher Nutzen",
        ],
        "source_review" => vec![
            "Suchauftrag",
            "Suchstrategie",
            "Taxonomie",
            "Quellenlandschaft",
            "Quellenkatalog",
            "Datenextraktion",
            "Gruppensynthese",
            "Abdeckung/Luecken",
            "Priorisierung",
        ],
        _ => Vec::new(),
    }
}

#[allow(dead_code)]
fn _unused_anyhow_marker() -> Result<()> {
    // Keeps `anyhow!` imported even if every call site folds it into
    // `Context::with_context`. Trimmed by the optimiser.
    Err(anyhow!("unreachable"))
}
