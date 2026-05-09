//! `ctox report …` command surface.
//!
//! Deterministic CLI subcommands the harness LLM (loaded with the
//! `skills/system/research/deep-research/` skill) calls via Bash to drive
//! a deep-research run. There is no LLM loop in this module — every
//! command is a pure transform on the SQLite report store.

use std::io::Read;
use std::path::{Path, PathBuf};

use anyhow::{anyhow, bail, Context, Result};
use rusqlite::{params, Connection, OptionalExtension};
use serde_json::{json, Value};

use crate::report::asset_pack::AssetPack;
use crate::report::checks::{
    record_check_outcome, run_character_budget_check, run_completeness_check,
    run_deliverable_quality_check, run_release_guard_check, CheckOutcome,
};
use crate::report::patch::{
    apply_block_patch, list_pending_blocks, normalise_markdown, record_skill_run,
    stage_pending_blocks, PatchSelection, SkillRunKind, SkillRunRecord, StagedBlock,
};
use crate::report::render::{
    build_manuscript, render_docx, render_markdown, DocxRenderError, MarkdownRenderOptions,
};
use crate::report::schema::{ensure_schema, new_id, now_iso, open, RunStatus};
use crate::report::sources::full_text::{fetch_full_text, url_or_license_permits, FullTextFetch};
use crate::report::sources::{NormalisedSource, ResolverStack, SourceKind};
use crate::report::state::{
    abort as state_abort, create_run, finalise as state_finalise, list_runs, load_run,
    CreateRunParams,
};
use crate::report::workspace::Workspace;

/// Entry point routed from `src/main.rs`. Mirrors the dispatch shape of
/// `mission::queue::handle_queue_command`.
pub fn handle_command(root: &Path, args: &[String]) -> Result<()> {
    match args.first().map(String::as_str) {
        None | Some("help") | Some("-h") | Some("--help") => {
            cmd_help();
            Ok(())
        }
        Some("new") => cmd_new(root, &args[1..]),
        Some("list") => cmd_list(root, &args[1..]),
        Some("status") => cmd_status(root, &args[1..]),
        Some("research-log-add") => cmd_research_log_add(root, &args[1..]),
        Some("add-evidence") => cmd_add_evidence(root, &args[1..]),
        Some("evidence-show") => cmd_evidence_show(root, &args[1..]),
        Some("figure-add") => cmd_figure_add(root, &args[1..]),
        Some("figure-list") => cmd_figure_list(root, &args[1..]),
        Some("table-add") => cmd_table_add(root, &args[1..]),
        Some("table-list") => cmd_table_list(root, &args[1..]),
        Some("project-description-sync") => cmd_project_description_sync(root, &args[1..]),
        Some("review-import") | Some("review") => cmd_review_import(root, &args[1..]),
        Some("source-review-sync") => cmd_source_review_sync(root, &args[1..]),
        Some("storyline-set") => cmd_storyline_set(root, &args[1..]),
        Some("storyline-show") => cmd_storyline_show(root, &args[1..]),
        Some("block-stage") => cmd_block_stage(root, &args[1..]),
        Some("block-apply") => cmd_block_apply(root, &args[1..]),
        Some("block-list") => cmd_block_list(root, &args[1..]),
        Some("check") => cmd_check(root, &args[1..]),
        Some("ask-user") => cmd_ask_user(root, &args[1..]),
        Some("answer") => cmd_answer(root, &args[1..]),
        Some("render") => cmd_render(root, &args[1..]),
        Some("finalise") | Some("finalize") => cmd_finalise(root, &args[1..]),
        Some("abort") => cmd_abort(root, &args[1..]),
        Some("blueprints") => cmd_blueprints(root, &args[1..]),
        Some(other) => {
            cmd_help();
            Err(anyhow!("unknown ctox report subcommand: {other}"))
        }
    }
}

// ---------- argument-parsing helpers ----------

fn find_flag<'a>(args: &'a [String], flag: &str) -> Option<&'a str> {
    let idx = args.iter().position(|a| a == flag)?;
    args.get(idx + 1).map(String::as_str)
}

fn collect_flag(args: &[String], flag: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut idx = 0;
    while idx < args.len() {
        if args[idx] == flag {
            if let Some(value) = args.get(idx + 1) {
                let trimmed = value.trim();
                if !trimmed.is_empty() {
                    out.push(trimmed.to_string());
                }
                idx += 2;
                continue;
            }
        }
        idx += 1;
    }
    out
}

fn reject_placeholder_text(label: &str, value: &str) -> Result<()> {
    let lower = value.to_lowercase();
    let placeholders = [
        "fake",
        "dummy",
        "placeholder",
        "platzhalter",
        "lorem",
        "tbd",
        "todo",
        "test schematic",
        "test figure",
        "testgrafik",
        "fakefig",
    ];
    if let Some(hit) = placeholders.iter().find(|needle| lower.contains(**needle)) {
        bail!("{label} contains placeholder/test text ({hit:?}); use client-ready wording");
    }
    Ok(())
}

fn has_flag(args: &[String], flag: &str) -> bool {
    args.iter().any(|a| a == flag)
}

fn first_positional<'a>(args: &'a [String]) -> Option<&'a str> {
    args.iter()
        .find(|a| !a.starts_with("--"))
        .map(String::as_str)
}

fn require_run_id<'a>(args: &'a [String], usage: &str) -> Result<&'a str> {
    if let Some(id) = find_flag(args, "--run-id") {
        return Ok(id);
    }
    if let Some(id) = first_positional(args) {
        return Ok(id);
    }
    bail!("{}", usage);
}

fn parse_csv_list(raw: Option<&str>) -> Vec<String> {
    raw.map(|value| {
        value
            .split(',')
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(str::to_string)
            .collect()
    })
    .unwrap_or_default()
}

fn read_markdown_file(path_str: &str) -> Result<String> {
    let path = PathBuf::from(path_str);
    let body = std::fs::read_to_string(&path)
        .with_context(|| format!("read markdown file {}", path.display()))?;
    Ok(body)
}

fn research_payload_count_hint(value: &Value) -> i64 {
    fn walk(value: &Value, key: Option<&str>, best: &mut i64) {
        let relevant_array = matches!(
            key,
            Some(
                "sources"
                    | "results"
                    | "items"
                    | "hits"
                    | "records"
                    | "papers"
                    | "candidates"
                    | "documents"
            )
        );
        let relevant_number = matches!(
            key,
            Some(
                "sources_count"
                    | "sources_found"
                    | "total_results"
                    | "reviewed_results"
                    | "candidate_hits"
                    | "screened_candidates"
                    | "results_count"
                    | "hit_count"
            )
        );
        match value {
            Value::Array(items) => {
                if relevant_array {
                    *best = (*best).max(items.len() as i64);
                }
                for item in items {
                    walk(item, None, best);
                }
            }
            Value::Object(map) => {
                for (child_key, child) in map {
                    walk(child, Some(child_key.as_str()), best);
                }
            }
            Value::Number(number) if relevant_number => {
                if let Some(count) = number.as_i64() {
                    *best = (*best).max(count);
                } else if let Some(count) = number.as_u64() {
                    *best = (*best).max(count.min(i64::MAX as u64) as i64);
                }
            }
            _ => {}
        }
    }

    let mut best = 0;
    walk(value, None, &mut best);
    best
}

/// arXiv resolver-API fallback. When `resolve_arxiv` times out (a
/// recurring problem from third-party networks), insert a minimal
/// evidence row pointing at the canonical arXiv URL, then attempt the
/// direct PDF fetch. The row carries `resolver_used = "arxiv-direct"`
/// so the operator can later see the lookup was metadata-light.
fn arxiv_direct_pdf_fallback(
    root: &Path,
    run_id: &str,
    arxiv_id: &str,
    pdf_url: &str,
) -> Result<()> {
    let conn = open(root)?;
    ensure_schema(&conn)?;
    // Stable evidence_id derived from "arxiv:<id>" so a later
    // successful resolve still resolves to the same row.
    let evidence_id = {
        use sha2::{Digest, Sha256};
        let mut h = Sha256::new();
        h.update(b"arxiv:");
        h.update(arxiv_id.trim().to_ascii_lowercase().as_bytes());
        let digest = h.finalize();
        let mut hex = String::with_capacity(2 + 16);
        hex.push_str("ev_");
        for byte in digest.iter().take(8) {
            use std::fmt::Write as _;
            let _ = write!(&mut hex, "{:02x}", byte);
        }
        hex
    };
    let abs_url = format!("https://arxiv.org/abs/{arxiv_id}");
    let now = now_iso();
    conn.execute(
        "INSERT OR REPLACE INTO report_evidence_register (
             evidence_id, run_id, kind, canonical_id, title, authors_json,
             venue, year, publisher, url_canonical, url_full_text,
             license, abstract_md, snippet_md, retrieved_at,
             resolver_used, integrity_hash, raw_payload_json,
             created_at, updated_at, citations_count
         ) VALUES (?1, ?2, 'arxiv', ?3, NULL, '[]', 'arXiv', NULL, NULL,
                   ?4, ?5, NULL, NULL, NULL, ?6, 'arxiv-direct', NULL,
                   ?7, ?8, ?8, 0)",
        params![
            evidence_id,
            run_id,
            arxiv_id,
            abs_url,
            pdf_url,
            now,
            "{\"fallback\":\"arxiv-direct\"}",
            now,
        ],
    )
    .context("insert arxiv-direct fallback row")?;

    println!("evidence_id: {evidence_id}");
    println!("kind:        arxiv");
    println!("canonical:   {arxiv_id}");
    println!("url:         {abs_url}");
    println!("resolver:    arxiv-direct (resolver-API failed; metadata-light row)");

    match fetch_full_text(pdf_url) {
        Ok(f) => {
            let chars = f.markdown.chars().count() as i64;
            conn.execute(
                "UPDATE report_evidence_register \
                 SET full_text_md = ?1, full_text_source = ?2, \
                     full_text_chars = ?3, updated_at = ?4 \
                 WHERE run_id = ?5 AND evidence_id = ?6",
                params![
                    f.markdown,
                    f.source_label,
                    chars,
                    now_iso(),
                    run_id,
                    evidence_id,
                ],
            )
            .context("persist arxiv-direct full text")?;
            emit_full_text_note(Some(&f));
        }
        Err(err) => {
            eprintln!("warning: arXiv direct-PDF fetch from {pdf_url} failed: {err}");
            emit_full_text_note(None);
            bail!(
                "arXiv {arxiv_id}: resolver API and direct PDF both failed — \
                 evidence row was created but carries no abstract or full text"
            );
        }
    }
    Ok(())
}

/// Try to download the open-access full text of a resolver-fetched
/// source and persist it to `report_evidence_register.full_text_md`.
/// Skipped silently when (a) the source has no `url_full_text`, (b) the
/// license is not recognised as open-access, or (c) the fetch/parse
/// errors out. Returns the [`FullTextFetch`] on success so the caller
/// can surface the result to the LLM via the resolver summary.
fn try_attach_full_text(
    root: &Path,
    run_id: &str,
    evidence_id: &str,
    source: &NormalisedSource,
) -> Option<FullTextFetch> {
    let Some(url) = source.url_full_text.as_deref() else {
        return None;
    };
    if url.trim().is_empty() {
        return None;
    }
    if !url_or_license_permits(source.license.as_deref(), url) {
        return None;
    }
    let fetch = match fetch_full_text(url) {
        Ok(f) => f,
        Err(err) => {
            eprintln!("warning: full-text fetch from {url} failed: {err}");
            return None;
        }
    };
    let chars = fetch.markdown.chars().count() as i64;
    let conn = match open(root) {
        Ok(c) => c,
        Err(err) => {
            eprintln!("warning: could not open store to persist full text: {err}");
            return None;
        }
    };
    if let Err(err) = ensure_schema(&conn) {
        eprintln!("warning: ensure_schema before full-text persist failed: {err}");
        return None;
    }
    let now = now_iso();
    let res = conn.execute(
        "UPDATE report_evidence_register \
         SET full_text_md = ?1, full_text_source = ?2, full_text_chars = ?3, \
             updated_at = ?4 \
         WHERE run_id = ?5 AND evidence_id = ?6",
        params![
            fetch.markdown,
            fetch.source_label,
            chars,
            now,
            run_id,
            evidence_id,
        ],
    );
    match res {
        Ok(_) => Some(fetch),
        Err(err) => {
            eprintln!("warning: persisting full-text into evidence register failed: {err}");
            None
        }
    }
}

/// Print a one-line summary of a successful full-text attachment so the
/// harness LLM sees that the source is now available at full-text depth,
/// not just abstract depth.
fn emit_full_text_note(fetch: Option<&FullTextFetch>) {
    if let Some(f) = fetch {
        println!(
            "full_text:   attached via {} ({} chars). Use `ctox report evidence-show --full-text` to read.",
            f.source_label,
            f.markdown.chars().count()
        );
    } else {
        println!(
            "full_text:   not attached (source carries no open-access full-text URL, or fetch was skipped)"
        );
    }
}

/// Print a multi-line summary of a resolver-fetched source. Goes to stdout
/// so the harness LLM sees the title + authors + abstract snippet
/// immediately when it calls `add-evidence --doi`/`--arxiv-id`. Without
/// this, the resolver path silently drops the abstract content into the
/// DB and the LLM has no way to incorporate it into block prose.
fn emit_resolver_summary(evidence_id: &str, source: &NormalisedSource) {
    println!("evidence_id: {evidence_id}");
    println!("kind:        {}", source.kind.as_str());
    println!("canonical:   {}", source.canonical_id);
    if let Some(title) = source.title.as_deref() {
        println!("title:       {title}");
    }
    if !source.authors.is_empty() {
        println!("authors:     {}", source.authors.join("; "));
    }
    if let Some(year) = source.year {
        println!("year:        {year}");
    }
    if let Some(venue) = source.venue.as_deref() {
        println!("venue:       {venue}");
    }
    if let Some(publisher) = source.publisher.as_deref() {
        println!("publisher:   {publisher}");
    }
    if let Some(url) = source.url_canonical.as_deref() {
        println!("url:         {url}");
    }
    println!("resolver:    {}", source.resolver_used.as_str());
    if let Some(abs_md) = source.abstract_md.as_deref() {
        let trimmed = abs_md.trim();
        let chars = trimmed.chars().count();
        println!("abstract ({chars} chars):");
        // Stream the whole abstract — the LLM needs every sentence to
        // ground prose in the source. No truncation.
        for line in trimmed.lines() {
            println!("  {}", line);
        }
    } else {
        println!(
            "abstract:    (none — Crossref/OpenAlex returned no abstract \
             for this source; use `ctox web read --url <url>` and \
             `add-evidence --abstract-file` to attach a manual snippet)"
        );
    }
    if let Some(snip) = source.snippet_md.as_deref() {
        let chars = snip.chars().count();
        if chars > 0 {
            println!("snippet ({chars} chars):");
            for line in snip.lines() {
                println!("  {}", line);
            }
        }
    }
}

// ---------- subcommand impls ----------

