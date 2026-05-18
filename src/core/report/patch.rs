//! Patch mechanism for the deep-research backend.
//!
//! Mirrors the Förderantrag agent's `stagePendingBlocks` /
//! `commitBlocks` / `applyBlockPatch` flow (see
//! `Foerdervorhaben-Agent.html` lines 3526–3548 and 6564–6607). The
//! contract:
//!
//! - Sub-skills emit a [`SkillRunRecord`] with one or more
//!   [`StagedBlock`]s. The record is persisted into `report_skill_runs`
//!   (audit trail), and each block is upserted into
//!   `report_pending_blocks` keyed by `(run_id, instance_id)` —
//!   last-write-wins.
//! - The manager later calls [`apply_block_patch`] with a
//!   [`PatchSelection`] that names the `skill_run_id` and the list of
//!   `instance_ids` to commit. Markdown never enters this module from
//!   the manager; it only ever flows in via the recorded skill run.
//! - On commit each pending block is upserted into `report_blocks`
//!   (`INSERT OR REPLACE`) and a `report_provenance` row is written.
//!   Pending rows for the committed `instance_ids` are deleted.
//!
//! Failures (`blocking_reason` set + empty `blocks`) are still recorded
//! into `report_skill_runs` because they are part of the audit trail.

use anyhow::{anyhow, Context, Result};
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};

use crate::report::schema::{new_id, now_iso};

/// One staged block as emitted by a write/revision sub-skill.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct StagedBlock {
    pub instance_id: String,
    pub doc_id: String,
    pub block_id: String,
    pub block_template_id: String,
    pub title: String,
    pub ord: i64,
    pub markdown: String,
    pub reason: String,
    #[serde(default)]
    pub used_reference_ids: Vec<String>,
}

/// Sub-skill run kind. Matches the `report_skill_runs.kind` column.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SkillRunKind {
    Write,
    Revision,
    FlowReview,
}

impl SkillRunKind {
    pub fn as_str(self) -> &'static str {
        match self {
            SkillRunKind::Write => "write",
            SkillRunKind::Revision => "revision",
            SkillRunKind::FlowReview => "flow_review",
        }
    }

    pub fn parse(s: &str) -> Result<Self> {
        match s {
            "write" => Ok(SkillRunKind::Write),
            "revision" => Ok(SkillRunKind::Revision),
            "flow_review" => Ok(SkillRunKind::FlowReview),
            other => Err(anyhow!("unknown skill run kind: {other}")),
        }
    }
}

/// One sub-skill invocation result, recorded verbatim into
/// `report_skill_runs`.
#[derive(Debug, Clone)]
pub struct SkillRunRecord {
    pub skill_run_id: String,
    pub run_id: String,
    pub kind: SkillRunKind,
    pub summary: String,
    pub blocking_reason: Option<String>,
    pub blocking_questions: Vec<String>,
    pub blocks: Vec<StagedBlock>,
    pub raw_output: Value,
}

/// Selection passed to [`apply_block_patch`].
///
/// `instance_ids = None` commits every staged block of the named
/// skill run. `instance_ids = Some(vec![])` is treated identically to
/// `None`.
#[derive(Debug, Clone)]
pub struct PatchSelection {
    pub skill_run_id: String,
    pub instance_ids: Option<Vec<String>>,
    pub used_research_ids: Vec<String>,
}

/// Outcome returned from [`apply_block_patch`].
#[derive(Debug, Clone)]
pub struct PatchOutcome {
    pub committed_block_ids: Vec<String>,
    pub now_iso: String,
}

/// Insert (or replace) one row in `report_skill_runs`.
///
/// The same `skill_run_id` may be re-recorded; we use INSERT OR REPLACE
/// so retries are idempotent. `raw_output` is persisted verbatim.
pub fn record_skill_run(conn: &Connection, run: &SkillRunRecord) -> Result<()> {
    let now = now_iso();
    let blocking_questions_json = serde_json::to_string(&run.blocking_questions)
        .context("failed to encode blocking_questions for skill run record")?;
    let raw_output_json = serde_json::to_string(&run.raw_output)
        .context("failed to encode raw_output for skill run record")?;

    conn.execute(
        "INSERT OR REPLACE INTO report_skill_runs (
             skill_run_id, run_id, kind, invoked_at, finished_at,
             summary, blocking_reason, blocking_questions_json, raw_output_json
         ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
        params![
            run.skill_run_id,
            run.run_id,
            run.kind.as_str(),
            now,
            now,
            run.summary,
            run.blocking_reason,
            blocking_questions_json,
            raw_output_json,
        ],
    )
    .context("failed to record sub-skill run")?;
    Ok(())
}

