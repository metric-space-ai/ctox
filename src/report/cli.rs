//! `ctox report …` command surface.
//!
//! Top-level dispatcher for every operator-facing deep-research command.
//! Argument parsing is hand-rolled (consistent with the rest of CTOX —
//! `src/main.rs` and `src/mission/queue.rs` follow the same pattern). The
//! commands ultimately call into `crate::report::state`,
//! `crate::report::manager`, `crate::report::workspace`,
//! `crate::report::patch`, and `crate::report::render`.

use std::collections::HashMap;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::time::Duration;

use anyhow::{anyhow, bail, Context, Result};
use rusqlite::{params, OptionalExtension};
use serde_json::{json, Value};

use crate::report::asset_pack::AssetPack;
use crate::report::manager::{run_manager, ManagerConfig, ManagerRunOutcome};
use crate::report::patch::{
    apply_block_patch, record_skill_run, stage_pending_blocks, PatchSelection, SkillRunKind,
    SkillRunRecord, StagedBlock,
};
use crate::report::render::{
    build_manuscript, render_docx, render_markdown, DocxRenderError, MarkdownRenderOptions,
};
use crate::report::schema::{ensure_schema, new_id, now_iso, open, RunStatus};
use crate::report::schemas::parse_write_or_revise;
use crate::report::sources::{ResolverStack, SourceKind};
use crate::report::state::{
    abort as state_abort, create_run, finalise as state_finalise, list_runs, load_run,
    CreateRunParams,
};
use crate::report::sub_skill::{CtoxSubSkillRunner, DefaultInferenceCallable};
use crate::report::tools::SubSkillRunner;
use crate::report::workspace::{SkillMode, Workspace};

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
        Some("run") => cmd_run(root, &args[1..]),
        Some("continue") => cmd_continue(root, &args[1..]),
        Some("answer") => cmd_answer(root, &args[1..]),
        Some("revise") => cmd_revise(root, &args[1..]),
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

/// Collect paired `--instance-id ID --goal "..."` flags. Each goal
/// belongs to the most recently seen `--instance-id`. Returns
/// `(instance_id, goal)` tuples in the order they appear on the command
/// line.
fn collect_instance_goal_pairs(args: &[String]) -> Vec<(String, String)> {
    let mut out: Vec<(String, String)> = Vec::new();
    let mut current_id: Option<String> = None;
    let mut idx = 0;
    while idx < args.len() {
        let cur = args[idx].as_str();
        if cur == "--instance-id" {
            if let Some(value) = args.get(idx + 1) {
                current_id = Some(value.trim().to_string());
                idx += 2;
                continue;
            }
        } else if cur == "--goal" {
            if let (Some(id), Some(goal)) = (current_id.as_ref(), args.get(idx + 1)) {
                out.push((id.clone(), goal.trim().to_string()));
                idx += 2;
                continue;
            }
        }
        idx += 1;
    }
    out
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

fn cmd_run(root: &Path, args: &[String]) -> Result<()> {
    run_with_config(root, args, /*continue_check*/ false)
}

fn cmd_continue(root: &Path, args: &[String]) -> Result<()> {
    let run_id =
        first_positional(args).ok_or_else(|| anyhow!("usage: ctox report continue RUN_ID"))?;
    // Pre-flight: every blocking question must be answered.
    let conn = open(root)?;
    ensure_schema(&conn)?;
    let open_count: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM report_questions
             WHERE run_id = ?1 AND answered_at IS NULL AND allow_fallback = 0",
            params![run_id],
            |row| row.get(0),
        )
        .context("failed to count open blocking questions")?;
    if open_count > 0 {
        bail!(
            "{open_count} open blocking question(s) — answer them with `ctox report answer` first"
        );
    }
    run_with_config(root, args, /*continue_check*/ true)
}