fn cmd_research_log_add(root: &Path, args: &[String]) -> Result<()> {
    let run_id = require_run_id(
        args,
        "usage: ctox report research-log-add --run-id RUN --question Q --sources-count N [--focus F] [--resolver R] [--summary S | --summary-file F] [--raw-payload-file F]",
    )?;
    let _ = load_run(root, run_id)?;
    let question = find_flag(args, "--question")
        .ok_or_else(|| anyhow!("--question is required"))?
        .trim()
        .to_string();
    if question.is_empty() {
        bail!("--question must not be empty");
    }
    let sources_count: i64 = find_flag(args, "--sources-count")
        .or_else(|| find_flag(args, "--source-count"))
        .ok_or_else(|| anyhow!("--sources-count N is required"))?
        .parse()
        .context("--sources-count must be an integer")?;
    if sources_count < 0 {
        bail!("--sources-count must be >= 0");
    }
    let focus = find_flag(args, "--focus").map(str::to_string);
    let resolver = find_flag(args, "--resolver").map(str::to_string);
    let summary = if let Some(path) = find_flag(args, "--summary-file") {
        Some(read_markdown_file(path)?)
    } else {
        find_flag(args, "--summary").map(str::to_string)
    };
    let raw_payload = if let Some(path) = find_flag(args, "--raw-payload-file") {
        let raw = read_markdown_file(path)?;
        let value: Value = serde_json::from_str(&raw)
            .with_context(|| format!("parse raw payload JSON from {path}"))?;
        Some(value)
    } else {
        None
    };
    if sources_count > 0 && raw_payload.is_none() {
        bail!("--raw-payload-file is required when --sources-count is greater than zero");
    }
    if let Some(payload) = &raw_payload {
        let hint = research_payload_count_hint(payload);
        if hint == 0 && sources_count > 0 {
            bail!("raw payload has no recognisable sources/results count; cannot back --sources-count {sources_count}");
        }
        if sources_count
            > hint
                .saturating_mul(110)
                .saturating_div(100)
                .saturating_add(2)
        {
            bail!("--sources-count {sources_count} is not backed by raw payload count hint {hint}");
        }
    }
    let raw_payload_json = raw_payload
        .as_ref()
        .map(serde_json::to_string)
        .transpose()?;

    let conn = open(root)?;
    ensure_schema(&conn)?;
    let research_id = new_id("research");
    let now = now_iso();
    conn.execute(
        "INSERT INTO report_research_log (
             research_id, run_id, question, focus, asked_at, resolver, summary,
             sources_count, raw_payload_json
         ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
        params![
            research_id,
            run_id,
            question,
            focus,
            now,
            resolver,
            summary,
            sources_count,
            raw_payload_json
        ],
    )?;
    println!("research_id:   {research_id}");
    println!("sources_count: {sources_count}");
    println!(
        "Use with: ctox report block-apply --run-id {run_id} --used-research-ids {research_id}"
    );
    Ok(())
}

fn cmd_new(root: &Path, args: &[String]) -> Result<()> {
    let report_type = first_positional(args)
        .ok_or_else(|| {
            anyhow!(
                "usage: ctox report new <report_type> --domain <id> --depth <id> --topic \"...\""
            )
        })?
        .to_string();
    let domain = find_flag(args, "--domain")
        .ok_or_else(|| anyhow!("--domain <id> is required"))?
        .to_string();
    let depth = find_flag(args, "--depth")
        .ok_or_else(|| anyhow!("--depth <id> is required"))?
        .to_string();
    let language = find_flag(args, "--language").unwrap_or("en").to_string();
    let topic = find_flag(args, "--topic")
        .ok_or_else(|| anyhow!("--topic \"...\" is required"))?
        .to_string();
    let reference_docs = collect_flag(args, "--reference-doc");
    let seed_dois = collect_flag(args, "--seed-doi");
    let review_docs = collect_flag(args, "--review-doc");

    let pack = AssetPack::load()?;
    pack.report_type(&report_type)
        .with_context(|| format!("unknown report_type: {report_type}"))?;
    pack.domain_profile(&domain)
        .with_context(|| format!("unknown domain_profile: {domain}"))?;
    pack.depth_profile(&depth)
        .with_context(|| format!("unknown depth_profile: {depth}"))?;

    // Resolve style profile from reference_profiles[]: prefer an entry
    // whose id matches the domain, else `auto`, else the first profile.
    let style_profile_id = pack
        .reference_profile(&domain)
        .or_else(|| pack.reference_profile("auto"))
        .or_else(|| pack.reference_profiles.first())
        .map(|p| p.style_profile_id.clone())
        .filter(|s| !s.is_empty())
        .or_else(|| pack.style_profiles.first().map(|p| p.id.clone()))
        .ok_or_else(|| anyhow!("asset pack contains no usable style profile"))?;

    let mut package_summary = json!({
        "report_type_id": report_type,
        "domain_profile_id": domain,
        "depth_profile_id": depth,
        "style_profile_id": style_profile_id,
        "language": language,
    });
    if !reference_docs.is_empty() {
        package_summary["reference_docs"] = json!(reference_docs);
    }
    if !seed_dois.is_empty() {
        package_summary["seed_dois"] = json!(seed_dois);
    }

    let params_in = CreateRunParams {
        report_type_id: report_type,
        domain_profile_id: domain,
        depth_profile_id: depth,
        style_profile_id,
        language,
        raw_topic: topic,
        package_summary: Some(package_summary),
    };
    let run_id = create_run(root, params_in)?;

    // Pre-resolve seed DOIs into the evidence register. Best effort: a
    // single failure is logged to stderr but does not abort run creation.
    if !seed_dois.is_empty() {
        match ResolverStack::new(root, &run_id, None) {
            Ok(stack) => {
                for doi in &seed_dois {
                    match stack.resolve_doi(doi) {
                        Ok(Some(source)) => {
                            if let Err(err) = stack.record_into_register(&source) {
                                eprintln!("warning: could not record DOI {doi}: {err}");
                            }
                        }
                        Ok(None) => eprintln!("warning: DOI {doi} did not resolve"),
                        Err(err) => eprintln!("warning: DOI {doi} resolver error: {err}"),
                    }
                }
            }
            Err(err) => eprintln!("warning: resolver stack unavailable: {err}"),
        }
    }

    // Import review docs as report_review_feedback rows. Best-effort.
    if !review_docs.is_empty() {
        import_review_docs(root, &run_id, &review_docs)?;
    }

    println!("Run created: {run_id}");
    println!("Next: populate evidence, storyline, figures/tables, blocks, then run `ctox report check --run-id {run_id} <kind>` for each gate.");
    Ok(())
}

fn cmd_review_import(root: &Path, args: &[String]) -> Result<()> {
    let run_id = require_run_id(
        args,
        "usage: ctox report review-import --run-id RUN --review-doc PATH",
    )?;
    let _ = load_run(root, run_id)?;
    let review_docs = collect_flag(args, "--review-doc");
    if review_docs.is_empty() {
        bail!("--review-doc PATH is required");
    }
    let imported = import_review_docs(root, run_id, &review_docs)?;
    println!("Imported {imported} review feedback note(s).");
    Ok(())
}

fn import_review_docs(root: &Path, run_id: &str, review_docs: &[String]) -> Result<usize> {
    let conn = open(root)?;
    ensure_schema(&conn)?;
    let mut imported = 0usize;
    for path in review_docs {
        match extract_docx_comments(Path::new(path)) {
            Ok(comments) if comments.is_empty() => {
                eprintln!("warning: review doc {path} contained no comments");
            }
            Ok(comments) => {
                let now = now_iso();
                for (instance_hint, body) in comments {
                    let feedback_id = new_id("feedback");
                    let res = conn.execute(
                        "INSERT INTO report_review_feedback (
                             feedback_id, run_id, source_file, instance_id,
                             form_only, body, imported_at
                         ) VALUES (?1, ?2, ?3, ?4, 0, ?5, ?6)",
                        params![feedback_id, run_id, path, instance_hint, body, now,],
                    );
                    if let Err(err) = res {
                        eprintln!("warning: failed to import review note from {path}: {err}");
                    } else {
                        imported += 1;
                    }
                }
            }
            Err(err) => eprintln!("warning: review doc {path} parse error: {err}"),
        }
    }
    Ok(imported)
}

fn cmd_list(root: &Path, args: &[String]) -> Result<()> {
    let status_filter = find_flag(args, "--status").map(str::to_string);
    let limit: usize = find_flag(args, "--limit")
        .and_then(|s| s.parse().ok())
        .unwrap_or(20);
    let runs = list_runs(root, limit.max(1))?;
    let filtered: Vec<_> = runs
        .into_iter()
        .filter(|r| {
            status_filter
                .as_deref()
                .map(|s| r.status == s)
                .unwrap_or(true)
        })
        .collect();
    if filtered.is_empty() {
        println!("(no report runs)");
        return Ok(());
    }
    println!("RUN_ID\tTYPE\tSTATUS\tTOPIC");
    for r in &filtered {
        let topic = truncate_for_table(&r.raw_topic, 60);
        println!(
            "{}\t{}\t{}\t{}",
            r.run_id, r.report_type_id, r.status, topic
        );
    }
    Ok(())
}

fn cmd_status(root: &Path, args: &[String]) -> Result<()> {
    let run_id = first_positional(args)
        .ok_or_else(|| anyhow!("usage: ctox report status RUN_ID [--json]"))?;
    let workspace = Workspace::load(root, run_id)?;
    let snapshot = workspace.workspace_snapshot()?;
    let conn = open(root)?;
    ensure_schema(&conn)?;
    let last_checks = collect_last_check_outcomes(&conn, run_id)?;
    let mut payload = snapshot.clone();
    payload["last_check_outcomes"] = last_checks.clone();

    if has_flag(args, "--json") {
        println!("{}", serde_json::to_string_pretty(&payload)?);
        return Ok(());
    }

    let metadata = workspace.run_metadata()?;
    println!("Run:        {}", metadata.run_id);
    println!("Type:       {}", metadata.report_type_id);
    println!("Status:     {}", metadata.status);
    println!(
        "Topic:      {}",
        truncate_for_table(&metadata.raw_topic, 80)
    );
    if let Some(comp) = snapshot.get("completeness") {
        let total = comp
            .get("total_required")
            .and_then(Value::as_u64)
            .unwrap_or(0);
        let done = comp
            .get("done_required")
            .and_then(Value::as_u64)
            .unwrap_or(0);
        println!("Complete:   {done}/{total} required blocks");
    }
    if let Some(budget) = snapshot.get("character_budget") {
        let target = budget
            .get("target_chars")
            .and_then(Value::as_u64)
            .unwrap_or(0);
        let actual = budget
            .get("actual_chars")
            .and_then(Value::as_u64)
            .unwrap_or(0);
        let delta = budget
            .get("delta_chars")
            .and_then(Value::as_i64)
            .unwrap_or(0);
        println!("Budget:     {actual}/{target} chars (delta {delta:+})");
    }
    let evidence_count = snapshot
        .get("evidence_register_size")
        .and_then(Value::as_u64)
        .unwrap_or(0);
    println!("Evidence:   {evidence_count} registered");
    let open_count = snapshot
        .get("open_questions")
        .and_then(Value::as_array)
        .map(|a| a.len())
        .unwrap_or(0);
    println!("Open Qs:    {open_count}");
    if let Some(rg) = last_checks.get("release_guard") {
        println!("release_guard:  {}", short_check_summary(rg));
    }
    if let Some(nf) = last_checks.get("narrative_flow") {
        println!("narrative_flow: {}", short_check_summary(nf));
    }
    if let Some(dq) = last_checks.get("deliverable_quality") {
        println!("deliverable_quality: {}", short_check_summary(dq));
    }
    Ok(())
}

// ============================================================
// New deterministic CLI subcommands invoked by the harness LLM.
// ============================================================