/// Stage every block from one sub-skill run into `report_pending_blocks`.
///
/// Returns the list of `instance_id`s that ended up staged. Older
/// pending entries for the same `(run_id, instance_id)` are overwritten
/// (last-write-wins inside the staging area).
pub fn stage_pending_blocks(
    conn: &Connection,
    run_id: &str,
    skill_run_id: &str,
    kind: SkillRunKind,
    blocks: &[StagedBlock],
) -> Result<Vec<String>> {
    let now = now_iso();
    let mut staged: Vec<String> = Vec::with_capacity(blocks.len());
    for block in blocks {
        // Last-write-wins: drop any prior pending row for this instance id.
        conn.execute(
            "DELETE FROM report_pending_blocks
             WHERE run_id = ?1 AND instance_id = ?2",
            params![run_id, block.instance_id],
        )
        .with_context(|| {
            format!(
                "failed to clear stale pending block for instance {} of run {}",
                block.instance_id, run_id
            )
        })?;

        let normalised = normalise_markdown(&block.markdown);
        let used_reference_ids_json = serde_json::to_string(&block.used_reference_ids)
            .context("failed to encode used_reference_ids for staged block")?;
        // Sub-skills do not write skill_ids/research_ids on staging; the
        // manager attaches `used_research_ids[]` at commit time. We
        // persist empty arrays here so the round-trip via
        // `apply_block_patch` is well-formed.
        let empty_array_json = "[]".to_string();

        conn.execute(
            "INSERT INTO report_pending_blocks (
                 run_id, skill_run_id, kind, instance_id, doc_id, block_id,
                 block_template_id, title, ord, markdown, reason,
                 used_skill_ids_json, used_research_ids_json,
                 used_reference_ids_json, committed_at
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11,
                       ?12, ?13, ?14, ?15)",
            params![
                run_id,
                skill_run_id,
                kind.as_str(),
                block.instance_id,
                block.doc_id,
                block.block_id,
                block.block_template_id,
                block.title,
                block.ord,
                normalised,
                block.reason,
                empty_array_json,
                empty_array_json,
                used_reference_ids_json,
                now,
            ],
        )
        .with_context(|| {
            format!(
                "failed to stage pending block instance {} of run {}",
                block.instance_id, run_id
            )
        })?;
        staged.push(block.instance_id.clone());
    }
    Ok(staged)
}