fn run_with_config(root: &Path, args: &[String], continue_check: bool) -> Result<()> {
    let _ = continue_check; // pre-checks already happened in the caller
    let run_id = first_positional(args)
        .ok_or_else(|| anyhow!("usage: ctox report run RUN_ID [--max-turns N] [--no-research] [--no-revision] [--max-duration-min N]"))?;
    let max_turns: u32 = find_flag(args, "--max-turns")
        .and_then(|s| s.parse().ok())
        .unwrap_or(90);
    let allow_research = !has_flag(args, "--no-research");
    let allow_revision = !has_flag(args, "--no-revision");
    let max_duration_min: u32 = find_flag(args, "--max-duration-min")
        .and_then(|s| s.parse().ok())
        .unwrap_or(18);

    // Confirm the run exists before constructing the heavyweight pieces.
    let _ = load_run(root, run_id)?;

    let config = ManagerConfig {
        max_turns: max_turns as usize,
        max_run_duration: Duration::from_secs(u64::from(max_duration_min) * 60),
        allow_research,
        allow_research_retry: allow_research,
        allow_revision,
        ..ManagerConfig::default()
    };
    let inference_for_runner = DefaultInferenceCallable::new(root);
    let inference_for_manager = DefaultInferenceCallable::new(root);
    let runner = CtoxSubSkillRunner::new(root, Box::new(inference_for_runner))
        .context("failed to construct sub-skill runner")?;

    let outcome = run_manager(root, run_id, config, &runner, &inference_for_manager)?;
    print_manager_outcome(&outcome);
    Ok(())
}