/// `ctox report add-evidence --run-id RUN [--doi DOI | --url URL | --arxiv-id ID]
///                            [--title T] [--authors "A1; A2"] [--year Y]
///                            [--venue V] [--abstract-file PATH]
///                            [--snippet-file PATH] [--license L]`
///
/// Adds one row to `report_evidence_register`. If `--doi` (or `--arxiv-id`)
/// is supplied, the resolver stack is consulted first (Crossref / OpenAlex /
/// arXiv) to enrich the metadata. Otherwise the explicit flags are used
/// verbatim.
fn cmd_add_evidence(root: &Path, args: &[String]) -> Result<()> {
    let run_id = require_run_id(args, "usage: ctox report add-evidence --run-id RUN ...")?;
    let _ = load_run(root, run_id)?;
    let conn = open(root)?;
    ensure_schema(&conn)?;

    let doi = find_flag(args, "--doi");
    let arxiv_id = find_flag(args, "--arxiv-id");
    let url = find_flag(args, "--url");

    let no_full_text = has_flag(args, "--no-full-text");

    if let Some(doi) = doi {
        let stack =
            ResolverStack::new(root, run_id, None).context("failed to construct resolver stack")?;
        match stack.resolve_doi(doi) {
            Ok(Some(source)) => {
                let recorded = stack
                    .record_into_register(&source)
                    .context("failed to record resolved DOI into evidence register")?;
                let ft = (!no_full_text)
                    .then(|| try_attach_full_text(root, run_id, &recorded, &source))
                    .flatten();
                emit_resolver_summary(&recorded, &source);
                emit_full_text_note(ft.as_ref());
                return Ok(());
            }
            Ok(None) => bail!("DOI {doi} did not resolve via Crossref/OpenAlex"),
            Err(err) => bail!("DOI {doi} resolver error: {err}"),
        }
    }

    if let Some(arxiv) = arxiv_id {
        let stack =
            ResolverStack::new(root, run_id, None).context("failed to construct resolver stack")?;
        match stack.resolve_arxiv(arxiv) {
            Ok(Some(source)) => {
                let recorded = stack
                    .record_into_register(&source)
                    .context("failed to record resolved arXiv id into evidence register")?;
                let ft = (!no_full_text)
                    .then(|| try_attach_full_text(root, run_id, &recorded, &source))
                    .flatten();
                emit_resolver_summary(&recorded, &source);
                emit_full_text_note(ft.as_ref());
                return Ok(());
            }
            Ok(None) => bail!("arXiv id {arxiv} did not resolve"),
            // arXiv API timeouts are unfortunately common from third-
            // party hosts. Fall back to a direct PDF fetch so we still
            // get the paper body even when metadata lookup failed.
            Err(err) if !no_full_text => {
                eprintln!(
                    "warning: arXiv resolver failed for {arxiv}: {err}; \
                     falling back to direct PDF fetch"
                );
                let pdf_url = format!("https://arxiv.org/pdf/{arxiv}.pdf");
                arxiv_direct_pdf_fallback(root, run_id, arxiv, &pdf_url)?;
                return Ok(());
            }
            Err(err) => bail!("arXiv {arxiv} resolver error: {err}"),
        }
    }

    // Manual evidence card.
    //
    // Hard rule: stub evidences are forbidden. Every manual row must carry
    // *real* source content — either an --abstract-file with >=200 chars
    // pulled from `ctox web read URL` (or any other tool that produces
    // markdown), or a --snippet-file of similar length. Title-only rows
    // are rejected: that path was the loophole that produced the previous
    // fake feasibility study.
    let title = find_flag(args, "--title");
    let authors_raw = find_flag(args, "--authors").unwrap_or("");
    let year_raw = find_flag(args, "--year");
    let venue = find_flag(args, "--venue");
    let abstract_md = find_flag(args, "--abstract-file")
        .map(read_markdown_file)
        .transpose()?;
    let snippet_md = find_flag(args, "--snippet-file")
        .map(read_markdown_file)
        .transpose()?;
    let license = find_flag(args, "--license");

    if title.is_none() && url.is_none() {
        bail!("manual evidence requires --title and --url at minimum");
    }
    if title.is_none() {
        bail!("manual evidence requires --title");
    }
    let abstract_chars = abstract_md
        .as_deref()
        .map(|s| s.chars().count())
        .unwrap_or(0);
    let snippet_chars = snippet_md
        .as_deref()
        .map(|s| s.chars().count())
        .unwrap_or(0);
    if abstract_chars < 200 && snippet_chars < 200 {
        bail!(
            "manual evidence requires --abstract-file or --snippet-file with at least 200 chars \
             of real source content. Stub evidences are rejected. Use \
             `ctox web read --url <url>` to fetch the page first, then pass \
             the result via --abstract-file. For papers with a DOI, prefer \
             `ctox report add-evidence --doi <DOI>` instead."
        );
    }

    let kind = if url.is_some() { "url" } else { "manual" };
    let canonical_id = url.or(title).unwrap_or("");
    let authors_vec: Vec<String> = authors_raw
        .split(';')
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect();
    let authors_json = serde_json::to_string(&authors_vec).context("encode --authors as JSON")?;
    let year: Option<i64> = year_raw.and_then(|s| s.parse().ok());
    let evidence_id = new_id("ev");
    let now = now_iso();
    // raw_payload_json captures the manual provenance so the resolver
    // path and the manual path share the same row shape.
    let raw_payload_json = serde_json::to_string(&json!({
        "manual": true,
        "source_url": url,
        "supplied_via_cli": true,
    }))
    .context("encode raw_payload_json")?;

    conn.execute(
        "INSERT OR REPLACE INTO report_evidence_register (
             evidence_id, run_id, kind, canonical_id, title, authors_json,
             venue, year, publisher, url_canonical, url_full_text,
             license, abstract_md, snippet_md, retrieved_at,
             resolver_used, integrity_hash, raw_payload_json,
             created_at, updated_at, citations_count
         ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, NULL, ?9, NULL,
                   ?10, ?11, ?12, ?13, 'manual', NULL, ?14, ?15, ?15, 0)",
        params![
            evidence_id,
            run_id,
            kind,
            canonical_id,
            title,
            authors_json,
            venue,
            year,
            url,
            license,
            abstract_md,
            snippet_md,
            now,
            raw_payload_json,
            now,
        ],
    )
    .context("failed to insert manual evidence row")?;
    // Manual --url path: when the URL points at a directly-fetchable
    // open-access HTML/PDF and the caller did not pass --no-full-text,
    // also pull the full body so the LLM has paper-level depth without
    // a separate `web read` round-trip.
    let mut full_text_attached: Option<FullTextFetch> = None;
    if !no_full_text {
        if let Some(url) = url {
            if url_or_license_permits(license, url) {
                match fetch_full_text(url) {
                    Ok(f) => {
                        let chars = f.markdown.chars().count() as i64;
                        if let Err(err) = conn.execute(
                            "UPDATE report_evidence_register \
                             SET full_text_md = ?1, full_text_source = ?2, \
                                 full_text_chars = ?3, updated_at = ?4 \
                             WHERE run_id = ?5 AND evidence_id = ?6",
                            params![
                                f.markdown,
                                f.source_label,
                                chars,
                                now_iso(),
                                run_id,
                                evidence_id,
                            ],
                        ) {
                            eprintln!("warning: persisting full-text from --url failed: {err}");
                        } else {
                            full_text_attached = Some(f);
                        }
                    }
                    Err(err) => {
                        eprintln!("warning: full-text fetch from {url} failed: {err}");
                    }
                }
            }
        }
    }

    println!("evidence_id: {evidence_id}");
    println!("kind:        {kind}");
    if let Some(t) = title {
        println!("title:       {t}");
    }
    if !authors_vec.is_empty() {
        println!("authors:     {}", authors_vec.join("; "));
    }
    if let Some(y) = year {
        println!("year:        {y}");
    }
    if let Some(v) = venue {
        println!("venue:       {v}");
    }
    if let Some(u) = url {
        println!("url:         {u}");
    }
    println!("resolver:    manual");
    if let Some(abs_md) = abstract_md.as_deref() {
        let trimmed = abs_md.trim();
        println!("abstract ({} chars):", trimmed.chars().count());
        for line in trimmed.lines() {
            println!("  {}", line);
        }
    }
    if let Some(snip) = snippet_md.as_deref() {
        let trimmed = snip.trim();
        let chars = trimmed.chars().count();
        if chars > 0 {
            println!("snippet ({chars} chars):");
            for line in trimmed.lines() {
                println!("  {}", line);
            }
        }
    }
    emit_full_text_note(full_text_attached.as_ref());
    let _ = (abstract_chars, snippet_chars);
    Ok(())
}

/// `ctox report evidence-show --run-id RUN [--evidence-id ID | --all] [--json]`
///
/// Read-back path the harness LLM uses to load abstract content into its
/// working context before drafting a block. Without this, the resolver
/// path puts content in the DB but the LLM never sees it — which is the
/// failure mode that produced the previous halluzinated feasibility
/// study. The skill mandates calling `evidence-show` for every cited
/// `evidence_id` before `block-stage`.
fn cmd_evidence_show(root: &Path, args: &[String]) -> Result<()> {
    let run_id = require_run_id(
        args,
        "usage: ctox report evidence-show --run-id RUN [--evidence-id ID | --all] [--json]",
    )?;
    let _ = load_run(root, run_id)?;
    let conn = open(root)?;
    ensure_schema(&conn)?;
    let json_out = has_flag(args, "--json");
    let want_all = has_flag(args, "--all");
    let one_id = find_flag(args, "--evidence-id");
    if !want_all && one_id.is_none() {
        bail!("specify --evidence-id ID or --all");
    }

    let want_full_text = has_flag(args, "--full-text");
    let mut sql = String::from(
        "SELECT evidence_id, kind, canonical_id, title, authors_json, \
                venue, year, publisher, url_canonical, url_full_text, \
                license, abstract_md, snippet_md, resolver_used, \
                full_text_md, full_text_source, full_text_chars \
         FROM report_evidence_register WHERE run_id = ?1",
    );
    let mut bind: Vec<String> = vec![run_id.to_string()];
    if let Some(id) = one_id {
        sql.push_str(" AND evidence_id = ?2");
        bind.push(id.to_string());
    }
    sql.push_str(" ORDER BY retrieved_at ASC");

    let mut stmt = conn.prepare(&sql)?;
    let params_dyn: Vec<&dyn rusqlite::ToSql> =
        bind.iter().map(|s| s as &dyn rusqlite::ToSql).collect();
    let rows = stmt
        .query_map(params_dyn.as_slice(), |row| {
            let evidence_id: String = row.get(0)?;
            let kind: String = row.get(1)?;
            let canonical_id: Option<String> = row.get(2)?;
            let title: Option<String> = row.get(3)?;
            let authors_json: Option<String> = row.get(4)?;
            let venue: Option<String> = row.get(5)?;
            let year: Option<i64> = row.get(6)?;
            let publisher: Option<String> = row.get(7)?;
            let url_canonical: Option<String> = row.get(8)?;
            let url_full_text: Option<String> = row.get(9)?;
            let license: Option<String> = row.get(10)?;
            let abstract_md: Option<String> = row.get(11)?;
            let snippet_md: Option<String> = row.get(12)?;
            let resolver_used: Option<String> = row.get(13)?;
            let full_text_md: Option<String> = row.get(14)?;
            let full_text_source: Option<String> = row.get(15)?;
            let full_text_chars: Option<i64> = row.get(16)?;
            Ok((
                evidence_id,
                kind,
                canonical_id,
                title,
                authors_json,
                venue,
                year,
                publisher,
                url_canonical,
                url_full_text,
                license,
                abstract_md,
                snippet_md,
                resolver_used,
                full_text_md,
                full_text_source,
                full_text_chars,
            ))
        })?
        .collect::<Result<Vec<_>, _>>()?;

    if rows.is_empty() {
        bail!("no evidence row matches the filter");
    }

    if json_out {
        let payload: Vec<Value> = rows
            .iter()
            .map(|r| {
                let authors: Vec<String> =
                    r.4.as_deref()
                        .and_then(|s| serde_json::from_str(s).ok())
                        .unwrap_or_default();
                json!({
                    "evidence_id": r.0,
                    "kind": r.1,
                    "canonical_id": r.2,
                    "title": r.3,
                    "authors": authors,
                    "venue": r.5,
                    "year": r.6,
                    "publisher": r.7,
                    "url_canonical": r.8,
                    "url_full_text": r.9,
                    "license": r.10,
                    "abstract_md": r.11,
                    "snippet_md": r.12,
                    "resolver_used": r.13,
                    "full_text_md": if want_full_text { r.14.clone() } else { None },
                    "full_text_source": r.15,
                    "full_text_chars": r.16,
                    "abstract_chars": r.11.as_deref().map(|s| s.chars().count()).unwrap_or(0),
                    "snippet_chars": r.12.as_deref().map(|s| s.chars().count()).unwrap_or(0),
                })
            })
            .collect();
        println!("{}", serde_json::to_string_pretty(&Value::Array(payload))?);
    } else {
        for (i, r) in rows.iter().enumerate() {
            if i > 0 {
                println!();
                println!("---");
            }
            println!("evidence_id: {}", r.0);
            println!("kind:        {}", r.1);
            if let Some(c) = r.2.as_deref() {
                println!("canonical:   {c}");
            }
            if let Some(t) = r.3.as_deref() {
                println!("title:       {t}");
            }
            let authors: Vec<String> =
                r.4.as_deref()
                    .and_then(|s| serde_json::from_str(s).ok())
                    .unwrap_or_default();
            if !authors.is_empty() {
                println!("authors:     {}", authors.join("; "));
            }
            if let Some(y) = r.6 {
                println!("year:        {y}");
            }
            if let Some(v) = r.5.as_deref() {
                println!("venue:       {v}");
            }
            if let Some(p) = r.7.as_deref() {
                println!("publisher:   {p}");
            }
            if let Some(u) = r.8.as_deref() {
                println!("url:         {u}");
            }
            if let Some(u) = r.9.as_deref() {
                println!("full_text:   {u}");
            }
            if let Some(l) = r.10.as_deref() {
                println!("license:     {l}");
            }
            if let Some(rv) = r.13.as_deref() {
                println!("resolver:    {rv}");
            }
            if let Some(abs_md) = r.11.as_deref() {
                let trimmed = abs_md.trim();
                println!("abstract ({} chars):", trimmed.chars().count());
                for line in trimmed.lines() {
                    println!("  {}", line);
                }
            } else {
                println!("abstract:    (none)");
            }
            if let Some(snip) = r.12.as_deref() {
                let trimmed = snip.trim();
                let c = trimmed.chars().count();
                if c > 0 {
                    println!("snippet ({c} chars):");
                    for line in trimmed.lines() {
                        println!("  {}", line);
                    }
                }
            }
            // Full-text presence is always reported; the body itself
            // is gated on --full-text because it can be hundreds of KB
            // and would flood the LLM's context if dumped per call.
            if let Some(label) = r.15.as_deref() {
                let chars = r.16.unwrap_or(0);
                println!("full_text:   attached via {label} ({chars} chars)");
                if want_full_text {
                    if let Some(body) = r.14.as_deref() {
                        let trimmed = body.trim();
                        println!("full_text body:");
                        for line in trimmed.lines() {
                            println!("  {}", line);
                        }
                    }
                } else {
                    println!(
                        "             (run with --full-text to include the body in this output)"
                    );
                }
            } else {
                println!("full_text:   (not attached)");
            }
        }
    }
    Ok(())
}

/// `ctox report block-stage --run-id RUN --instance-id ID --markdown-file F
///                          [--doc-id D] [--block-id B] [--title T] [--ord N]
///                          [--reason R] [--used-reference-ids "ev1,ev2"]`
// ============================================================
// Layer 1: Figures
// ============================================================

/// `ctox report figure-add --run-id RUN --kind <schematic|chart|photo|extracted>
///   --caption "..." --source "..." [--instance-id ID]
///   ( --code-mermaid FILE | --code-python FILE | --code-graphviz FILE
///   | --image-file FILE | --extract-from-evidence ev_X --page N )`
///
/// Generates or imports a figure for the run. Code-driven modes render
/// the source to a PNG via the relevant tool (mmdc / python /
/// graphviz). Extract mode pulls a page-image from a stored OA-PDF in
/// the evidence register. The PNG is persisted under the run's figure
/// dir; the row in `report_figures` carries the path, caption, source
/// label, and optional original source code.
fn cmd_figure_add(root: &Path, args: &[String]) -> Result<()> {
    let run_id = require_run_id(args, "usage: ctox report figure-add --run-id RUN ...")?;
    let _ = load_run(root, run_id)?;
    let kind = find_flag(args, "--kind").unwrap_or("schematic");
    if !matches!(kind, "schematic" | "chart" | "photo" | "extracted") {
        bail!("--kind must be one of: schematic, chart, photo, extracted (got {kind:?})");
    }
    let caption = find_flag(args, "--caption")
        .ok_or_else(|| anyhow!("--caption \"...\" is required"))?
        .to_string();
    let source_label = find_flag(args, "--source")
        .ok_or_else(|| anyhow!("--source \"...\" is required (e.g. 'eigene Darstellung' or DOI)"))?
        .to_string();
    let instance_id = find_flag(args, "--instance-id").map(str::to_string);
    reject_placeholder_text("--caption", &caption)?;
    reject_placeholder_text("--source", &source_label)?;
    if let Some(instance_id) = &instance_id {
        reject_placeholder_text("--instance-id", instance_id)?;
    }

    let figures_dir = root.join("runtime").join("report_figures").join(run_id);
    std::fs::create_dir_all(&figures_dir).context("create figures dir")?;
    let figure_id = new_id("fig");
    let png_path = figures_dir.join(format!("{figure_id}.png"));

    // Determine source mode and produce the PNG.
    let (code_kind, code_md) = if let Some(path) = find_flag(args, "--code-mermaid") {
        let code = read_markdown_file(path)?;
        render_mermaid_to_png(&code, &png_path)?;
        (Some("mermaid".to_string()), Some(code))
    } else if let Some(path) = find_flag(args, "--code-python") {
        let code = read_markdown_file(path)?;
        render_python_to_png(&code, &png_path)?;
        (Some("matplotlib".to_string()), Some(code))
    } else if let Some(path) = find_flag(args, "--code-graphviz") {
        let code = read_markdown_file(path)?;
        render_graphviz_to_png(&code, &png_path)?;
        (Some("graphviz".to_string()), Some(code))
    } else if let Some(path) = find_flag(args, "--image-file") {
        std::fs::copy(path, &png_path)
            .with_context(|| format!("copy image-file {path} to figures dir"))?;
        (None, None)
    } else if let Some(ev_id) = find_flag(args, "--extract-from-evidence") {
        let page: i64 = find_flag(args, "--page")
            .and_then(|s| s.parse().ok())
            .ok_or_else(|| anyhow!("--page N is required with --extract-from-evidence"))?;
        extract_pdf_page_to_png(root, run_id, ev_id, page, &png_path)?;
        (Some("pdf-extract".to_string()), None)
    } else {
        bail!(
            "specify one of: --code-mermaid, --code-python, --code-graphviz, --image-file, --extract-from-evidence"
        );
    };

    // Best-effort image dimension probe for the renderer.
    let (w_px, h_px) = match image::open(&png_path) {
        Ok(img) => (Some(img.width() as i64), Some(img.height() as i64)),
        Err(_) => (None, None),
    };

    let conn = open(root)?;
    ensure_schema(&conn)?;
    let now = now_iso();
    conn.execute(
        "INSERT INTO report_figures (
            figure_id, run_id, fig_number, kind, instance_id, image_path,
            caption, source_label, code_kind, code_md, width_px, height_px,
            created_at
         ) VALUES (?1, ?2, NULL, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)",
        params![
            figure_id,
            run_id,
            kind,
            instance_id,
            png_path.to_string_lossy().to_string(),
            caption,
            source_label,
            code_kind,
            code_md,
            w_px,
            h_px,
            now,
        ],
    )
    .context("insert figure row")?;

    println!("figure_id:   {figure_id}");
    println!("kind:        {kind}");
    println!("path:        {}", png_path.display());
    if let (Some(w), Some(h)) = (w_px, h_px) {
        println!("size:        {w}x{h} px");
    }
    println!("caption:     {caption}");
    println!("source:      {source_label}");
    println!(
        "Cite from a block via the token {{{{fig:{figure_id}}}}} — the renderer assigns a fig_number."
    );
    Ok(())
}