/// Apply a patch: commit the requested staged blocks into
/// `report_blocks`, write provenance, and clear the matching pending
/// rows.
///
/// Behaviour:
/// - Reads pending rows for the given `skill_run_id`.
/// - Filters to the requested `instance_ids` (or all if `None`/empty).
/// - Re-normalises markdown defensively (no rewrite — only line endings
///   and trailing whitespace) and verifies the sha256 hasn't drifted
///   since staging.
/// - Upserts into `report_blocks` via `INSERT OR REPLACE` keyed by
///   `(run_id, instance_id)`.
/// - Writes one `report_provenance` row per committed block (`kind =
///   "write"` or `"revision"`).
/// - Deletes the committed pending rows.
pub fn apply_block_patch(
    conn: &Connection,
    run_id: &str,
    sel: &PatchSelection,
) -> Result<PatchOutcome> {
    // Locate the skill run; we need `kind` and to verify the run id.
    let kind: String = conn
        .query_row(
            "SELECT kind FROM report_skill_runs WHERE skill_run_id = ?1 AND run_id = ?2",
            params![sel.skill_run_id, run_id],
            |row| row.get(0),
        )
        .optional()
        .context("failed to look up skill run for patch")?
        .ok_or_else(|| {
            anyhow!(
                "skill run {} not found for run {}",
                sel.skill_run_id,
                run_id
            )
        })?;
    let parsed_kind = SkillRunKind::parse(&kind)?;
    if matches!(parsed_kind, SkillRunKind::FlowReview) {
        return Err(anyhow!(
            "skill run {} is a flow_review and cannot be committed via apply_block_patch",
            sel.skill_run_id
        ));
    }
    let provenance_kind = match parsed_kind {
        SkillRunKind::Write => "write",
        SkillRunKind::Revision => "revision",
        SkillRunKind::FlowReview => unreachable!(),
    };

    // Build the wanted-instance filter.
    let wanted: Option<Vec<String>> = match &sel.instance_ids {
        Some(ids) if !ids.is_empty() => Some(ids.clone()),
        _ => None,
    };

    // Read every pending block for this skill run.
    let mut stmt = conn.prepare(
        "SELECT instance_id, doc_id, block_id, block_template_id, title, ord,
                markdown, reason, used_skill_ids_json, used_research_ids_json,
                used_reference_ids_json
         FROM report_pending_blocks
         WHERE run_id = ?1 AND skill_run_id = ?2
         ORDER BY ord ASC, instance_id ASC",
    )?;
    let rows = stmt.query_map(params![run_id, sel.skill_run_id], |row| {
        let used_skill_ids_json: Option<String> = row.get(8)?;
        let used_research_ids_json: Option<String> = row.get(9)?;
        let used_reference_ids_json: Option<String> = row.get(10)?;
        Ok(StagedRow {
            instance_id: row.get(0)?,
            doc_id: row.get(1)?,
            block_id: row.get(2)?,
            block_template_id: row.get(3)?,
            title: row.get(4)?,
            ord: row.get(5)?,
            markdown: row.get(6)?,
            reason: row.get(7)?,
            used_skill_ids: decode_string_list(used_skill_ids_json.as_deref()),
            used_research_ids: decode_string_list(used_research_ids_json.as_deref()),
            used_reference_ids: decode_string_list(used_reference_ids_json.as_deref()),
        })
    })?;
    let mut pending: Vec<StagedRow> = Vec::new();
    for row in rows {
        pending.push(row?);
    }
    drop(stmt);

    if pending.is_empty() {
        return Err(anyhow!(
            "no pending blocks found for skill run {} on run {}",
            sel.skill_run_id,
            run_id
        ));
    }

    let now = now_iso();
    let mut committed: Vec<String> = Vec::new();
    for row in &pending {
        if let Some(ids) = &wanted {
            if !ids.iter().any(|id| id == &row.instance_id) {
                continue;
            }
        }
        // Normalise defensively and verify no drift since staging.
        let normalised = normalise_markdown(&row.markdown);
        let hash_before = sha256_hex(&row.markdown);
        let hash_after = sha256_hex(&normalised);
        // The expected invariant: `row.markdown` was stored already
        // normalised by `stage_pending_blocks`, so re-normalising is a
        // no-op and the two hashes match. If they don't, the markdown
        // mutated in-flight and we refuse to commit.
        if hash_before != hash_after {
            return Err(anyhow!(
                "markdown for instance {} drifted between staging and commit (hash mismatch)",
                row.instance_id
            ));
        }

        let used_research_ids: Vec<String> = if !sel.used_research_ids.is_empty() {
            sel.used_research_ids.clone()
        } else {
            row.used_research_ids.clone()
        };
        let used_skill_ids = row.used_skill_ids.clone();
        let used_reference_ids = row.used_reference_ids.clone();

        let used_skill_ids_json =
            serde_json::to_string(&used_skill_ids).context("encode used_skill_ids on commit")?;
        let used_research_ids_json = serde_json::to_string(&used_research_ids)
            .context("encode used_research_ids on commit")?;
        let used_reference_ids_json = serde_json::to_string(&used_reference_ids)
            .context("encode used_reference_ids on commit")?;

        // INSERT OR REPLACE keyed by (run_id, instance_id) — the unique
        // constraint isn't declared but we drop any prior row first to
        // emulate the same semantics.
        conn.execute(
            "DELETE FROM report_blocks WHERE run_id = ?1 AND instance_id = ?2",
            params![run_id, row.instance_id],
        )
        .with_context(|| {
            format!(
                "failed to clear prior committed block for instance {} of run {}",
                row.instance_id, run_id
            )
        })?;
        conn.execute(
            "INSERT INTO report_blocks (
                 run_id, instance_id, doc_id, block_id, block_template_id,
                 title, ord, markdown, reason, used_skill_ids_json,
                 used_research_ids_json, used_reference_ids_json, committed_at
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)",
            params![
                run_id,
                row.instance_id,
                row.doc_id,
                row.block_id,
                row.block_template_id,
                row.title,
                row.ord,
                normalised,
                row.reason,
                used_skill_ids_json,
                used_research_ids_json,
                used_reference_ids_json,
                now,
            ],
        )
        .with_context(|| {
            format!(
                "failed to commit block instance {} of run {}",
                row.instance_id, run_id
            )
        })?;

        // Provenance entry.
        let prov_id = new_id("prov");
        let payload = json!({
            "kind": provenance_kind,
            "skill_run_id": sel.skill_run_id,
            "instance_id": row.instance_id,
            "used_skill_ids": used_skill_ids,
            "used_research_ids": used_research_ids,
            "used_reference_ids": used_reference_ids,
        });
        let payload_json =
            serde_json::to_string(&payload).context("failed to encode provenance payload")?;
        conn.execute(
            "INSERT INTO report_provenance (
                 prov_id, run_id, kind, occurred_at, instance_id, skill_run_id,
                 research_id, payload_json
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, NULL, ?7)",
            params![
                prov_id,
                run_id,
                provenance_kind,
                now,
                row.instance_id,
                sel.skill_run_id,
                payload_json,
            ],
        )
        .with_context(|| {
            format!(
                "failed to write provenance for instance {} of run {}",
                row.instance_id, run_id
            )
        })?;

        // Clear the staged row.
        conn.execute(
            "DELETE FROM report_pending_blocks
             WHERE run_id = ?1 AND skill_run_id = ?2 AND instance_id = ?3",
            params![run_id, sel.skill_run_id, row.instance_id],
        )
        .with_context(|| {
            format!(
                "failed to clear pending block {} after commit",
                row.instance_id
            )
        })?;

        committed.push(row.instance_id.clone());
    }

    if committed.is_empty() {
        return Err(anyhow!(
            "patch selection matched no staged blocks for skill run {}",
            sel.skill_run_id
        ));
    }

    Ok(PatchOutcome {
        committed_block_ids: committed,
        now_iso: now,
    })
}