fn cmd_answer(root: &Path, args: &[String]) -> Result<()> {
    let run_id = first_positional(args).ok_or_else(|| {
        anyhow!("usage: ctox report answer RUN_ID --question-id Q_ID --answer \"...\"")
    })?;
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

fn cmd_revise(root: &Path, args: &[String]) -> Result<()> {
    let run_id = first_positional(args).ok_or_else(|| {
        anyhow!("usage: ctox report revise RUN_ID --instance-id ID --goal \"...\" [...]")
    })?;
    let pairs = collect_instance_goal_pairs(args);
    if pairs.is_empty() {
        bail!("at least one --instance-id ID --goal \"...\" pair is required");
    }
    // Group goals per instance_id for the sub-skill input.
    let mut by_instance: HashMap<String, Vec<String>> = HashMap::new();
    let mut order: Vec<String> = Vec::new();
    for (id, goal) in pairs {
        if !by_instance.contains_key(&id) {
            order.push(id.clone());
        }
        by_instance.entry(id).or_default().push(goal);
    }
    let instance_ids: Vec<String> = order;
    let goals: Vec<String> = instance_ids
        .iter()
        .filter_map(|id| by_instance.get(id))
        .flat_map(|gs| gs.iter().cloned())
        .collect();

    let workspace = Workspace::load(root, run_id)?;
    let input = workspace.skill_input(SkillMode::Revision, &instance_ids, None, &goals)?;

    let inference = DefaultInferenceCallable::new(root);
    let runner = CtoxSubSkillRunner::new(root, Box::new(inference))
        .context("failed to construct sub-skill runner")?;
    let raw = runner
        .run_revisor(&input)
        .context("revisor sub-skill returned an error")?;
    let parsed =
        parse_write_or_revise(&raw).context("revisor sub-skill output failed schema validation")?;

    let conn = open(root)?;
    ensure_schema(&conn)?;
    let skill_run_id = new_id("skill_revise");
    let raw_output_json =
        serde_json::to_value(&parsed).context("encode revisor output for skill run record")?;
    let blocks: Vec<StagedBlock> = parsed
        .blocks
        .iter()
        .map(|b| StagedBlock {
            instance_id: b.instance_id.clone(),
            doc_id: b.doc_id.clone(),
            block_id: b.block_id.clone(),
            block_template_id: b.block_id.clone(),
            title: b.title.clone(),
            ord: b.order,
            markdown: b.markdown.clone(),
            reason: b.reason.clone(),
            used_reference_ids: b.used_reference_ids.clone(),
        })
        .collect();
    let blocking_reason = if parsed.blocking_reason.trim().is_empty() {
        None
    } else {
        Some(parsed.blocking_reason.clone())
    };
    let record = SkillRunRecord {
        skill_run_id: skill_run_id.clone(),
        run_id: run_id.to_string(),
        kind: SkillRunKind::Revision,
        summary: parsed.summary.clone(),
        blocking_reason: blocking_reason.clone(),
        blocking_questions: parsed.blocking_questions.clone(),
        blocks: blocks.clone(),
        raw_output: raw_output_json,
    };
    record_skill_run(&conn, &record)?;
    stage_pending_blocks(
        &conn,
        run_id,
        &skill_run_id,
        SkillRunKind::Revision,
        &blocks,
    )?;

    if blocks.is_empty() {
        if let Some(reason) = blocking_reason {
            // Persist a question card so `ctox report answer` can follow up.
            if !parsed.blocking_questions.is_empty() {
                let question_id = new_id("q");
                let questions_json = serde_json::to_string(&parsed.blocking_questions)
                    .context("encode blocking_questions for question card")?;
                conn.execute(
                    "INSERT INTO report_questions (
                         question_id, run_id, section, reason, questions_json,
                         allow_fallback, raised_at, answered_at, answer_text
                     ) VALUES (?1, ?2, ?3, ?4, ?5, 0, ?6, NULL, NULL)",
                    params![
                        question_id,
                        run_id,
                        "ctox report revise",
                        reason,
                        questions_json,
                        now_iso(),
                    ],
                )
                .context("failed to persist revise question card")?;
                println!("Revisor blocked: {reason}");
                println!(
                    "New question {question_id} ({} item(s))",
                    parsed.blocking_questions.len()
                );
                println!("Answer with: ctox report answer {run_id} --question-id {question_id} --answer \"...\"");
                return Ok(());
            }
            println!("Revisor blocked: {reason}");
            return Ok(());
        }
        println!("Revisor returned no blocks and no blocking reason.");
        return Ok(());
    }

    let selection = PatchSelection {
        skill_run_id: skill_run_id.clone(),
        instance_ids: None,
        used_research_ids: Vec::new(),
    };
    let outcome = apply_block_patch(&conn, run_id, &selection)?;
    println!(
        "Revised {} block(s); committed {} (skill_run {})",
        blocks.len(),
        outcome.committed_block_ids.len(),
        skill_run_id
    );
    if !parsed.summary.trim().is_empty() {
        println!("Summary: {}", parsed.summary);
    }
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

USAGE
  ctox report new <report_type> --domain <id> --depth <id> [--language en|de] --topic \"...\"
                                                 [--reference-doc PATH]... [--seed-doi DOI]...
                                                 [--review-doc PATH]...
  ctox report list [--status STATUS] [--limit N]
  ctox report status RUN_ID [--json]
  ctox report run RUN_ID [--max-turns N] [--no-research] [--no-revision]
                          [--max-duration-min N]
  ctox report continue RUN_ID
  ctox report answer RUN_ID --question-id Q_ID --answer \"...\"
  ctox report revise RUN_ID --instance-id BLOCK_X --goal \"...\"
                            [--instance-id BLOCK_Y --goal \"...\"]...
  ctox report render RUN_ID --format docx|md|json [--out PATH]
  ctox report finalise RUN_ID
  ctox report abort RUN_ID --reason \"...\"
  ctox report blueprints
  ctox report help

Operator guide: skills/system/research/deep-research/SKILL.md
                skills/system/research/deep-research/references/setup_guide.md"
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

fn print_manager_outcome(outcome: &ManagerRunOutcome) {
    let decision = outcome.decision.as_str();
    println!("Decision:    {decision}");
    println!("Turns:       {}", outcome.turns);
    println!("Tool calls:  {}", outcome.tool_calls);
    if !outcome.summary.trim().is_empty() {
        println!("Summary:     {}", outcome.summary);
    }
    if !outcome.changed_blocks.is_empty() {
        println!("Changed:     {}", outcome.changed_blocks.join(", "));
    }
    if !outcome.reason.trim().is_empty() {
        println!("Reason:      {}", outcome.reason);
    }
    if !outcome.open_questions.is_empty() {
        println!("Open Qs ({}):", outcome.open_questions.len());
        for q in &outcome.open_questions {
            println!("  - {}", truncate_for_table(q, 100));
        }
        println!("Answer with: ctox report answer RUN_ID --question-id Q_ID --answer \"...\"");
    }
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