fn render_mermaid_to_png(code: &str, out: &Path) -> Result<()> {
    use std::io::Write;
    use std::process::Command;
    let tmp_dir = tempfile::tempdir().context("tempdir for mermaid")?;
    let src = tmp_dir.path().join("diagram.mmd");
    std::fs::File::create(&src)?
        .write_all(code.as_bytes())
        .context("write mermaid source")?;
    let status = Command::new("mmdc")
        .arg("-i")
        .arg(&src)
        .arg("-o")
        .arg(out)
        .arg("--quiet")
        .status();
    match status {
        Ok(s) if s.success() => Ok(()),
        Ok(s) => bail!("mmdc exited with {}", s.code().unwrap_or(-1)),
        Err(err) => bail!(
            "mmdc not available ({err}); install @mermaid-js/mermaid-cli or use --code-python instead"
        ),
    }
}

fn render_python_to_png(code: &str, out: &Path) -> Result<()> {
    use std::io::Write;
    use std::process::Command;
    let tmp_dir = tempfile::tempdir().context("tempdir for python")?;
    let src = tmp_dir.path().join("diagram.py");
    // Append a small footer that saves the current figure to `out` so
    // the LLM-supplied script can use plt.* without worrying about
    // savefig wiring.
    let mut full = String::from(
        "# CTOX figure-add wrapper: matplotlib must use Agg backend.\n\
         import matplotlib\n\
         matplotlib.use('Agg')\n\
         import matplotlib.pyplot as plt\n\
         _OUT = ",
    );
    full.push('"');
    full.push_str(&out.to_string_lossy());
    full.push('"');
    full.push_str("\n\n");
    full.push_str(code);
    full.push_str(
        "\n\n# Save the current figure (or all of them concatenated, last wins).\n\
         try:\n    plt.savefig(_OUT, dpi=150, bbox_inches='tight')\n\
         except Exception as _err:\n    raise SystemExit(f'matplotlib savefig failed: {_err}')\n",
    );
    std::fs::File::create(&src)?
        .write_all(full.as_bytes())
        .context("write python source")?;
    let status = Command::new("python3")
        .arg(&src)
        .status()
        .context("python3 not available")?;
    if !status.success() {
        bail!("python3 exited with {}", status.code().unwrap_or(-1));
    }
    if !out.exists() {
        bail!("python script ran but did not produce {}", out.display());
    }
    Ok(())
}

fn render_graphviz_to_png(code: &str, out: &Path) -> Result<()> {
    use std::io::Write;
    use std::process::Command;
    let tmp_dir = tempfile::tempdir().context("tempdir for graphviz")?;
    let src = tmp_dir.path().join("diagram.dot");
    std::fs::File::create(&src)?
        .write_all(code.as_bytes())
        .context("write graphviz source")?;
    let status = Command::new("dot")
        .arg("-Tpng")
        .arg(&src)
        .arg("-o")
        .arg(out)
        .status();
    match status {
        Ok(s) if s.success() => Ok(()),
        Ok(s) => bail!("dot exited with {}", s.code().unwrap_or(-1)),
        Err(err) => bail!("dot not available ({err}); install graphviz"),
    }
}