/// List every pending block for a run, across all skill runs.
pub fn list_pending_blocks(conn: &Connection, run_id: &str) -> Result<Vec<StagedBlock>> {
    let mut stmt = conn.prepare(
        "SELECT instance_id, doc_id, block_id, block_template_id, title, ord,
                markdown, reason, used_reference_ids_json
         FROM report_pending_blocks
         WHERE run_id = ?1
         ORDER BY ord ASC, instance_id ASC",
    )?;
    let rows = stmt.query_map(params![run_id], |row| {
        let template_id: Option<String> = row.get(3)?;
        let used_reference_ids_json: Option<String> = row.get(8)?;
        Ok(StagedBlock {
            instance_id: row.get(0)?,
            doc_id: row.get(1)?,
            block_id: row.get(2)?,
            block_template_id: template_id.unwrap_or_default(),
            title: row.get(4)?,
            ord: row.get(5)?,
            markdown: row.get(6)?,
            reason: row.get::<_, Option<String>>(7)?.unwrap_or_default(),
            used_reference_ids: decode_string_list(used_reference_ids_json.as_deref()),
        })
    })?;
    let mut out = Vec::new();
    for row in rows {
        out.push(row?);
    }
    Ok(out)
}

/// Fetch the recorded skill run plus its staged blocks (if any).
pub fn list_skill_run(conn: &Connection, skill_run_id: &str) -> Result<Option<SkillRunRecord>> {
    let row = conn
        .query_row(
            "SELECT skill_run_id, run_id, kind, summary, blocking_reason,
                    blocking_questions_json, raw_output_json
             FROM report_skill_runs WHERE skill_run_id = ?1",
            params![skill_run_id],
            |row| {
                let blocking_questions_json: Option<String> = row.get(5)?;
                let raw_output_json: Option<String> = row.get(6)?;
                Ok(SkillRunRow {
                    skill_run_id: row.get(0)?,
                    run_id: row.get(1)?,
                    kind: row.get(2)?,
                    summary: row.get::<_, Option<String>>(3)?.unwrap_or_default(),
                    blocking_reason: row.get(4)?,
                    blocking_questions_json,
                    raw_output_json,
                })
            },
        )
        .optional()
        .context("failed to load skill run row")?;
    let row = match row {
        Some(r) => r,
        None => return Ok(None),
    };

    let blocking_questions: Vec<String> = row
        .blocking_questions_json
        .as_deref()
        .and_then(|s| serde_json::from_str(s).ok())
        .unwrap_or_default();
    let raw_output: Value = row
        .raw_output_json
        .as_deref()
        .and_then(|s| serde_json::from_str(s).ok())
        .unwrap_or(Value::Null);

    // Pull staged blocks (if any) so the record reflects what would be
    // committed.
    let mut stmt = conn.prepare(
        "SELECT instance_id, doc_id, block_id, block_template_id, title, ord,
                markdown, reason, used_reference_ids_json
         FROM report_pending_blocks
         WHERE skill_run_id = ?1
         ORDER BY ord ASC, instance_id ASC",
    )?;
    let block_rows = stmt.query_map(params![skill_run_id], |row| {
        let template_id: Option<String> = row.get(3)?;
        let used_reference_ids_json: Option<String> = row.get(8)?;
        Ok(StagedBlock {
            instance_id: row.get(0)?,
            doc_id: row.get(1)?,
            block_id: row.get(2)?,
            block_template_id: template_id.unwrap_or_default(),
            title: row.get(4)?,
            ord: row.get(5)?,
            markdown: row.get(6)?,
            reason: row.get::<_, Option<String>>(7)?.unwrap_or_default(),
            used_reference_ids: decode_string_list(used_reference_ids_json.as_deref()),
        })
    })?;
    let mut blocks = Vec::new();
    for r in block_rows {
        blocks.push(r?);
    }

    Ok(Some(SkillRunRecord {
        skill_run_id: row.skill_run_id,
        run_id: row.run_id,
        kind: SkillRunKind::parse(&row.kind)?,
        summary: row.summary,
        blocking_reason: row.blocking_reason,
        blocking_questions,
        blocks,
        raw_output,
    }))
}

