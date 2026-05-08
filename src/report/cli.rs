//! `ctox report …` command surface.
//!
//! Deterministic CLI subcommands the harness LLM (loaded with the
//! `skills/system/research/deep-research/` skill) calls via Bash to drive
//! a deep-research run. There is no LLM loop in this module — every
//! command is a pure transform on the SQLite report store.

use std::io::Read;
use std::path::{Path, PathBuf};

use anyhow::{anyhow, bail, Context, Result};
use rusqlite::{params, OptionalExtension};
use serde_json::{json, Value};

use crate::report::asset_pack::AssetPack;
use crate::report::checks::{
    record_check_outcome, run_character_budget_check, run_completeness_check,
    run_release_guard_check, CheckOutcome,
};
use crate::report::patch::{
    apply_block_patch, list_pending_blocks, normalise_markdown, record_skill_run,
    stage_pending_blocks, PatchSelection, SkillRunKind, SkillRunRecord, StagedBlock,
};
use crate::report::render::{
    build_manuscript, render_docx, render_markdown, DocxRenderError, MarkdownRenderOptions,
};
use crate::report::schema::{ensure_schema, new_id, now_iso, open, RunStatus};
use crate::report::sources::full_text::{
    fetch_full_text, license_permits_open_access, FullTextFetch,
};
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
        Some("add-evidence") => cmd_add_evidence(root, &args[1..]),
        Some("evidence-show") => cmd_evidence_show(root, &args[1..]),
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