fn extract_pdf_page_to_png(
    root: &Path,
    run_id: &str,
    evidence_id: &str,
    page_index: i64,
    out: &Path,
) -> Result<()> {
    let conn = open(root)?;
    let row: Option<(String, String)> = conn
        .query_row(
            "SELECT canonical_id, COALESCE(url_full_text, '') FROM report_evidence_register \
             WHERE run_id = ?1 AND evidence_id = ?2",
            params![run_id, evidence_id],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .optional()?;
    let (_canonical, url) =
        row.ok_or_else(|| anyhow!("evidence_id {evidence_id} not found in run {run_id}"))?;
    if url.trim().is_empty() {
        bail!("evidence {evidence_id} has no url_full_text — cannot extract a page");
    }
    // Use ureq directly here rather than the OA pipeline so we don't
    // care about license here — the operator has already opted in by
    // running figure-add.
    let agent = ureq::AgentBuilder::new()
        .timeout_connect(std::time::Duration::from_secs(8))
        .timeout_read(std::time::Duration::from_secs(60))
        .build();
    let resp = agent
        .get(&url)
        .call()
        .with_context(|| format!("fetch PDF {url}"))?;
    let mut bytes: Vec<u8> = Vec::new();
    use std::io::Read as _;
    resp.into_reader()
        .read_to_end(&mut bytes)
        .context("read PDF body")?;
    // Use ctox-pdf-parse's page-render hook — for now the simplest
    // reliable extraction is a Python/poppler shell-out.
    let tmp_dir = tempfile::tempdir().context("tempdir for pdf extract")?;
    let pdf_path = tmp_dir.path().join("source.pdf");
    std::fs::write(&pdf_path, &bytes).context("write source pdf")?;
    use std::process::Command;
    let prefix = tmp_dir.path().join("page");
    let status = Command::new("pdftoppm")
        .arg("-png")
        .arg("-f")
        .arg(format!("{}", page_index))
        .arg("-l")
        .arg(format!("{}", page_index))
        .arg("-r")
        .arg("150")
        .arg(&pdf_path)
        .arg(&prefix)
        .status()
        .context("pdftoppm not available; install poppler-utils")?;
    if !status.success() {
        bail!("pdftoppm exited with {}", status.code().unwrap_or(-1));
    }
    // pdftoppm names the output e.g. `page-1.png` or `page-01.png`.
    for entry in std::fs::read_dir(tmp_dir.path())? {
        let entry = entry?;
        let name = entry.file_name();
        let name_s = name.to_string_lossy();
        if name_s.starts_with("page-") && name_s.ends_with(".png") {
            std::fs::copy(entry.path(), out).context("copy extracted page")?;
            return Ok(());
        }
    }
    bail!("pdftoppm produced no output");
}

/// `ctox report figure-list --run-id RUN [--json]`
fn cmd_figure_list(root: &Path, args: &[String]) -> Result<()> {
    let run_id = require_run_id(args, "usage: ctox report figure-list --run-id RUN [--json]")?;
    let _ = load_run(root, run_id)?;
    let json_out = has_flag(args, "--json");
    let conn = open(root)?;
    let mut stmt = conn.prepare(
        "SELECT figure_id, kind, instance_id, image_path, caption, source_label, \
                code_kind, width_px, height_px \
         FROM report_figures WHERE run_id = ?1 ORDER BY created_at ASC",
    )?;
    let rows: Vec<(
        String,
        String,
        Option<String>,
        String,
        String,
        String,
        Option<String>,
        Option<i64>,
        Option<i64>,
    )> = stmt
        .query_map(params![run_id], |row| {
            Ok((
                row.get(0)?,
                row.get(1)?,
                row.get(2)?,
                row.get(3)?,
                row.get(4)?,
                row.get(5)?,
                row.get(6)?,
                row.get(7)?,
                row.get(8)?,
            ))
        })?
        .collect::<Result<Vec<_>, _>>()?;
    if json_out {
        let payload: Vec<Value> = rows
            .iter()
            .map(|r| {
                json!({
                    "figure_id": r.0,
                    "kind": r.1,
                    "instance_id": r.2,
                    "image_path": r.3,
                    "caption": r.4,
                    "source_label": r.5,
                    "code_kind": r.6,
                    "width_px": r.7,
                    "height_px": r.8,
                })
            })
            .collect();
        println!("{}", serde_json::to_string_pretty(&Value::Array(payload))?);
    } else {
        println!("FIGURES ({})", rows.len());
        for r in &rows {
            println!(
                "  {}\tkind={}\tcite as {{{{fig:{}}}}}\t{}",
                r.0,
                r.1,
                r.0,
                truncate_for_table(&r.4, 60)
            );
        }
    }
    Ok(())
}

// ============================================================
// Layer 2: Real Word tables
// ============================================================

/// `ctox report table-add --run-id RUN --kind <kind> --caption "..."
///   --csv-file F [--instance-id ID] [--legend "..."]`
///
/// Persists a structured table into `report_tables`. CSV first row is
/// the header, subsequent rows are data. The DOCX renderer emits this
/// as a native Word table; the Markdown renderer emits a GFM pipe
/// table. Cite from a block via `{{tbl:<table_id>}}`.
fn cmd_table_add(root: &Path, args: &[String]) -> Result<()> {
    let run_id = require_run_id(args, "usage: ctox report table-add --run-id RUN ...")?;
    let _ = load_run(root, run_id)?;
    let kind = find_flag(args, "--kind").unwrap_or("generic");
    if !matches!(
        kind,
        "matrix" | "scenario" | "defect_catalog" | "risk_register" | "abbreviations" | "generic"
    ) {
        bail!(
            "--kind must be one of: matrix, scenario, defect_catalog, risk_register, abbreviations, generic (got {kind:?})"
        );
    }
    let caption = find_flag(args, "--caption")
        .ok_or_else(|| anyhow!("--caption \"...\" is required"))?
        .to_string();
    let csv_path =
        find_flag(args, "--csv-file").ok_or_else(|| anyhow!("--csv-file PATH is required"))?;
    let instance_id = find_flag(args, "--instance-id").map(str::to_string);
    let legend = find_flag(args, "--legend").map(str::to_string);

    let raw =
        std::fs::read_to_string(csv_path).with_context(|| format!("read csv file {csv_path}"))?;
    let mut rdr = csv::ReaderBuilder::new()
        .has_headers(false)
        .flexible(true)
        .from_reader(raw.as_bytes());
    let mut all_rows: Vec<Vec<String>> = Vec::new();
    for record in rdr.records() {
        let record = record.context("parse csv record")?;
        all_rows.push(record.iter().map(|s| s.to_string()).collect());
    }
    if all_rows.len() < 2 {
        bail!("CSV must have at least one header row and one data row");
    }
    let header = all_rows.remove(0);
    if header.is_empty() || header.iter().any(|cell| cell.trim().is_empty()) {
        bail!("CSV header cells must be non-empty");
    }
    for (idx, row) in all_rows.iter().enumerate() {
        if row.len() != header.len() {
            bail!(
                "CSV row {} has {} cells but header has {}; quote commas inside cell text or rewrite the cell",
                idx + 2,
                row.len(),
                header.len()
            );
        }
    }
    let conn = open(root)?;
    ensure_schema(&conn)?;
    let table_id = insert_report_table(
        &conn,
        run_id,
        kind,
        instance_id.as_deref(),
        &caption,
        legend.as_deref(),
        &header,
        &all_rows,
    )?;
    println!("table_id:   {table_id}");
    println!("kind:       {kind}");
    println!("rows:       {} (plus header)", all_rows.len());
    println!("cols:       {}", header.len());
    println!("caption:    {caption}");
    println!(
        "Cite from a block via the token {{{{tbl:{table_id}}}}} — the renderer assigns a tbl_number."
    );
    Ok(())
}

fn insert_report_table(
    conn: &Connection,
    run_id: &str,
    kind: &str,
    instance_id: Option<&str>,
    caption: &str,
    legend: Option<&str>,
    header: &[String],
    rows: &[Vec<String>],
) -> Result<String> {
    let header_json = serde_json::to_string(header)?;
    let rows_json = serde_json::to_string(rows)?;
    let table_id = new_id("tbl");
    let now = now_iso();
    conn.execute(
        "INSERT INTO report_tables (
            table_id, run_id, tbl_number, kind, instance_id, caption,
            legend, header_json, rows_json, created_at
         ) VALUES (?1, ?2, NULL, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
        params![
            table_id,
            run_id,
            kind,
            instance_id,
            caption,
            legend,
            header_json,
            rows_json,
            now,
        ],
    )
    .context("insert table row")?;
    Ok(table_id)
}

/// `ctox report table-list --run-id RUN [--json]`
fn cmd_table_list(root: &Path, args: &[String]) -> Result<()> {
    let run_id = require_run_id(args, "usage: ctox report table-list --run-id RUN [--json]")?;
    let _ = load_run(root, run_id)?;
    let json_out = has_flag(args, "--json");
    let conn = open(root)?;
    let mut stmt = conn.prepare(
        "SELECT table_id, kind, instance_id, caption, legend, header_json, rows_json \
         FROM report_tables WHERE run_id = ?1 ORDER BY created_at ASC",
    )?;
    let rows: Vec<(
        String,
        String,
        Option<String>,
        String,
        Option<String>,
        String,
        String,
    )> = stmt
        .query_map(params![run_id], |row| {
            Ok((
                row.get(0)?,
                row.get(1)?,
                row.get(2)?,
                row.get(3)?,
                row.get(4)?,
                row.get(5)?,
                row.get(6)?,
            ))
        })?
        .collect::<Result<Vec<_>, _>>()?;
    if json_out {
        let payload: Vec<Value> = rows
            .iter()
            .map(|r| {
                let header: Vec<String> = serde_json::from_str(&r.5).unwrap_or_default();
                let data: Vec<Vec<String>> = serde_json::from_str(&r.6).unwrap_or_default();
                json!({
                    "table_id": r.0,
                    "kind": r.1,
                    "instance_id": r.2,
                    "caption": r.3,
                    "legend": r.4,
                    "header": header,
                    "rows": data,
                })
            })
            .collect();
        println!("{}", serde_json::to_string_pretty(&Value::Array(payload))?);
    } else {
        println!("TABLES ({})", rows.len());
        for r in &rows {
            let cols: Vec<String> = serde_json::from_str(&r.5).unwrap_or_default();
            let data: Vec<Vec<String>> = serde_json::from_str(&r.6).unwrap_or_default();
            println!(
                "  {}\tkind={}\trows={}x{}\tcite as {{{{tbl:{}}}}}\t{}",
                r.0,
                r.1,
                data.len(),
                cols.len(),
                r.0,
                truncate_for_table(&r.3, 50)
            );
        }
    }
    Ok(())
}

/// `ctox report project-description-sync --run-id RUN`
///
/// Generates deterministic project-description attachments from the run
/// contract. The first supported attachment is the project-scope table:
/// Laufzeit, Status, Budget and Kostenbloecke are extracted from the raw topic
/// and committed project-scope prose, then stored as a native Word table.
fn cmd_project_description_sync(root: &Path, args: &[String]) -> Result<()> {
    let run_id = require_run_id(
        args,
        "usage: ctox report project-description-sync --run-id RUN",
    )?;
    let _ = load_run(root, run_id)?;
    let workspace = Workspace::load(root, run_id)?;
    let metadata = workspace.run_metadata()?;
    if metadata.report_type_id != "project_description" {
        bail!(
            "project-description-sync only applies to report_type project_description, got {}",
            metadata.report_type_id
        );
    }

    let mut source_text = metadata.raw_topic.clone();
    for block in workspace.committed_blocks().unwrap_or_default() {
        if block.block_id == "project_scope_budget_timeline"
            || block.block_id == "project_implementation_focus"
            || block.block_id == "project_innovation_project"
        {
            source_text.push('\n');
            source_text.push_str(&block.markdown);
        }
    }

    let rows = project_scope_rows_from_text(&source_text);
    if rows.is_empty() {
        bail!(
            "project-description-sync found no Laufzeit/Status/Budget/Kostenbloecke facts in topic or committed project-scope prose"
        );
    }

    let conn = open(root)?;
    ensure_schema(&conn)?;
    conn.execute(
        "DELETE FROM report_tables
         WHERE run_id = ?1
           AND instance_id = 'doc_project_description__project_scope_budget_timeline'
           AND lower(caption) IN ('projektumfang auf einen blick', 'projektrahmen auf einen blick')",
        params![run_id],
    )
    .context("delete prior generated project-description scope table")?;

    let headers = vec![
        "Rahmenparameter".to_string(),
        "Angabe".to_string(),
        "Herkunft".to_string(),
    ];
    let table_id = insert_report_table(
        &conn,
        run_id,
        "generic",
        Some("doc_project_description__project_scope_budget_timeline"),
        "Projektumfang auf einen Blick",
        Some("Aus dem Aufgabenrahmen bzw. dem committeten Projektumfang extrahiert; keine neuen Zahlen wurden ergaenzt."),
        &headers,
        &rows,
    )?;
    println!("Synced project-description scope table.");
    println!("table_id: {table_id}");
    println!("rows: {}", rows.len());
    Ok(())
}

fn project_scope_rows_from_text(source: &str) -> Vec<Vec<String>> {
    let fields = [
        (
            "Laufzeit",
            &["laufzeit", "umsetzungszeitraum", "projektlaufzeit"][..],
        ),
        ("Status", &["status", "vorhabenstatus", "projektstatus"][..]),
        ("Budget", &["budget", "gesamtbudget", "projektbudget"][..]),
        (
            "Kostenbloecke",
            &["kostenblöcke", "kostenbloecke", "kostenpositionen"][..],
        ),
    ];
    let mut rows = Vec::new();
    for (label, needles) in fields {
        if let Some(value) = extract_project_scope_value(source, needles) {
            rows.push(vec![
                label.to_string(),
                value,
                "Aufgabenrahmen / Projektangaben".to_string(),
            ]);
        }
    }
    rows
}

fn extract_project_scope_value(source: &str, needles: &[&str]) -> Option<String> {
    for line in source.lines() {
        let cleaned = line
            .trim()
            .trim_start_matches(['-', '*'])
            .trim()
            .trim_start_matches(|ch: char| ch.is_ascii_digit() || ch == '.' || ch == ')')
            .trim();
        if cleaned.is_empty() {
            continue;
        }
        let lower = cleaned.to_lowercase();
        if !needles.iter().any(|needle| lower.contains(needle)) {
            continue;
        }
        let value = cleaned
            .split_once(':')
            .map(|(_, value)| value.trim())
            .or_else(|| cleaned.split_once('-').map(|(_, value)| value.trim()))
            .unwrap_or(cleaned)
            .trim_matches(['.', ';']);
        let value = value.trim();
        if value.is_empty() || value.eq_ignore_ascii_case(cleaned) {
            continue;
        }
        return Some(value.to_string());
    }
    None
}

/// `ctox report source-review-sync --run-id RUN`
///
/// Rebuilds the release-critical source-review tables from persisted state
/// instead of letting the writer hand-author screening/catalogue numbers.
/// It deliberately does not invent sources: every visible catalogue row is
/// derived from `report_evidence_register`, and every reviewed-result total is
/// derived from `report_research_log.sources_count`.
fn cmd_source_review_sync(root: &Path, args: &[String]) -> Result<()> {
    let run_id = require_run_id(args, "usage: ctox report source-review-sync --run-id RUN")?;
    let _ = load_run(root, run_id)?;
    let workspace = Workspace::load(root, run_id)?;
    let metadata = workspace.run_metadata()?;
    if metadata.report_type_id != "source_review" {
        bail!(
            "source-review-sync only applies to report_type source_review, got {}",
            metadata.report_type_id
        );
    }
    let evidence = workspace.evidence_register()?;
    let research_logs = workspace.research_log_entries()?;
    if evidence.is_empty() {
        bail!("source-review-sync requires persisted evidence; add sources with `ctox report add-evidence` first");
    }
    if research_logs.is_empty() {
        bail!("source-review-sync requires persisted research logs; add search passes with `ctox report research-log-add` first");
    }

    let conn = open(root)?;
    ensure_schema(&conn)?;

    conn.execute(
        "DELETE FROM report_tables
         WHERE run_id = ?1
           AND (
             instance_id IN (
               'doc_source_review__source_review_search_method',
               'doc_source_review__source_review_catalog',
               'doc_source_review__source_review_taxonomy'
             )
             OR lower(caption) LIKE 'sources by group:%'
             OR lower(caption) LIKE 'source catalog:%'
             OR lower(caption) LIKE 'search protocol%'
             OR lower(caption) LIKE 'scoring model%'
           )",
        params![run_id],
    )
    .context("delete prior generated source-review tables")?;

    let evidence_ids: Vec<String> = evidence
        .iter()
        .map(|entry| entry.evidence_id.clone())
        .collect();
    let research_ids: Vec<String> = research_logs
        .iter()
        .map(|entry| entry.research_id.clone())
        .collect();
    let research_ids_json = serde_json::to_string(&research_ids)?;
    conn.execute(
        "UPDATE report_blocks
         SET used_research_ids_json = ?1
         WHERE run_id = ?2 AND block_id = 'source_review_search_method'",
        params![research_ids_json, run_id],
    )
    .context("link search-method block to persisted research logs")?;
    append_source_review_method_note(&conn, run_id, &research_logs)?;
    distribute_source_review_references(&conn, run_id, &evidence_ids)?;

    let scoring_headers = vec![
        "Score".to_string(),
        "Usefulness for the review".to_string(),
        "Typical evidence basis".to_string(),
    ];
    let scoring_rows = vec![
        vec![
            "A".to_string(),
            "Directly useful quantitative or normative source".to_string(),
            "Primary data, standard, regulation, technical report, dataset, or paper with explicit load/wind/vibration/payload values".to_string(),
        ],
        vec![
            "B".to_string(),
            "Useful contextual or partially quantitative source".to_string(),
            "Credible source with operating limits, methods, definitions, or extractable constraints".to_string(),
        ],
        vec![
            "C".to_string(),
            "Indirect support".to_string(),
            "Background, taxonomy, adjacent engineering evidence, or source path confirmation".to_string(),
        ],
        vec![
            "D".to_string(),
            "Low direct value".to_string(),
            "Metadata-only, inaccessible, duplicate, or weakly related source kept only for coverage/gap reasoning".to_string(),
        ],
    ];
    insert_report_table(
        &conn,
        run_id,
        "generic",
        Some("doc_source_review__source_review_taxonomy"),
        "Scoring model for source usefulness",
        Some("The score is assigned from persisted source metadata and extracted text depth; it is a triage score, not a scientific quality score."),
        &scoring_headers,
        &scoring_rows,
    )?;

    let search_headers = vec![
        "Search path".to_string(),
        "Search terms".to_string(),
        "Reviewed results".to_string(),
        "Included sources".to_string(),
        "Excluded results".to_string(),
        "Selection rationale".to_string(),
    ];
    let search_rows = source_review_search_rows(&research_logs, evidence.len() as i64);
    insert_report_table(
        &conn,
        run_id,
        "generic",
        Some("doc_source_review__source_review_search_method"),
        "Search protocol and source selection",
        Some("Reviewed-result totals come from persisted research logs; included-source totals are capped to the persisted evidence register."),
        &search_headers,
        &search_rows,
    )?;

    let source_headers = vec![
        "Group".to_string(),
        "Source".to_string(),
        "Type".to_string(),
        "Publisher / author".to_string(),
        "Year".to_string(),
        "Data contribution".to_string(),
        "Score".to_string(),
        "Access URL / DOI".to_string(),
    ];
    let mut grouped: std::collections::BTreeMap<String, Vec<Vec<String>>> =
        std::collections::BTreeMap::new();
    for entry in &evidence {
        let group = classify_source_review_group(entry);
        grouped.entry(group.clone()).or_default().push(vec![
            group,
            source_review_title(entry),
            source_review_type(entry),
            source_review_publisher_author(entry),
            entry.year.map(|year| year.to_string()).unwrap_or_default(),
            source_review_data_contribution(entry),
            source_review_score(entry),
            source_review_access(entry),
        ]);
    }
    for (group, rows) in grouped {
        insert_report_table(
            &conn,
            run_id,
            "generic",
            Some("doc_source_review__source_review_catalog"),
            &format!("Sources by group: {group}"),
            Some("Rows are generated from persisted evidence entries; empty access cells indicate a source that must be treated as a coverage gap rather than a quoted usable source."),
            &source_headers,
            &rows,
        )?;
    }

    println!("Synced source-review tables from persisted state.");
    println!("research_logs: {}", research_logs.len());
    println!(
        "reviewed_results: {}",
        research_logs
            .iter()
            .map(|entry| entry.sources_count.max(0))
            .sum::<i64>()
    );
    println!("evidence_sources: {}", evidence.len());
    Ok(())
}

fn append_source_review_method_note(
    conn: &Connection,
    run_id: &str,
    research_logs: &[crate::report::workspace::ResearchLogEntry],
) -> Result<()> {
    let existing: Option<String> = conn
        .query_row(
            "SELECT markdown FROM report_blocks
             WHERE run_id = ?1 AND block_id = 'source_review_search_method'
             ORDER BY ord LIMIT 1",
            params![run_id],
            |row| row.get(0),
        )
        .optional()?;
    let Some(markdown) = existing else {
        return Ok(());
    };
    if markdown.to_lowercase().contains("search paths covered:") {
        return Ok(());
    }
    let paths = research_logs
        .iter()
        .filter_map(|entry| entry.focus.as_deref())
        .map(str::trim)
        .filter(|focus| !focus.is_empty())
        .collect::<Vec<_>>()
        .join("; ");
    let terms = research_logs
        .iter()
        .map(|entry| entry.question.trim())
        .filter(|question| !question.is_empty())
        .take(12)
        .collect::<Vec<_>>()
        .join("; ");
    let note = format!(
        "\n\nSearch paths covered: {}. Search terms included: {}. Inclusion logic: retain sources with direct load, wind, gust, vibration, payload, mass-class, operating-limit, standard, regulatory, dataset, or manufacturer relevance; exclude duplicates, inaccessible metadata-only hits, generic marketing pages without usable data, and sources outside the target mass class unless they support definitions or gap assessment.",
        if paths.is_empty() { "persisted multi-path search logs" } else { &paths },
        if terms.is_empty() { "persisted query terms from the search log" } else { &terms }
    );
    conn.execute(
        "UPDATE report_blocks
         SET markdown = markdown || ?1
         WHERE run_id = ?2 AND block_id = 'source_review_search_method'",
        params![note, run_id],
    )
    .context("append source-review search-method provenance note")?;
    Ok(())
}

fn distribute_source_review_references(
    conn: &Connection,
    run_id: &str,
    evidence_ids: &[String],
) -> Result<()> {
    let blocks: Vec<String> = conn
        .prepare(
            "SELECT instance_id FROM report_blocks
             WHERE run_id = ?1 AND block_id LIKE 'source_review_%'
             ORDER BY ord ASC",
        )?
        .query_map(params![run_id], |row| row.get::<_, String>(0))?
        .collect::<Result<Vec<_>, _>>()?;
    if blocks.is_empty() {
        return Ok(());
    }
    let chunk_size = evidence_ids.len().div_ceil(blocks.len()).max(1);
    for (idx, instance_id) in blocks.iter().enumerate() {
        let start = idx * chunk_size;
        let end = ((idx + 1) * chunk_size).min(evidence_ids.len());
        let chunk = if start < evidence_ids.len() {
            evidence_ids[start..end].to_vec()
        } else {
            Vec::new()
        };
        let encoded = serde_json::to_string(&chunk)?;
        conn.execute(
            "UPDATE report_blocks
             SET used_reference_ids_json = ?1
             WHERE run_id = ?2 AND instance_id = ?3",
            params![encoded, run_id, instance_id],
        )
        .with_context(|| format!("attach evidence references to {instance_id}"))?;
    }
    Ok(())
}

fn source_review_search_rows(
    research_logs: &[crate::report::workspace::ResearchLogEntry],
    included_total: i64,
) -> Vec<Vec<String>> {
    let reviewed_total: i64 = research_logs
        .iter()
        .map(|entry| entry.sources_count.max(0))
        .sum::<i64>()
        .max(1);
    let mut remaining_included = included_total.max(0);
    let mut rows = Vec::new();
    for (idx, entry) in research_logs.iter().enumerate() {
        let reviewed = entry.sources_count.max(0);
        let included = if idx + 1 == research_logs.len() {
            remaining_included
        } else {
            let share =
                ((reviewed as f64 / reviewed_total as f64) * included_total as f64).round() as i64;
            share.clamp(0, remaining_included)
        };
        remaining_included = (remaining_included - included).max(0);
        rows.push(vec![
            entry
                .focus
                .as_deref()
                .filter(|focus| !focus.trim().is_empty())
                .unwrap_or("General search")
                .to_string(),
            entry.question.clone(),
            reviewed.to_string(),
            included.to_string(),
            reviewed.saturating_sub(included).to_string(),
            entry
                .summary
                .as_deref()
                .map(truncate_cell)
                .unwrap_or_else(|| "Filtered for sources with directly usable data, definitions, operating limits, standards, or gap evidence.".to_string()),
        ]);
    }
    rows
}

fn classify_source_review_group(entry: &crate::report::workspace::EvidenceEntry) -> String {
    let text = format!(
        "{} {} {} {} {}",
        entry.kind,
        entry.title.as_deref().unwrap_or_default(),
        entry.publisher.as_deref().unwrap_or_default(),
        entry.venue.as_deref().unwrap_or_default(),
        entry
            .url_canonical
            .as_deref()
            .or(entry.url_full_text.as_deref())
            .unwrap_or_default()
    )
    .to_lowercase();
    if contains_any(
        &text,
        &["faa", "easa", "cfr", "regulation", "authority", "agency"],
    ) {
        "Regulation and authorities".to_string()
    } else if contains_any(
        &text,
        &[
            "dod",
            "army",
            "navy",
            "air force",
            "nato",
            "mil-std",
            "dtic",
        ],
    ) {
        "Defence and technical reports".to_string()
    } else if contains_any(&text, &["nasa", "ntrs", "technical report", "report no"]) {
        "NASA and public technical reports".to_string()
    } else if contains_any(&text, &["astm", "iso", "rtca", "sae", "standard"]) {
        "Standards and norms".to_string()
    } else if contains_any(
        &text,
        &[
            "dataset",
            "repository",
            "github",
            "zenodo",
            "kaggle",
            "dataverse",
        ],
    ) {
        "Datasets and repositories".to_string()
    } else if contains_any(&text, &["patent", "us20", "ep0", "wipo"]) {
        "Patents and inventions".to_string()
    } else if contains_any(
        &text,
        &[
            "dji",
            "skydio",
            "manufacturer",
            "oem",
            "manual",
            "datasheet",
        ],
    ) {
        "OEM and industry sources".to_string()
    } else if contains_any(
        &text,
        &[
            "journal",
            "conference",
            "elsevier",
            "ieee",
            "springer",
            "mdpi",
            "arxiv",
        ],
    ) {
        "Academic literature".to_string()
    } else {
        "Web and secondary technical sources".to_string()
    }
}

fn source_review_title(entry: &crate::report::workspace::EvidenceEntry) -> String {
    entry
        .title
        .as_deref()
        .or(entry.canonical_id.as_deref())
        .unwrap_or(&entry.evidence_id)
        .trim()
        .to_string()
}

fn source_review_type(entry: &crate::report::workspace::EvidenceEntry) -> String {
    let kind = entry.kind.trim();
    if kind.is_empty() {
        "source".to_string()
    } else {
        kind.to_string()
    }
}

fn source_review_publisher_author(entry: &crate::report::workspace::EvidenceEntry) -> String {
    if let Some(publisher) = entry.publisher.as_deref().filter(|s| !s.trim().is_empty()) {
        return publisher.trim().to_string();
    }
    if !entry.authors.is_empty() {
        return entry
            .authors
            .iter()
            .take(3)
            .cloned()
            .collect::<Vec<_>>()
            .join("; ");
    }
    entry
        .venue
        .as_deref()
        .unwrap_or_default()
        .trim()
        .to_string()
}

fn source_review_data_contribution(entry: &crate::report::workspace::EvidenceEntry) -> String {
    let body = entry
        .abstract_md
        .as_deref()
        .or(entry.snippet_md.as_deref())
        .unwrap_or_default()
        .replace(['\n', '\t'], " ");
    if body.trim().is_empty() {
        return "Metadata only; use mainly for source-path coverage or gap assessment.".to_string();
    }
    truncate_cell(body.trim())
}

fn source_review_score(entry: &crate::report::workspace::EvidenceEntry) -> String {
    let title = entry.title.as_deref().unwrap_or_default().to_lowercase();
    let direct_terms = contains_any(
        &title,
        &[
            "load",
            "loads",
            "gust",
            "wind",
            "vibration",
            "payload",
            "weight",
            "mass",
        ],
    );
    if direct_terms && entry.content_chars >= 2_000 {
        "A - direct".to_string()
    } else if entry.content_chars >= 1_000 {
        "B - useful".to_string()
    } else if entry.content_chars >= 500 || source_review_access(entry).starts_with("http") {
        "C - indirect".to_string()
    } else {
        "D - weak".to_string()
    }
}

fn source_review_access(entry: &crate::report::workspace::EvidenceEntry) -> String {
    entry
        .url_canonical
        .as_deref()
        .or(entry.url_full_text.as_deref())
        .or_else(|| {
            entry.canonical_id.as_deref().filter(|id| {
                id.starts_with("http://") || id.starts_with("https://") || id.starts_with("10.")
            })
        })
        .unwrap_or_default()
        .trim()
        .to_string()
}

fn contains_any(haystack: &str, needles: &[&str]) -> bool {
    needles.iter().any(|needle| haystack.contains(needle))
}

fn truncate_cell(value: &str) -> String {
    let cleaned = value.split_whitespace().collect::<Vec<_>>().join(" ");
    if cleaned.chars().count() <= 240 {
        return cleaned;
    }
    let mut out = cleaned.chars().take(237).collect::<String>();
    out.push_str("...");
    out
}

// ============================================================
// Layer 3: Storyline + arc_position
// ============================================================

/// `ctox report storyline-set --run-id RUN --markdown-file F`
///
/// Persists the run-wide narrative spine. The Skill mandates that this
/// is set after the evidence pass and before any block-stage. The text
/// is a free-form narrative treatment in prose: the central tensions,
/// the naïve answer that gets overturned, the turning point, the
/// resolution as architecture.
fn cmd_storyline_set(root: &Path, args: &[String]) -> Result<()> {
    let run_id = require_run_id(
        args,
        "usage: ctox report storyline-set --run-id RUN --markdown-file F",
    )?;
    let _ = load_run(root, run_id)?;
    let path = find_flag(args, "--markdown-file")
        .ok_or_else(|| anyhow!("--markdown-file PATH is required"))?;
    let body =
        std::fs::read_to_string(path).with_context(|| format!("read storyline file {path}"))?;
    let trimmed = body.trim();
    if trimmed.chars().count() < 400 {
        bail!(
            "storyline must be at least ~400 chars of narrative prose (got {}). Cover: \
             central tension(s), naive answer + why it fails, turning-point finding, \
             resolution as architecture, block-arc role hints.",
            trimmed.chars().count()
        );
    }
    let conn = open(root)?;
    ensure_schema(&conn)?;
    let now = now_iso();
    let updated = conn.execute(
        "UPDATE report_runs SET storyline_md = ?1, storyline_set_at = ?2 \
         WHERE run_id = ?3",
        params![trimmed.to_string(), now, run_id],
    )?;
    if updated == 0 {
        bail!("run {run_id} not found");
    }
    println!("storyline persisted ({} chars)", trimmed.chars().count());
    println!("View with: ctox report storyline-show --run-id {run_id}");
    Ok(())
}

/// `ctox report storyline-show --run-id RUN [--json]`
fn cmd_storyline_show(root: &Path, args: &[String]) -> Result<()> {
    let run_id = require_run_id(
        args,
        "usage: ctox report storyline-show --run-id RUN [--json]",
    )?;
    let _ = load_run(root, run_id)?;
    let conn = open(root)?;
    let row: Option<(Option<String>, Option<String>)> = conn
        .query_row(
            "SELECT storyline_md, storyline_set_at FROM report_runs WHERE run_id = ?1",
            params![run_id],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .optional()?;
    let (story, set_at) = row.ok_or_else(|| anyhow!("run {run_id} not found"))?;
    let story = story.unwrap_or_default();
    if has_flag(args, "--json") {
        println!(
            "{}",
            serde_json::to_string_pretty(&json!({
                "run_id": run_id,
                "storyline_md": story,
                "storyline_set_at": set_at,
                "chars": story.chars().count(),
            }))?
        );
    } else if story.is_empty() {
        println!("(no storyline set yet — use `ctox report storyline-set`)");
    } else {
        println!(
            "storyline ({} chars, set {}):",
            story.chars().count(),
            set_at.as_deref().unwrap_or("?")
        );
        println!();
        println!("{}", story);
    }
    Ok(())
}

///
/// Stages one block-markdown into `report_pending_blocks` (paired with a
/// new `report_skill_runs` parent row). The caller — typically the
/// harness LLM that just authored the markdown — provides every field;
/// nothing is generated.
fn cmd_block_stage(root: &Path, args: &[String]) -> Result<()> {
    let run_id = require_run_id(
        args,
        "usage: ctox report block-stage --run-id RUN --instance-id ID --markdown-file F",
    )?;
    let _ = load_run(root, run_id)?;
    let instance_id = find_flag(args, "--instance-id")
        .ok_or_else(|| anyhow!("--instance-id is required"))?
        .to_string();
    let markdown_path =
        find_flag(args, "--markdown-file").ok_or_else(|| anyhow!("--markdown-file is required"))?;
    let markdown = normalise_markdown(&read_markdown_file(markdown_path)?);
    if markdown.trim().is_empty() {
        bail!("markdown file {markdown_path:?} is empty");
    }

    // Derive doc_id / block_id from instance_id "doc__block" if not given.
    let (default_doc, default_block) = match instance_id.split_once("__") {
        Some((d, b)) => (d.to_string(), b.to_string()),
        None => (String::from(""), instance_id.clone()),
    };
    let doc_id = find_flag(args, "--doc-id")
        .map(str::to_string)
        .unwrap_or(default_doc);
    let block_id = find_flag(args, "--block-id")
        .map(str::to_string)
        .unwrap_or(default_block);
    let title = find_flag(args, "--title").unwrap_or("").to_string();
    let ord: i64 = find_flag(args, "--ord")
        .and_then(|s| s.parse().ok())
        .unwrap_or(0);
    let reason = find_flag(args, "--reason")
        .unwrap_or("authored")
        .to_string();
    let used_reference_ids: Vec<String> = find_flag(args, "--used-reference-ids")
        .map(|raw| {
            raw.split(',')
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
                .collect()
        })
        .unwrap_or_default();

    let staged = StagedBlock {
        instance_id: instance_id.clone(),
        doc_id,
        block_id: block_id.clone(),
        block_template_id: block_id,
        title,
        ord,
        markdown,
        reason,
        used_reference_ids,
    };

    let conn = open(root)?;
    ensure_schema(&conn)?;
    let skill_run_id = new_id("skill_write");
    let record = SkillRunRecord {
        skill_run_id: skill_run_id.clone(),
        run_id: run_id.to_string(),
        kind: SkillRunKind::Write,
        summary: "harness-authored block".to_string(),
        blocking_reason: None,
        blocking_questions: Vec::new(),
        blocks: vec![staged.clone()],
        raw_output: Value::Null,
    };
    record_skill_run(&conn, &record)?;
    stage_pending_blocks(&conn, run_id, &skill_run_id, SkillRunKind::Write, &[staged])?;

    // Optional: thread arc_position through to the pending row so the
    // renderer + flow check see the dramatic role of this block.
    if let Some(pos) = find_flag(args, "--arc-position") {
        if !matches!(
            pos,
            "tension_open"
                | "tension_deepen"
                | "complication"
                | "turning_point"
                | "resolution_construct"
                | "resolution_ratify"
                | "support"
        ) {
            bail!(
                "--arc-position must be one of: tension_open, tension_deepen, complication, turning_point, resolution_construct, resolution_ratify, support (got {pos:?})"
            );
        }
        conn.execute(
            "UPDATE report_pending_blocks SET arc_position = ?1 \
             WHERE run_id = ?2 AND instance_id = ?3 AND skill_run_id = ?4",
            params![pos, run_id, instance_id, skill_run_id],
        )
        .context("set arc_position on pending block")?;
    }

    println!("Staged {instance_id} (skill_run_id {skill_run_id})");
    println!("Apply with: ctox report block-apply --run-id {run_id}");
    Ok(())
}

/// `ctox report block-apply --run-id RUN [--instance-id ID]...`
///
/// Commits all (or the named subset of) currently pending blocks to
/// `report_blocks`. After this point the blocks are visible to checks +
/// render.
fn cmd_block_apply(root: &Path, args: &[String]) -> Result<()> {
    let run_id = require_run_id(
        args,
        "usage: ctox report block-apply --run-id RUN [--instance-id ID]... [--used-research-ids r1,r2]",
    )?;
    let _ = load_run(root, run_id)?;
    let conn = open(root)?;
    ensure_schema(&conn)?;
    let instance_filter: Vec<String> = collect_flag(args, "--instance-id");
    let used_research_ids = parse_csv_list(find_flag(args, "--used-research-ids"));
    for research_id in &used_research_ids {
        let exists: Option<i64> = conn
            .query_row(
                "SELECT 1 FROM report_research_log WHERE run_id = ?1 AND research_id = ?2",
                params![run_id, research_id],
                |row| row.get(0),
            )
            .optional()?;
        if exists.is_none() {
            bail!("unknown research_id for run {run_id}: {research_id}");
        }
    }
    let pending = list_pending_blocks(&conn, run_id)?;
    if pending.is_empty() {
        println!("No pending blocks to apply.");
        return Ok(());
    }
    // Find the latest skill_run_id covering any pending row — patch.rs
    // applies per skill_run_id, so iterate distinct ones.
    let skill_run_ids: Vec<String> = conn
        .prepare("SELECT DISTINCT skill_run_id FROM report_pending_blocks WHERE run_id = ?1")?
        .query_map(params![run_id], |row| row.get::<_, String>(0))?
        .collect::<Result<Vec<_>, _>>()?;
    let instance_ids_filter = if instance_filter.is_empty() {
        None
    } else {
        Some(instance_filter.clone())
    };
    // Snapshot pending arc_positions before patch.rs drains them.
    let arc_map: std::collections::HashMap<String, String> = conn
        .prepare(
            "SELECT instance_id, arc_position FROM report_pending_blocks \
             WHERE run_id = ?1 AND arc_position IS NOT NULL",
        )?
        .query_map(params![run_id], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        })?
        .filter_map(|r| r.ok())
        .collect();

    let mut total_committed = 0usize;
    for skill_run_id in &skill_run_ids {
        let selection = PatchSelection {
            skill_run_id: skill_run_id.clone(),
            instance_ids: instance_ids_filter.clone(),
            used_research_ids: used_research_ids.clone(),
        };
        let outcome = apply_block_patch(&conn, run_id, &selection)?;
        total_committed += outcome.committed_block_ids.len();
    }

    // Propagate arc_positions to the committed rows.
    for (instance_id, pos) in &arc_map {
        let _ = conn.execute(
            "UPDATE report_blocks SET arc_position = ?1 \
             WHERE run_id = ?2 AND instance_id = ?3",
            params![pos, run_id, instance_id],
        );
    }

    println!("Committed {total_committed} block(s).");
    Ok(())
}

/// `ctox report block-list --run-id RUN [--pending] [--json]`
fn cmd_block_list(root: &Path, args: &[String]) -> Result<()> {
    let run_id = require_run_id(
        args,
        "usage: ctox report block-list --run-id RUN [--pending] [--json]",
    )?;
    let _ = load_run(root, run_id)?;
    let conn = open(root)?;
    ensure_schema(&conn)?;
    let json_out = has_flag(args, "--json");
    if has_flag(args, "--pending") {
        let pending = list_pending_blocks(&conn, run_id)?;
        if json_out {
            let payload: Vec<Value> = pending
                .iter()
                .map(|b| {
                    json!({
                        "instance_id": b.instance_id,
                        "doc_id": b.doc_id,
                        "block_id": b.block_id,
                        "title": b.title,
                        "ord": b.ord,
                        "markdown_chars": b.markdown.chars().count(),
                    })
                })
                .collect();
            println!("{}", serde_json::to_string_pretty(&Value::Array(payload))?);
        } else {
            println!("PENDING BLOCKS ({})", pending.len());
            for b in &pending {
                println!(
                    "  {}\tord={}\tchars={}\ttitle={}",
                    b.instance_id,
                    b.ord,
                    b.markdown.chars().count(),
                    truncate_for_table(&b.title, 60)
                );
            }
        }
        return Ok(());
    }
    // Committed blocks.
    let mut stmt = conn.prepare(
        "SELECT instance_id, doc_id, block_id, title, ord, length(markdown), committed_at
         FROM report_blocks WHERE run_id = ?1 ORDER BY ord ASC, committed_at ASC",
    )?;
    let rows: Vec<(String, String, String, String, i64, i64, String)> = stmt
        .query_map(params![run_id], |row| {
            Ok((
                row.get(0)?,
                row.get(1)?,
                row.get(2)?,
                row.get(3)?,
                row.get(4)?,
                row.get(5)?,
                row.get(6)?,
            ))
        })?
        .collect::<Result<Vec<_>, _>>()?;
    if json_out {
        let payload: Vec<Value> = rows
            .iter()
            .map(|(iid, did, bid, title, ord, len, ts)| {
                json!({
                    "instance_id": iid,
                    "doc_id": did,
                    "block_id": bid,
                    "title": title,
                    "ord": ord,
                    "markdown_chars": len,
                    "committed_at": ts,
                })
            })
            .collect();
        println!("{}", serde_json::to_string_pretty(&Value::Array(payload))?);
    } else {
        println!("COMMITTED BLOCKS ({})", rows.len());
        for (iid, _did, _bid, title, ord, len, _ts) in &rows {
            println!(
                "  {}\tord={}\tchars={}\ttitle={}",
                iid,
                ord,
                len,
                truncate_for_table(title, 60)
            );
        }
    }
    Ok(())
}

/// `ctox report check --run-id RUN <completeness|character_budget|release_guard|narrative_flow|deliverable_quality> [--json]`
///
/// Runs one of the four deterministic checks and persists the outcome to
/// `report_check_runs`. `narrative_flow` is the structural variant (no
/// LLM): missing-section / out-of-order / broken-cross-ref detector.
fn cmd_check(root: &Path, args: &[String]) -> Result<()> {
    let run_id = require_run_id(
        args,
        "usage: ctox report check --run-id RUN <kind> [--json]",
    )?;
    let _ = load_run(root, run_id)?;
    let kind = args
        .iter()
        .find(|a| {
            !a.starts_with("--")
                && *a != run_id
                && matches!(
                    a.as_str(),
                    "completeness"
                        | "character_budget"
                        | "release_guard"
                        | "narrative_flow"
                        | "deliverable_quality"
                )
        })
        .map(String::as_str)
        .ok_or_else(|| {
            anyhow!(
                "check kind must be one of: completeness, character_budget, release_guard, narrative_flow, deliverable_quality"
            )
        })?;
    let json_out = has_flag(args, "--json");

    let workspace = Workspace::load(root, run_id)?;
    let outcome: CheckOutcome = match kind {
        "completeness" => run_completeness_check(&workspace)?,
        "character_budget" => run_character_budget_check(&workspace)?,
        "release_guard" => run_release_guard_check(&workspace)?,
        "narrative_flow" => run_structural_narrative_flow(root, run_id, &workspace)?,
        "deliverable_quality" => run_deliverable_quality_check(&workspace)?,
        _ => unreachable!(),
    };

    let conn = open(root)?;
    ensure_schema(&conn)?;
    record_check_outcome(&conn, run_id, &outcome)?;

    if json_out {
        let payload = json!({
            "check_kind": outcome.check_kind,
            "ready_to_finish": outcome.ready_to_finish,
            "needs_revision": outcome.needs_revision,
            "summary": outcome.summary,
            "raw_payload": outcome.raw_payload,
        });
        println!("{}", serde_json::to_string_pretty(&payload)?);
    } else {
        println!(
            "{}: ready_to_finish={} needs_revision={}",
            outcome.check_kind, outcome.ready_to_finish, outcome.needs_revision
        );
        if !outcome.summary.trim().is_empty() {
            println!("Summary: {}", outcome.summary);
        }
    }
    Ok(())
}

/// Structural narrative-flow check (no LLM). Verifies that the run has
/// every required block, that block ordinals are monotone-non-decreasing,
/// and that no committed block is empty. Failures are surfaced via the
/// regular `CheckOutcome` shape.
fn run_structural_narrative_flow(
    root: &Path,
    run_id: &str,
    workspace: &Workspace<'_>,
) -> Result<CheckOutcome> {
    let report_type_id = workspace.run_metadata()?.report_type_id;
    let pack = AssetPack::load()?;
    let report_type = pack
        .report_type(&report_type_id)
        .with_context(|| format!("unknown report_type {report_type_id}"))?;
    // Required block_ids come from the report_type's document_blueprint.
    let blueprint_id = report_type.document_blueprint_id.as_str();
    let required: Vec<String> = if blueprint_id.is_empty() {
        report_type.block_library_keys.clone()
    } else {
        match pack.document_blueprint(blueprint_id) {
            Ok(blueprint) => blueprint
                .sequence
                .into_iter()
                .filter(|entry| entry.required)
                .map(|entry| entry.block_id)
                .collect(),
            Err(_) => report_type.block_library_keys.clone(),
        }
    };

    let conn = open(root)?;
    ensure_schema(&conn)?;
    let mut stmt = conn.prepare(
        "SELECT block_id, ord, length(markdown) FROM report_blocks
         WHERE run_id = ?1 ORDER BY ord ASC",
    )?;
    let rows: Vec<(String, i64, i64)> = stmt
        .query_map(params![run_id], |row| {
            Ok((row.get(0)?, row.get(1)?, row.get(2)?))
        })?
        .collect::<Result<Vec<_>, _>>()?;

    let committed_block_ids: Vec<String> = rows.iter().map(|(b, _, _)| b.clone()).collect();
    let missing: Vec<String> = required
        .iter()
        .filter(|b| !committed_block_ids.contains(b))
        .cloned()
        .collect();

    let mut empty_blocks: Vec<String> = Vec::new();
    for (block_id, _ord, len) in &rows {
        if *len < 50 {
            empty_blocks.push(block_id.clone());
        }
    }

    let mut order_violations: Vec<String> = Vec::new();
    let mut last_ord: i64 = i64::MIN;
    for (block_id, ord, _len) in &rows {
        if *ord < last_ord {
            order_violations.push(block_id.clone());
        }
        last_ord = *ord;
    }

    let ready = missing.is_empty() && empty_blocks.is_empty() && order_violations.is_empty();
    let needs_revision = !empty_blocks.is_empty() || !order_violations.is_empty();
    let summary = if ready {
        "narrative flow OK: required blocks present, ordinals monotone, no empty blocks".to_string()
    } else {
        let mut parts = Vec::new();
        if !missing.is_empty() {
            parts.push(format!("missing required blocks: {}", missing.join(", ")));
        }
        if !empty_blocks.is_empty() {
            parts.push(format!("empty blocks: {}", empty_blocks.join(", ")));
        }
        if !order_violations.is_empty() {
            parts.push(format!(
                "out-of-order blocks: {}",
                order_violations.join(", ")
            ));
        }
        parts.join("; ")
    };
    Ok(CheckOutcome {
        check_kind: "narrative_flow".to_string(),
        summary,
        check_applicable: true,
        ready_to_finish: ready,
        needs_revision,
        candidate_instance_ids: Vec::new(),
        goals: Vec::new(),
        reasons: Vec::new(),
        raw_payload: json!({
            "committed_block_ids": committed_block_ids,
            "missing_required": missing,
            "empty_blocks": empty_blocks,
            "order_violations": order_violations,
        }),
    })
}

/// `ctox report ask-user --run-id RUN --question "..."` (repeatable)
///
/// Records open questions for the operator. Returns the new
/// question_id(s); the harness LLM should also surface the questions in
/// its chat reply so the operator sees them immediately.
fn cmd_ask_user(root: &Path, args: &[String]) -> Result<()> {
    let run_id = require_run_id(
        args,
        "usage: ctox report ask-user --run-id RUN --question \"...\"",
    )?;
    let _ = load_run(root, run_id)?;
    let questions: Vec<String> = collect_flag(args, "--question");
    if questions.is_empty() {
        bail!("at least one --question \"...\" is required");
    }
    let section = find_flag(args, "--section").unwrap_or("ctox report ask-user");
    let reason = find_flag(args, "--reason").unwrap_or("operator decision required");
    let allow_fallback = if has_flag(args, "--allow-fallback") {
        1
    } else {
        0
    };
    let conn = open(root)?;
    ensure_schema(&conn)?;
    let question_id = new_id("q");
    let questions_json =
        serde_json::to_string(&questions).context("encode --question values as JSON")?;
    conn.execute(
        "INSERT INTO report_questions (
             question_id, run_id, section, reason, questions_json,
             allow_fallback, raised_at, answered_at, answer_text
         ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, NULL, NULL)",
        params![
            question_id,
            run_id,
            section,
            reason,
            questions_json,
            allow_fallback,
            now_iso(),
        ],
    )
    .context("failed to insert question row")?;
    println!("question_id={question_id}");
    for q in &questions {
        println!("  - {}", q);
    }
    println!(
        "Operator answers with: ctox report answer {run_id} --question-id {question_id} --answer \"...\""
    );
    Ok(())
}