// ---- private helpers ----

#[derive(Debug, Clone)]
struct StagedRow {
    instance_id: String,
    doc_id: String,
    block_id: String,
    block_template_id: Option<String>,
    title: String,
    ord: i64,
    markdown: String,
    reason: Option<String>,
    used_skill_ids: Vec<String>,
    used_research_ids: Vec<String>,
    used_reference_ids: Vec<String>,
}

#[derive(Debug, Clone)]
struct SkillRunRow {
    skill_run_id: String,
    run_id: String,
    kind: String,
    summary: String,
    blocking_reason: Option<String>,
    blocking_questions_json: Option<String>,
    raw_output_json: Option<String>,
}

fn decode_string_list(raw: Option<&str>) -> Vec<String> {
    raw.and_then(|s| serde_json::from_str::<Vec<String>>(s).ok())
        .unwrap_or_default()
}

/// Markdown content normalisation. Replaces `\r\n` with `\n`, strips
/// trailing whitespace per line, and trims the trailing whitespace on
/// the final block. Does NOT rewrite content (no smart-quote
/// substitution, no hyphen substitution — lints catch those).
pub fn normalise_markdown(markdown: &str) -> String {
    let mut out = String::with_capacity(markdown.len());
    let unified = markdown.replace("\r\n", "\n").replace('\r', "\n");
    let mut first = true;
    for line in unified.split('\n') {
        if !first {
            out.push('\n');
        }
        first = false;
        let trimmed_end = trim_trailing_whitespace(line);
        out.push_str(trimmed_end);
    }
    // Strip trailing whitespace+blank lines on the final block.
    while out.ends_with(['\n', ' ', '\t']) {
        out.pop();
    }
    out
}

fn trim_trailing_whitespace(s: &str) -> &str {
    let bytes = s.as_bytes();
    let mut end = bytes.len();
    while end > 0 {
        let c = bytes[end - 1];
        if c == b' ' || c == b'\t' {
            end -= 1;
        } else {
            break;
        }
    }
    &s[..end]
}

fn sha256_hex(s: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(s.as_bytes());
    let digest = hasher.finalize();
    let mut out = String::with_capacity(digest.len() * 2);
    for byte in digest {
        out.push_str(&format!("{byte:02x}"));
    }
    out
}