fn read_markdown_file(path_str: &str) -> Result<String> {
    let path = PathBuf::from(path_str);
    let body = std::fs::read_to_string(&path)
        .with_context(|| format!("read markdown file {}", path.display()))?;
    Ok(body)
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
    if !license_permits_open_access(source.license.as_deref()) {
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
    println!(
        "kind:        {}",
        source.kind.as_str()
    );
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
        let conn = open(root)?;
        ensure_schema(&conn)?;
        for path in &review_docs {
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
                        }
                    }
                }
                Err(err) => eprintln!("warning: review doc {path} parse error: {err}"),
            }
        }
    }

    println!("Run created: {run_id}");
    println!("Next: ctox report run {run_id}");
    Ok(())
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
        let stack = ResolverStack::new(root, run_id, None)
            .context("failed to construct resolver stack")?;
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
        let stack = ResolverStack::new(root, run_id, None)
            .context("failed to construct resolver stack")?;
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
    let authors_json = serde_json::to_string(&authors_vec)
        .context("encode --authors as JSON")?;
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
            if license_permits_open_access(license)
                || url.to_ascii_lowercase().contains("arxiv.org")
                || url.to_ascii_lowercase().ends_with(".pdf")
            {
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
                            eprintln!(
                                "warning: persisting full-text from --url failed: {err}"
                            );
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
    let params_dyn: Vec<&dyn rusqlite::ToSql> = bind
        .iter()
        .map(|s| s as &dyn rusqlite::ToSql)
        .collect();
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
                let authors: Vec<String> = r
                    .4
                    .as_deref()
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
            let authors: Vec<String> = r
                .4
                .as_deref()
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
///
/// Stages one block-markdown into `report_pending_blocks` (paired with a
/// new `report_skill_runs` parent row). The caller — typically the
/// harness LLM that just authored the markdown — provides every field;
/// nothing is generated.
fn cmd_block_stage(root: &Path, args: &[String]) -> Result<()> {
    let run_id = require_run_id(args, "usage: ctox report block-stage --run-id RUN --instance-id ID --markdown-file F")?;
    let _ = load_run(root, run_id)?;
    let instance_id = find_flag(args, "--instance-id")
        .ok_or_else(|| anyhow!("--instance-id is required"))?
        .to_string();
    let markdown_path = find_flag(args, "--markdown-file")
        .ok_or_else(|| anyhow!("--markdown-file is required"))?;
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
    let reason = find_flag(args, "--reason").unwrap_or("authored").to_string();
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
    let run_id = require_run_id(args, "usage: ctox report block-apply --run-id RUN [--instance-id ID]...")?;
    let _ = load_run(root, run_id)?;
    let conn = open(root)?;
    ensure_schema(&conn)?;
    let instance_filter: Vec<String> = collect_flag(args, "--instance-id");
    let pending = list_pending_blocks(&conn, run_id)?;
    if pending.is_empty() {
        println!("No pending blocks to apply.");
        return Ok(());
    }
    // Find the latest skill_run_id covering any pending row — patch.rs
    // applies per skill_run_id, so iterate distinct ones.
    let skill_run_ids: Vec<String> = conn
        .prepare(
            "SELECT DISTINCT skill_run_id FROM report_pending_blocks WHERE run_id = ?1",
        )?
        .query_map(params![run_id], |row| row.get::<_, String>(0))?
        .collect::<Result<Vec<_>, _>>()?;
    let instance_ids_filter = if instance_filter.is_empty() {
        None
    } else {
        Some(instance_filter.clone())
    };
    let mut total_committed = 0usize;
    for skill_run_id in &skill_run_ids {
        let selection = PatchSelection {
            skill_run_id: skill_run_id.clone(),
            instance_ids: instance_ids_filter.clone(),
            used_research_ids: Vec::new(),
        };
        let outcome = apply_block_patch(&conn, run_id, &selection)?;
        total_committed += outcome.committed_block_ids.len();
    }
    println!("Committed {total_committed} block(s).");
    Ok(())
}

/// `ctox report block-list --run-id RUN [--pending] [--json]`
fn cmd_block_list(root: &Path, args: &[String]) -> Result<()> {
    let run_id = require_run_id(args, "usage: ctox report block-list --run-id RUN [--pending] [--json]")?;
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

/// `ctox report check --run-id RUN <completeness|character_budget|release_guard|narrative_flow> [--json]`
///
/// Runs one of the four deterministic checks and persists the outcome to
/// `report_check_runs`. `narrative_flow` is the structural variant (no
/// LLM): missing-section / out-of-order / broken-cross-ref detector.
fn cmd_check(root: &Path, args: &[String]) -> Result<()> {
    let run_id = require_run_id(args, "usage: ctox report check --run-id RUN <kind> [--json]")?;
    let _ = load_run(root, run_id)?;
    let kind = args
        .iter()
        .find(|a| {
            !a.starts_with("--")
                && *a != run_id
                && matches!(
                    a.as_str(),
                    "completeness" | "character_budget" | "release_guard" | "narrative_flow"
                )
        })
        .map(String::as_str)
        .ok_or_else(|| {
            anyhow!(
                "check kind must be one of: completeness, character_budget, release_guard, narrative_flow"
            )
        })?;
    let json_out = has_flag(args, "--json");

    let workspace = Workspace::load(root, run_id)?;
    let outcome: CheckOutcome = match kind {
        "completeness" => run_completeness_check(&workspace)?,
        "character_budget" => run_character_budget_check(&workspace)?,
        "release_guard" => run_release_guard_check(&workspace)?,
        "narrative_flow" => run_structural_narrative_flow(root, run_id, &workspace)?,
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
        "narrative flow OK: required blocks present, ordinals monotone, no empty blocks"
            .to_string()
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
    let run_id = require_run_id(args, "usage: ctox report ask-user --run-id RUN --question \"...\"")?;
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
    let questions_json = serde_json::to_string(&questions)
        .context("encode --question values as JSON")?;
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
        anyhow!("usage: ctox report render RUN_ID --format docx|md|json [--out PATH]")
    })?;
    let format = find_flag(args, "--format")
        .ok_or_else(|| anyhow!("--format docx|md|json is required"))?
        .to_ascii_lowercase();
    let out_path = find_flag(args, "--out").map(PathBuf::from);
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

fn cmd_finalise(root: &Path, args: &[String]) -> Result<()> {
    let run_id =
        first_positional(args).ok_or_else(|| anyhow!("usage: ctox report finalise RUN_ID"))?;
    let conn = open(root)?;
    ensure_schema(&conn)?;
    for kind in [
        "completeness",
        "character_budget",
        "release_guard",
        "narrative_flow",
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
            None => bail!("no {kind} check has run yet — run `ctox report run {run_id}` first"),
            Some((ready, _)) if ready == 0 => {
                // Distinguish "the check itself reported not_applicable" vs "the run is not done".
                let payload_text: Option<String> = conn
                    .query_row(
                        "SELECT payload_json
                         FROM report_check_runs
                         WHERE run_id = ?1 AND check_kind = ?2
                         ORDER BY checked_at DESC LIMIT 1",
                        params![run_id, kind],
                        |row| row.get(0),
                    )
                    .optional()
                    .unwrap_or(None);
                let applicable = payload_text
                    .as_deref()
                    .and_then(|s| serde_json::from_str::<Value>(s).ok())
                    .and_then(|v| v.get("check_applicable").and_then(Value::as_bool))
                    .unwrap_or(true);
                if !applicable {
                    continue;
                }
                bail!("{kind} is not ready_to_finish — run `ctox report run` until it passes");
            }
            Some(_) => {}
        }
    }
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

  ctox report add-evidence --run-id RUN
        ( --doi DOI | --arxiv-id ID | --url URL )
        [--title T] [--authors \"A1; A2\"] [--year Y] [--venue V]
        [--abstract-file PATH] [--snippet-file PATH] [--license L]
        [--no-full-text]
  ctox report evidence-show --run-id RUN ( --evidence-id ID | --all )
        [--full-text] [--json]

  ctox report block-stage --run-id RUN --instance-id ID --markdown-file F
        [--doc-id D] [--block-id B] [--title T] [--ord N] [--reason R]
        [--used-reference-ids \"ev1,ev2\"]
  ctox report block-apply --run-id RUN [--instance-id ID]...
  ctox report block-list --run-id RUN [--pending] [--json]

  ctox report check --run-id RUN <completeness|character_budget|release_guard|narrative_flow> [--json]

  ctox report ask-user --run-id RUN --question \"...\" [--question \"...\"]...
                       [--section S] [--reason R] [--allow-fallback]
  ctox report answer RUN_ID --question-id Q_ID --answer \"...\"

  ctox report render RUN_ID --format docx|md|json [--out PATH]
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
// Pulls `(anchor, body)` pairs from a `.docx` file's `word/comments.xml`
// part. `anchor` is the comment's `w:initials` or `w:author` (best-effort
// instance hint); `body` is the concatenated comment text. Failures are
// surfaced as anyhow errors and logged at the call site.

fn extract_docx_comments(path: &Path) -> Result<Vec<(Option<String>, String)>> {
    let file =
        std::fs::File::open(path).with_context(|| format!("open docx {}", path.display()))?;
    let mut archive = zip::ZipArchive::new(file)
        .with_context(|| format!("read docx archive {}", path.display()))?;
    let mut comments_xml = String::new();
    let mut found = false;
    if let Ok(mut entry) = archive.by_name("word/comments.xml") {
        entry
            .read_to_string(&mut comments_xml)
            .context("read word/comments.xml")?;
        found = true;
    }
    if !found {
        return Ok(Vec::new());
    }
    let doc =
        roxmltree::Document::parse(&comments_xml).context("parse word/comments.xml as XML")?;
    let mut out: Vec<(Option<String>, String)> = Vec::new();
    for comment in doc
        .descendants()
        .filter(|n| n.tag_name().name() == "comment")
    {
        let initials = comment.attribute("initials").map(str::to_string);
        let author = comment.attribute("author").map(str::to_string);
        let mut text = String::new();
        for t in comment.descendants().filter(|n| n.tag_name().name() == "t") {
            if !text.is_empty() {
                text.push(' ');
            }
            text.push_str(t.text().unwrap_or_default());
        }
        let trimmed = text.trim().to_string();
        if trimmed.is_empty() {
            continue;
        }
        let hint = initials.filter(|s| !s.is_empty()).or(author);
        out.push((hint, trimmed));
    }
    Ok(out)
}

// Keep `RunStatus` / `SourceKind` linked even if a particular CLI build
// drops the references through dead-code elimination.
#[allow(dead_code)]
fn _unused_marker() {
    let _ = RunStatus::Created.as_str();
    let _ = SourceKind::Doi.as_str();
}