/// `ctox report answer RUN_ID --question-id Q_ID --answer "..."`
fn cmd_answer(root: &Path, args: &[String]) -> Result<()> {
    let run_id = require_run_id(
        args,
        "usage: ctox report answer RUN_ID --question-id Q_ID --answer \"...\"",
    )?;
    let question_id =
        find_flag(args, "--question-id").ok_or_else(|| anyhow!("--question-id is required"))?;
    let answer = find_flag(args, "--answer").ok_or_else(|| anyhow!("--answer is required"))?;
    let conn = open(root)?;
    ensure_schema(&conn)?;
    let now = now_iso();
    let updated = conn.execute(
        "UPDATE report_questions
         SET answer_text = ?1, answered_at = ?2
         WHERE run_id = ?3 AND question_id = ?4",
        params![answer, now, run_id, question_id],
    )?;
    if updated == 0 {
        bail!("no open question {question_id} found for run {run_id}");
    }
    println!("Answered question {question_id}");
    Ok(())
}

fn cmd_render(root: &Path, args: &[String]) -> Result<()> {
    let run_id = first_positional(args).ok_or_else(|| {
        anyhow!(
            "usage: ctox report render RUN_ID --format docx|md|json [--out PATH] [--allow-draft]"
        )
    })?;
    let format = find_flag(args, "--format")
        .ok_or_else(|| anyhow!("--format docx|md|json is required"))?
        .to_ascii_lowercase();
    let out_path = find_flag(args, "--out").map(PathBuf::from);
    let allow_draft = has_flag(args, "--allow-draft");
    let workspace = Workspace::load(root, run_id)?;
    let manuscript = build_manuscript(&workspace)?;

    match format.as_str() {
        "md" | "markdown" => {
            let path = out_path.unwrap_or_else(|| PathBuf::from(format!("{run_id}.md")));
            let body = render_markdown(&manuscript, &MarkdownRenderOptions::default());
            let bytes = body.len();
            std::fs::write(&path, body)
                .with_context(|| format!("failed to write markdown to {}", path.display()))?;
            println!("Wrote {bytes} bytes to {}", path.display());
        }
        "json" => {
            let path = out_path.unwrap_or_else(|| PathBuf::from(format!("{run_id}.json")));
            let body = serde_json::to_string_pretty(&manuscript)
                .context("failed to serialise manuscript as JSON")?;
            let bytes = body.len();
            std::fs::write(&path, body)
                .with_context(|| format!("failed to write JSON to {}", path.display()))?;
            println!("Wrote {bytes} bytes to {}", path.display());
        }
        "docx" => {
            if !allow_draft {
                let conn = open(root)?;
                ensure_schema(&conn)?;
                ensure_required_release_checks_ready(&conn, run_id, "render DOCX")?;
            }
            let path = out_path.unwrap_or_else(|| PathBuf::from(format!("{run_id}.docx")));
            let skill_root = root
                .join("skills")
                .join("system")
                .join("research")
                .join("deep-research");
            match render_docx(&manuscript, &path, &skill_root, None) {
                Ok(outcome) => {
                    println!(
                        "Wrote {} bytes to {}",
                        outcome.byte_count,
                        outcome.output_path.display()
                    );
                    if !outcome.stdout_tail.trim().is_empty() {
                        println!("{}", outcome.stdout_tail.trim_end());
                    }
                }
                Err(DocxRenderError::DependencyMissing(dep)) => {
                    bail!(
                        "DOCX render requires Python dependency {dep}. Install it with: pip install {dep}"
                    );
                }
                Err(other) => return Err(anyhow!("{other}")),
            }
        }
        other => bail!("unknown --format {other:?}; want one of: md, json, docx"),
    }
    Ok(())
}

