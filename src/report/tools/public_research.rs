//! `public_research` tool. Resolves seed identifiers and runs a
//! free-form web research query, then folds every DOI / arXiv id the
//! bundle surfaced back through the resolver chain. Every resolved
//! source lands in `report_evidence_register`; the call itself is
//! recorded in `report_research_log`.

use anyhow::{Context, Result};
use rusqlite::params;
use serde::Deserialize;
use serde_json::{json, Value};

use crate::report::schema::{ensure_schema, new_id, now_iso, open};
use crate::report::sources::{
    EvidenceCache, WebResearchDepth, WebResearchOutcome, WebResearchQuery,
};
use crate::report::tools::{err, ok, ToolContext, ToolEnvelope};

const TOOL: &str = "public_research";

fn default_depth() -> String {
    "standard".to_string()
}

fn default_max_sources() -> usize {
    8
}

#[derive(Debug, Clone, Deserialize)]
pub struct Args {
    #[serde(default)]
    pub question: String,
    #[serde(default)]
    pub focus: String,
    #[serde(default = "default_depth")]
    pub depth: String,
    #[serde(default = "default_max_sources")]
    pub max_sources: usize,
    #[serde(default)]
    pub seed_dois: Vec<String>,
    #[serde(default)]
    pub seed_arxiv_ids: Vec<String>,
}

pub fn execute(ctx: &ToolContext, args: &Args) -> Result<ToolEnvelope> {
    // Per-run research budget enforcement.
    let metadata = ctx.workspace.run_metadata()?;
    let depth_profile = ctx.asset_pack.depth_profile(&metadata.depth_profile_id)?;
    let budget = depth_profile.research_budget;

    let conn = open(ctx.root)?;
    ensure_schema(&conn)?;

    if let Some(budget) = budget {
        let used: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM report_research_log WHERE run_id = ?1",
                params![ctx.run_id],
                |row| row.get(0),
            )
            .context("failed to count research_log rows")?;
        if used >= budget as i64 {
            return Ok(err(TOOL, "research budget exceeded".into()));
        }
    }

    let mut sources_resolved: usize = 0;
    let mut sources_unresolved: usize = 0;

    // Seed DOIs.
    for doi in &args.seed_dois {
        match ctx.resolver.resolve_doi(doi)? {
            Some(source) => {
                ctx.resolver.record_into_register(&source)?;
                sources_resolved += 1;
            }
            None => sources_unresolved += 1,
        }
    }

    // Seed arXiv ids.
    for arxiv_id in &args.seed_arxiv_ids {
        match ctx.resolver.resolve_arxiv(arxiv_id)? {
            Some(source) => {
                ctx.resolver.record_into_register(&source)?;
                sources_resolved += 1;
            }
            None => sources_unresolved += 1,
        }
    }

    // Web research call (only when a question is supplied).
    let mut summary = String::new();
    let mut raw_bundle: Value = Value::Null;
    let mut dois_extracted: Vec<String> = Vec::new();
    let mut arxiv_extracted: Vec<String> = Vec::new();
    if !args.question.trim().is_empty() {
        let depth = WebResearchDepth::from_label(&args.depth).unwrap_or_default();
        let focus = if args.focus.trim().is_empty() {
            None
        } else {
            Some(args.focus.clone())
        };
        let query = WebResearchQuery {
            question: args.question.clone(),
            focus,
            depth,
            max_sources: args.max_sources,
            workspace_path: None,
        };
        let outcome: WebResearchOutcome = ctx.resolver.execute_query(&query)?;
        summary = outcome.summary.clone();
        raw_bundle = outcome.raw_bundle.clone();
        dois_extracted = outcome.doi_extracted.clone();
        arxiv_extracted = outcome.arxiv_extracted.clone();

        // Resolve every extracted DOI.
        for doi in &dois_extracted {
            match ctx.resolver.resolve_doi(doi)? {
                Some(source) => {
                    ctx.resolver.record_into_register(&source)?;
                    sources_resolved += 1;
                }
                None => sources_unresolved += 1,
            }
        }
        // Resolve every extracted arXiv id.
        for arxiv_id in &arxiv_extracted {
            match ctx.resolver.resolve_arxiv(arxiv_id)? {
                Some(source) => {
                    ctx.resolver.record_into_register(&source)?;
                    sources_resolved += 1;
                }
                None => sources_unresolved += 1,
            }
        }
    } else if args.seed_dois.is_empty() && args.seed_arxiv_ids.is_empty() {
        return Ok(err(
            TOOL,
            "public_research requires either a question or seed identifiers".into(),
        ));
    }

    // Tally evidence_register total size for this run.
    let register_size = {
        let register_conn = crate::report::sources::open_register_conn(ctx.root)?;
        let cache = EvidenceCache::new(&register_conn, ctx.run_id);
        cache.count()?
    };

    // Persist the research_log row.
    let research_id = new_id("research");
    let now = now_iso();
    let raw_payload_json = serde_json::to_string(&json!({
        "depth": args.depth,
        "max_sources": args.max_sources,
        "seed_dois": args.seed_dois,
        "seed_arxiv_ids": args.seed_arxiv_ids,
        "dois_extracted": dois_extracted,
        "arxiv_extracted": arxiv_extracted,
        "raw_bundle": raw_bundle,
    }))
    .context("encode public_research raw payload")?;
    let summary_for_db = if summary.is_empty() {
        format!(
            "seed dois: {}, seed arxiv: {}",
            args.seed_dois.len(),
            args.seed_arxiv_ids.len()
        )
    } else {
        summary.clone()
    };
    conn.execute(
        "INSERT INTO report_research_log (
             research_id, run_id, question, focus, asked_at, resolver,
             summary, sources_count, raw_payload_json
         ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
        params![
            research_id,
            ctx.run_id,
            args.question,
            args.focus,
            now,
            "resolver_stack",
            summary_for_db,
            sources_resolved as i64,
            raw_payload_json,
        ],
    )
    .context("failed to insert report_research_log row")?;

    let data = json!({
        "research_id": research_id,
        "question": args.question,
        "focus": args.focus,
        "summary": summary,
        "sources_resolved": sources_resolved,
        "sources_unresolved": sources_unresolved,
        "evidence_register_size": register_size,
        "dois_extracted": dois_extracted,
        "arxiv_extracted": arxiv_extracted,
    });
    Ok(ok(TOOL, data))
}