fn ensure_required_release_checks_ready(
    conn: &Connection,
    run_id: &str,
    action: &str,
) -> Result<()> {
    for kind in [
        "completeness",
        "character_budget",
        "release_guard",
        "narrative_flow",
        "deliverable_quality",
    ] {
        let row: Option<(i64, Option<String>)> = conn
            .query_row(
                "SELECT ready_to_finish, payload_json
                 FROM report_check_runs
                 WHERE run_id = ?1 AND check_kind = ?2
                 ORDER BY checked_at DESC LIMIT 1",
                params![run_id, kind],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .optional()
            .with_context(|| format!("failed to read last {kind} check"))?;
        match row {
            None => bail!(
                "cannot {action}: no {kind} check has run yet; run `ctox report check --run-id {run_id} {kind}` first"
            ),
            Some((ready, payload_text)) if ready == 0 => {
                let applicable = payload_text
                    .as_deref()
                    .and_then(|s| serde_json::from_str::<Value>(s).ok())
                    .and_then(|v| v.get("check_applicable").and_then(Value::as_bool))
                    .unwrap_or(true);
                if !applicable {
                    continue;
                }
                bail!("cannot {action}: {kind} is not ready_to_finish; inspect `ctox report check --run-id {run_id} {kind} --json`, revise the named blocks/assets, and re-run the check");
            }
            Some(_) => {}
        }
    }
    Ok(())
}

fn cmd_finalise(root: &Path, args: &[String]) -> Result<()> {
    let run_id =
        first_positional(args).ok_or_else(|| anyhow!("usage: ctox report finalise RUN_ID"))?;
    let conn = open(root)?;
    ensure_schema(&conn)?;
    ensure_required_release_checks_ready(&conn, run_id, "finalise")?;
    state_finalise(&conn, run_id)?;
    println!("Run {run_id} finalised.");
    Ok(())
}

fn cmd_abort(root: &Path, args: &[String]) -> Result<()> {
    let run_id = first_positional(args)
        .ok_or_else(|| anyhow!("usage: ctox report abort RUN_ID --reason \"...\""))?;
    let reason =
        find_flag(args, "--reason").ok_or_else(|| anyhow!("--reason \"...\" is required"))?;
    let conn = open(root)?;
    ensure_schema(&conn)?;
    state_abort(&conn, run_id, reason)?;
    println!("Run {run_id} aborted: {reason}");
    Ok(())
}

fn cmd_blueprints(_root: &Path, _args: &[String]) -> Result<()> {
    let pack = AssetPack::load()?;
    println!("REPORT TYPES");
    for r in &pack.report_types {
        println!(
            "  {}\t{}\ttypical_chars={}\tmin_sections={}",
            r.id, r.label, r.typical_chars, r.min_sections
        );
    }
    println!();
    println!("DOMAIN PROFILES");
    for d in &pack.domain_profiles {
        println!("  {}\t{}", d.id, d.label);
    }
    println!();
    println!("DEPTH PROFILES");
    for d in &pack.depth_profiles {
        let min_evidence = d
            .min_evidence_count
            .or_else(|| {
                d.evidence_floor
                    .get("min_sources")
                    .and_then(Value::as_u64)
                    .map(|v| v as u32)
            })
            .unwrap_or(0);
        println!(
            "  {}\t{}\tmin_evidence_count={}",
            d.id, d.label, min_evidence
        );
    }
    Ok(())
}

fn cmd_help() {
    println!(
        "ctox report — deep research report runs

The intelligence lives in the harness LLM driven by the
`research/deep-research` skill. These commands are the deterministic
building blocks the LLM calls to drive the run.

USAGE
  ctox report new <report_type> --domain <id> --depth <id> [--language en|de] --topic \"...\"
                                                 [--reference-doc PATH]... [--seed-doi DOI]...
                                                 [--review-doc PATH]...
  ctox report list [--status STATUS] [--limit N]
  ctox report status RUN_ID [--json]

  ctox report research-log-add --run-id RUN --question \"...\" --sources-count N
        [--focus \"...\"] [--resolver \"...\"] [--summary \"...\" | --summary-file F]
        [--raw-payload-file F]

  ctox report add-evidence --run-id RUN
        ( --doi DOI | --arxiv-id ID | --url URL )
        [--title T] [--authors \"A1; A2\"] [--year Y] [--venue V]
        [--abstract-file PATH] [--snippet-file PATH] [--license L]
        [--no-full-text]
  ctox report evidence-show --run-id RUN ( --evidence-id ID | --all )
        [--full-text] [--json]

  ctox report block-stage --run-id RUN --instance-id ID --markdown-file F
        [--doc-id D] [--block-id B] [--title T] [--ord N] [--reason R]
        [--used-reference-ids \"ev1,ev2\"] [--arc-position <pos>]
        (arc_position one of: tension_open | tension_deepen | complication
         | turning_point | resolution_construct | resolution_ratify | support)

  ctox report figure-add --run-id RUN --kind <schematic|chart|photo|extracted>
        --caption \"...\" --source \"...\" [--instance-id ID]
        ( --code-mermaid F | --code-python F | --code-graphviz F
        | --image-file F | --extract-from-evidence ev_X --page N )
  ctox report figure-list --run-id RUN [--json]

  ctox report table-add --run-id RUN
        --kind <matrix|scenario|defect_catalog|risk_register|abbreviations|generic>
        --caption \"...\" --csv-file F [--instance-id ID] [--legend \"...\"]
  ctox report table-list --run-id RUN [--json]
  ctox report project-description-sync --run-id RUN
        (creates the project-scope table from persisted project facts)
  ctox report review-import --run-id RUN --review-doc PATH [--review-doc PATH]...
        (imports Word comments as review feedback, anchored when possible)
  ctox report source-review-sync --run-id RUN
        (rebuilds search protocol, scoring model, and grouped source tables
         from persisted research logs and evidence)

  ctox report storyline-set --run-id RUN --markdown-file F
  ctox report storyline-show --run-id RUN [--json]
  ctox report block-apply --run-id RUN [--instance-id ID]...
        [--used-research-ids \"research_1,research_2\"]
  ctox report block-list --run-id RUN [--pending] [--json]

  ctox report check --run-id RUN <completeness|character_budget|release_guard|narrative_flow|deliverable_quality> [--json]

  ctox report ask-user --run-id RUN --question \"...\" [--question \"...\"]...
                       [--section S] [--reason R] [--allow-fallback]
  ctox report answer RUN_ID --question-id Q_ID --answer \"...\"

  ctox report render RUN_ID --format docx|md|json [--out PATH] [--allow-draft]
  ctox report finalise RUN_ID
  ctox report abort RUN_ID --reason \"...\"
  ctox report blueprints
  ctox report help

Skill: skills/system/research/deep-research/SKILL.md"
    );
}

// ---------- helpers ----------

fn truncate_for_table(s: &str, max: usize) -> String {
    let cleaned = s.replace(['\n', '\t'], " ");
    if cleaned.chars().count() <= max {
        return cleaned;
    }
    let mut out: String = cleaned.chars().take(max.saturating_sub(1)).collect();
    out.push('…');
    out
}

fn collect_last_check_outcomes(conn: &rusqlite::Connection, run_id: &str) -> Result<Value> {
    let kinds = [
        "completeness",
        "character_budget",
        "release_guard",
        "narrative_flow",
        "deliverable_quality",
    ];
    let mut out = serde_json::Map::new();
    for kind in kinds {
        let value = conn
            .query_row(
                "SELECT ready_to_finish, needs_revision, checked_at, payload_json
                 FROM report_check_runs
                 WHERE run_id = ?1 AND check_kind = ?2
                 ORDER BY checked_at DESC LIMIT 1",
                params![run_id, kind],
                |row| {
                    let ready: i64 = row.get(0)?;
                    let needs: i64 = row.get(1)?;
                    let checked_at: String = row.get(2)?;
                    let payload_text: Option<String> = row.get(3)?;
                    let payload = payload_text
                        .as_deref()
                        .and_then(|s| serde_json::from_str::<Value>(s).ok())
                        .unwrap_or(Value::Null);
                    Ok(json!({
                        "ready_to_finish": ready != 0,
                        "needs_revision": needs != 0,
                        "checked_at": checked_at,
                        "payload": payload,
                    }))
                },
            )
            .optional()?;
        out.insert(kind.to_string(), value.unwrap_or(Value::Null));
    }
    Ok(Value::Object(out))
}

fn short_check_summary(value: &Value) -> String {
    let ready = value
        .get("ready_to_finish")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let needs = value
        .get("needs_revision")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let summary = value
        .get("payload")
        .and_then(|p| p.get("summary"))
        .and_then(Value::as_str)
        .unwrap_or("");
    let mut out = format!("ready={ready}, needs_revision={needs}");
    if !summary.is_empty() {
        out.push_str(" — ");
        out.push_str(&truncate_for_table(summary, 80));
    }
    out
}

// ---------- DOCX comment extractor ----------
//
// Pulls `(instance_hint, body)` pairs from a `.docx` file's comments. For
// reviewed Fördervorhaben reference documents, the extractor also reads
// `word/document.xml`, finds the paragraph carrying each comment marker, and
// maps known FVH block anchors to the nearest project_description instance.
// The comment body keeps a short anchor excerpt so form/style feedback remains
// actionable even when no exact instance mapping exists.

fn extract_docx_comments(path: &Path) -> Result<Vec<(Option<String>, String)>> {
    let file =
        std::fs::File::open(path).with_context(|| format!("open docx {}", path.display()))?;
    let mut archive = zip::ZipArchive::new(file)
        .with_context(|| format!("read docx archive {}", path.display()))?;
    let mut comments_xml = String::new();
    let mut document_xml = String::new();
    let mut found = false;
    if let Ok(mut entry) = archive.by_name("word/comments.xml") {
        entry
            .read_to_string(&mut comments_xml)
            .context("read word/comments.xml")?;
        found = true;
    }
    if let Ok(mut entry) = archive.by_name("word/document.xml") {
        entry
            .read_to_string(&mut document_xml)
            .context("read word/document.xml")?;
    }
    if !found {
        return Ok(Vec::new());
    }
    let anchors = extract_docx_comment_anchors(&document_xml);
    let doc =
        roxmltree::Document::parse(&comments_xml).context("parse word/comments.xml as XML")?;
    let mut out: Vec<(Option<String>, String)> = Vec::new();
    for comment in doc
        .descendants()
        .filter(|n| n.tag_name().name() == "comment")
    {
        let id = attr_local(comment, "id").unwrap_or_default();
        let text = text_from_xml_node(comment);
        let trimmed = text.trim().to_string();
        if trimmed.is_empty() {
            continue;
        }
        let anchor = anchors.iter().find(|a| a.comment_id == id);
        let instance_hint = anchor.and_then(|a| a.instance_id.clone()).or_else(|| {
            map_review_anchor_to_project_instance(
                anchor.map(|a| a.anchor_text.as_str()).unwrap_or(""),
            )
        });
        let body = if let Some(anchor) = anchor {
            format!(
                "Reference anchor: {} | Reviewer comment: {}",
                truncate_for_table(&anchor.anchor_text, 260),
                trimmed
            )
        } else {
            trimmed
        };
        out.push((instance_hint, body));
    }
    Ok(out)
}

#[derive(Debug, Clone)]
struct DocxCommentAnchor {
    comment_id: String,
    anchor_text: String,
    instance_id: Option<String>,
}

fn extract_docx_comment_anchors(document_xml: &str) -> Vec<DocxCommentAnchor> {
    if document_xml.trim().is_empty() {
        return Vec::new();
    }
    let Ok(doc) = roxmltree::Document::parse(document_xml) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    let mut current_fvh_block: Option<String> = None;
    for para in doc
        .descendants()
        .filter(|n| n.is_element() && n.tag_name().name() == "p")
    {
        let text = text_from_xml_node(para);
        if let Some(block) = fvh_block_marker(&text) {
            current_fvh_block = Some(block);
        }
        let mut comment_ids = Vec::new();
        for marker in para.descendants().filter(|n| {
            n.is_element()
                && (n.tag_name().name() == "commentRangeStart"
                    || n.tag_name().name() == "commentReference")
        }) {
            if let Some(id) = attr_local(marker, "id") {
                if !comment_ids.iter().any(|known| known == &id) {
                    comment_ids.push(id);
                }
            }
        }
        if comment_ids.is_empty() {
            continue;
        }
        let instance_id = current_fvh_block
            .as_deref()
            .and_then(map_fvh_block_to_project_instance)
            .or_else(|| map_review_anchor_to_project_instance(&text));
        let anchor_text = if text.trim().is_empty() {
            current_fvh_block.clone().unwrap_or_default()
        } else {
            text.trim().to_string()
        };
        for comment_id in comment_ids {
            out.push(DocxCommentAnchor {
                comment_id,
                anchor_text: anchor_text.clone(),
                instance_id: instance_id.clone(),
            });
        }
    }
    out
}

fn attr_local(node: roxmltree::Node<'_, '_>, local: &str) -> Option<String> {
    node.attributes()
        .find(|attr| attr.name() == local)
        .map(|attr| attr.value().to_string())
}

fn text_from_xml_node(node: roxmltree::Node<'_, '_>) -> String {
    let mut text = String::new();
    for t in node
        .descendants()
        .filter(|n| n.is_element() && n.tag_name().name() == "t")
    {
        if !text.is_empty() {
            text.push(' ');
        }
        text.push_str(t.text().unwrap_or_default());
    }
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn fvh_block_marker(text: &str) -> Option<String> {
    let start = text.find("[[FVH_BLOCK:")? + "[[FVH_BLOCK:".len();
    let rest = &text[start..];
    let end = rest.find("]]")?;
    Some(rest[..end].trim().to_string())
}

fn map_fvh_block_to_project_instance(block: &str) -> Option<String> {
    let target = match block {
        "doc_company::company_legal"
        | "doc_company::company_profile"
        | "doc_company::company_organigramm"
        | "doc_company::company_history"
        | "doc_company::company_portfolio"
        | "doc_company::company_event_context" => {
            "doc_project_description__project_company_context"
        }
        "doc_project::project_intro" | "doc_project::project_01_title_scope" => {
            "doc_project_description__project_innovation_project"
        }
        "doc_project::project_01_current_state" => {
            "doc_project_description__project_problem_statement"
        }
        "doc_project::project_01_development_goal" => {
            "doc_project_description__project_target_picture"
        }
        "doc_project::project_01_state_of_art" => {
            "doc_project_description__project_market_delimitation"
        }
        "doc_project::project_01_challenges_measures" | "doc_project::project_01_workpackages" => {
            "doc_project_description__project_implementation_focus"
        }
        "doc_project::project_01_costs_timeline" => {
            "doc_project_description__project_scope_budget_timeline"
        }
        _ => return None,
    };
    Some(target.to_string())
}

fn map_review_anchor_to_project_instance(anchor: &str) -> Option<String> {
    let lower = anchor.to_lowercase();
    let target = if lower.contains("gesellschaft")
        || lower.contains("unternehmensprofil")
        || lower.contains("historie")
        || lower.contains("produkte / leistungen")
    {
        "doc_project_description__project_company_context"
    } else if lower.contains("problembereich")
        || lower.contains("problemstellung")
        || lower.contains("status quo")
        || lower.contains("derzeitiger stand")
    {
        "doc_project_description__project_problem_statement"
    } else if lower.contains("entwicklungsziel") || lower.contains("zielbild") {
        "doc_project_description__project_target_picture"
    } else if lower.contains("abgrenzung") || lower.contains("stand der technik") {
        "doc_project_description__project_market_delimitation"
    } else if lower.contains("herausforderung")
        || lower.contains("maßnahmen")
        || lower.contains("massnahmen")
        || lower.contains("arbeitspakete")
        || lower.contains("umsetzung")
    {
        "doc_project_description__project_implementation_focus"
    } else if lower.contains("kosten") || lower.contains("zeitraum") || lower.contains("budget") {
        "doc_project_description__project_scope_budget_timeline"
    } else {
        return None;
    };
    Some(target.to_string())
}

// Keep `RunStatus` / `SourceKind` linked even if a particular CLI build
// drops the references through dead-code elimination.
#[allow(dead_code)]
fn _unused_marker() {
    let _ = RunStatus::Created.as_str();
    let _ = SourceKind::Doi.as_str();
}
