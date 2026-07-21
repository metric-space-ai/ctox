//! `ctox web person-research` — high-level orchestrator for company / person
//! recherche.
//!
//! Structural sibling of [`crate::deep_research`]. Where deep-research
//! aggregates evidence across scholarly/web/data sources for free-form
//! topical queries, `person-research` aggregates **typed field evidence**
//! for one company at a time, using the registered source modules
//! ([`crate::sources`]) and the generic DACH prospect-research source matrix
//! ([`crate::sources::EXCEL_MATRIX`]) as the routing oracle.
//!
//! The orchestrator never talks to a search engine directly: it builds
//! [`PersonResearchPlan`]s and drives the existing
//! [`run_ctox_web_search_tool`] / [`run_ctox_web_read_tool`] primitives
//! with `pinned_sources` set per plan. Per-source `extract_fields` runs
//! automatically inside `run_ctox_web_read_tool` (Phase 3), so this file
//! only has to aggregate the resulting evidence per field.

use std::cmp::Reverse;
use std::collections::BTreeMap;
use std::collections::BTreeSet;
use std::fs;
use std::io::Write;
use std::path::Path;
use std::path::PathBuf;
use std::time::Duration;
use std::time::SystemTime;
use std::time::UNIX_EPOCH;

use anyhow::Context;
use anyhow::Result;
use chrono::DateTime;
use rusqlite::Connection;
use rusqlite::OpenFlags;
use rusqlite::OptionalExtension;
use serde_json::json;
use serde_json::Value;

use crate::sources::{
    self, scrape_bridge, Country, FieldKey, ResearchMode, SourceCtx, SourceHit, SourceModule, Tier,
};
use crate::web_search::{
    run_ctox_web_read_tool, run_ctox_web_search_tool, CanonicalWebSearchRequest, ContextSize,
    DirectWebReadRequest, SearchUserLocation,
};

const MAX_HITS_PER_SOURCE: usize = 4;
const MAX_READS_PER_SOURCE: usize = 2;
const MAX_EVIDENCE_AGE_MS: u64 = 24 * 60 * 60 * 1_000;
const MAX_CLOCK_SKEW_MS: u64 = 5 * 60 * 1_000;

// ---------------------------------------------------------------------------
// Public request / response shape
// ---------------------------------------------------------------------------

/// Input to [`run_ctox_person_research_tool`].
#[derive(Debug, Clone)]
pub struct PersonResearchRequest {
    pub company: String,
    pub country: Country,
    pub mode: ResearchMode,
    /// Subset of [`FieldKey`] the caller wants populated. Empty means
    /// "everything the (mode, country) tuple supports".
    pub fields: Vec<FieldKey>,
    /// Tier-C source IDs (e.g. `["linkedin.com", "dnbhoovers.com"]`) the
    /// tenant has explicitly opted in to. Without an entry here, Tier-C
    /// sources are skipped from the plan even if a credential is present.
    pub include_private: Vec<String>,
    /// Optional explicit workspace directory. Falls back to
    /// `runtime/research/person/<timestamp>-<slug>/` when `None`.
    pub workspace: Option<PathBuf>,
    pub persist_workspace: bool,
}

/// One plan entry: one source module that may contribute to one or more
/// requested fields.
#[derive(Debug, Clone)]
struct PersonResearchPlan {
    source_id: &'static str,
    aliases: &'static [&'static str],
    host_suffixes: &'static [&'static str],
    tier: Tier,
    target_fields: Vec<FieldKey>,
    /// `true` if the source has an API path (`fetch_direct`); informational
    /// only — the actual dispatch goes through `run_ctox_web_search_tool`
    /// with `pinned_sources = [source_id]`, which honours `fetch_direct`
    /// automatically via Phase 3 plumbing.
    api_path: bool,
}

// ---------------------------------------------------------------------------
// Tool entry point
// ---------------------------------------------------------------------------

pub fn run_ctox_person_research_tool(
    root: &Path,
    request: &PersonResearchRequest,
) -> Result<Value> {
    let company = normalize_required_company(&request.company)?;
    if matches!(
        request.mode,
        ResearchMode::HaveData | ResearchMode::UpdateInventoryGeneral
    ) {
        // HaveData → A-block, no research action.
        // UpdateInventoryGeneral → Excel B-block has no source columns
        // populated, so the plan is intentionally empty. We still emit a
        // structured response so the caller can persist provenance.
        return Ok(empty_plan_response(&company, request, "mode_skipped"));
    }

    let plans = build_person_research_plan(request);
    if plans.is_empty() {
        return Ok(empty_plan_response(
            &company,
            request,
            "no_sources_for_request",
        ));
    }

    let mut field_evidence: BTreeMap<FieldKey, Vec<Value>> = BTreeMap::new();
    let mut search_runs: Vec<Value> = Vec::with_capacity(plans.len());
    let mut read_runs: Vec<Value> = Vec::new();
    let mut scrape_runs: Vec<Value> = Vec::new();
    let mut browser_extract_runs: Vec<Value> = Vec::new();
    let mut browser_assist_tasks: Vec<Value> = Vec::new();
    let mut visited_urls: BTreeSet<String> = BTreeSet::new();
    let ctox_bin = scrape_bridge::default_ctox_bin();

    for plan in &plans {
        // Fields this source's scrape target already produced this iteration.
        // The cascade below intentionally still runs (to supplement fields the
        // scrape target did not cover), but must not re-emit evidence for a
        // field the scrape target already extracted — that is duplicate
        // evidence for the same (source, field).
        let mut scrape_covered_fields: BTreeSet<FieldKey> = BTreeSet::new();
        // If the module is registered as a CTOX scrape target, delegate
        // extraction to the universal-scraping pipeline. Drift then flows
        // through `ctox scrape execute --allow-heal` into the repair queue
        // instead of silently failing here.
        if let Some(module) = sources::find(plan.source_id) {
            if module.scrape_target_key().is_some() {
                let result = scrape_bridge::run_via_scrape_target(
                    module,
                    &company,
                    request.country,
                    root,
                    &ctox_bin,
                );
                scrape_runs.push(json!({
                    "source_id": plan.source_id,
                    "target_key": result.target_key,
                    "classification": result.classification,
                    "reason": result.reason,
                    "repair_queued": result.repair_queued,
                    "run_id": result.run_id,
                    "record_count": result.fields.len(),
                    "evidence_rejections": result.evidence_rejections,
                }));
                for (field, ev) in result.fields {
                    if !request.fields.is_empty() && !request.fields.contains(&field) {
                        continue;
                    }
                    field_evidence.entry(field).or_default().push(json!({
                        "value": ev.value,
                        "confidence": ev.confidence.as_str(),
                        "source_id": plan.source_id,
                        "source_url": ev.source_url,
                        "tier": tier_label(plan.tier),
                        "via": "scrape_target",
                        "note": ev.note,
                    }));
                    scrape_covered_fields.insert(field);
                }
                // For drift / unreachable / blocked, fall through to the
                // search+read path as a safety net; for `succeeded` we
                // still run the cascade in case the script returned a
                // partial result and the cascade can supplement.
            }
        }

        // Honor the source's `shape_query` for the search-engine query
        // text. Crawl/snippet sources frequently need a tightened query
        // (e.g. `site:` operators, role-keyword bias) that the bare
        // company name doesn't carry.
        let effective_query = sources::find(plan.source_id)
            .and_then(|m| {
                let ctx = SourceCtx {
                    root,
                    country: Some(request.country),
                    mode: request.mode,
                };
                m.shape_query(&company, &ctx).map(|s| s.query)
            })
            .unwrap_or_else(|| company.clone());

        let search_payload = run_ctox_web_search_tool(
            root,
            &CanonicalWebSearchRequest {
                query: effective_query,
                external_web_access: None,
                allowed_domains: Vec::new(),
                user_location: SearchUserLocation {
                    country: Some(request.country.as_iso().to_string()),
                    ..SearchUserLocation::default()
                },
                search_context_size: Some(ContextSize::Medium),
                search_content_types: Vec::new(),
                include_sources: true,
                pinned_sources: vec![plan.source_id.to_string()],
            },
        );

        let search_payload = match search_payload {
            Ok(payload) => payload,
            Err(err) => {
                search_runs.push(json!({
                    "source_id": plan.source_id,
                    "target_fields": plan.target_fields.iter().map(|f| f.as_str()).collect::<Vec<_>>(),
                    "ok": false,
                    "error": err.to_string(),
                }));
                continue;
            }
        };

        let ok = search_payload
            .get("ok")
            .and_then(Value::as_bool)
            .unwrap_or(false);
        let hits = search_payload
            .get("results")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();
        search_runs.push(json!({
            "source_id": plan.source_id,
            "target_fields": plan.target_fields.iter().map(|f| f.as_str()).collect::<Vec<_>>(),
            "tier": tier_label(plan.tier),
            "api_path": plan.api_path,
            "ok": ok,
            "hit_count": hits.len(),
            "provider": search_payload.get("provider").cloned().unwrap_or(Value::Null),
            "source_failures": search_payload.get("source_failures").cloned().unwrap_or_else(|| Value::Array(Vec::new())),
        }));
        browser_assist_tasks.extend(browser_assist_tasks_from_source_failures(
            plan,
            search_payload.get("source_failures"),
        ));
        if !ok {
            continue;
        }

        // Snippet-mining sources (e.g. `person-discovery`) extract typed
        // evidence directly from the search-result hits without needing to
        // fetch the gated profile pages behind them. If `extract_from_hits`
        // returns evidence, we use it and skip the per-hit read loop.
        if let Some(module) = sources::find(plan.source_id) {
            let hit_objs = parse_verified_hits(&hits, plan, &company);
            if !hit_objs.is_empty() {
                let ctx = SourceCtx {
                    root,
                    country: Some(request.country),
                    mode: request.mode,
                };
                let snippet_evidence = module.extract_from_hits(&ctx, &company, &hit_objs);
                if !snippet_evidence.is_empty() {
                    for (field, ev) in snippet_evidence {
                        if !request.fields.is_empty() && !request.fields.contains(&field) {
                            continue;
                        }
                        if scrape_covered_fields.contains(&field) {
                            continue;
                        }
                        field_evidence.entry(field).or_default().push(json!({
                            "value": ev.value,
                            "confidence": ev.confidence.as_str(),
                            "source_id": plan.source_id,
                            "source_url": ev.source_url,
                            "tier": tier_label(plan.tier),
                            "via": "search_snippet",
                            "note": ev.note,
                        }));
                    }
                    // Snippet path produced evidence — do not also read each
                    // URL (typically gated by login walls).
                    continue;
                }
            }
        }

        let mut reads_for_plan = 0_usize;
        for hit in hits.iter().take(MAX_HITS_PER_SOURCE) {
            if reads_for_plan >= MAX_READS_PER_SOURCE {
                break;
            }
            if !search_hit_evidence_eligible(hit, plan, &company) {
                continue;
            }
            let Some(url) = hit.get("url").and_then(Value::as_str) else {
                continue;
            };
            if !visited_urls.insert(url.to_string()) {
                continue;
            }
            // Only read hits whose host belongs to the pinned source — a
            // pinned-source `fetch_direct` already returned typed data, but
            // the generic cascade may have added unrelated URLs to the
            // result list.
            if !url_belongs_to_source(url, plan.source_id, plan.aliases, plan.host_suffixes) {
                continue;
            }
            let read_payload = run_ctox_web_read_tool(
                root,
                &DirectWebReadRequest {
                    url: url.to_string(),
                    query: Some(company.clone()),
                    find: find_terms_for_fields(&plan.target_fields),
                    workspace: None,
                    include_full_text: false,
                    timeout_cap_ms: None,
                    max_artifact_bytes: None,
                    country: Some(request.country.as_iso().to_string()),
                },
            );
            match read_payload {
                Ok(payload) => {
                    if !web_read_evidence_eligible(&payload, plan, &company, url) {
                        read_runs.push(json!({
                            "source_id": plan.source_id,
                            "url": url,
                            "ok": false,
                            "evidence_rejected": true,
                        }));
                        continue;
                    }
                    if let Some(fields) = payload
                        .get("extracted_fields")
                        .and_then(|v| v.get("fields"))
                        .and_then(Value::as_array)
                    {
                        for entry in fields {
                            let Some(field_str) = entry.get("field").and_then(Value::as_str) else {
                                continue;
                            };
                            let Some(field) = FieldKey::from_str(field_str) else {
                                continue;
                            };
                            if !request.fields.is_empty() && !request.fields.contains(&field) {
                                continue;
                            }
                            if scrape_covered_fields.contains(&field) {
                                continue;
                            }
                            let Some(source_url) = entry
                                .get("canonical_url")
                                .or_else(|| entry.get("source_url"))
                                .and_then(Value::as_str)
                                .filter(|value| {
                                    valid_http_url(value)
                                        && url_belongs_to_source(
                                            value,
                                            plan.source_id,
                                            plan.aliases,
                                            plan.host_suffixes,
                                        )
                                })
                            else {
                                continue;
                            };
                            if !entry_company_identity_matches(&payload, entry, &company) {
                                continue;
                            }
                            let mut tagged = entry.clone();
                            tagged["source_id"] = Value::String(plan.source_id.to_string());
                            tagged["tier"] = Value::String(tier_label(plan.tier).to_string());
                            tagged["hit_url"] = Value::String(url.to_string());
                            tagged["source_url"] = Value::String(source_url.to_string());
                            tagged["canonical_url"] = Value::String(
                                payload
                                    .get("canonical_url")
                                    .and_then(Value::as_str)
                                    .unwrap_or(source_url)
                                    .to_string(),
                            );
                            tagged["evidence_eligible"] = Value::Bool(true);
                            tagged["verification_status"] = Value::String("verified".into());
                            tagged["checked_at"] =
                                payload.get("checked_at").cloned().unwrap_or(Value::Null);
                            field_evidence.entry(field).or_default().push(tagged);
                        }
                    }
                    read_runs.push(json!({
                        "source_id": plan.source_id,
                        "url": url,
                        "ok": payload.get("ok").cloned().unwrap_or(Value::Bool(false)),
                        "is_pdf": payload.get("is_pdf").cloned().unwrap_or(Value::Bool(false)),
                        "extracted_count": payload
                            .get("extracted_fields")
                            .and_then(|v| v.get("fields"))
                            .and_then(Value::as_array)
                            .map(Vec::len)
                            .unwrap_or(0),
                    }));
                    reads_for_plan += 1;
                }
                Err(err) => {
                    read_runs.push(json!({
                        "source_id": plan.source_id,
                        "url": url,
                        "ok": false,
                        "error": err.to_string(),
                    }));
                }
            }
        }
    }

    match collect_browser_extract_evidence(root, &company, plans.as_slice(), &request.fields) {
        Ok((browser_evidence, runs)) => {
            browser_extract_runs = runs;
            for (field, candidates) in browser_evidence {
                field_evidence.entry(field).or_default().extend(candidates);
            }
        }
        Err(err) => {
            browser_extract_runs.push(json!({
                "ok": false,
                "via": "browser_extract",
                "error": err.to_string(),
            }));
        }
    }

    let aggregated = aggregate_fields(&request.fields, plans.as_slice(), field_evidence);

    let payload = json!({
        "ok": true,
        "tool": "ctox_person_research",
        "company": company,
        "country": request.country.as_iso(),
        "mode": request.mode.as_str(),
        "requested_fields": request
            .fields
            .iter()
            .map(|f| f.as_str())
            .collect::<Vec<_>>(),
        "plan": plans
            .iter()
            .map(|p| json!({
                "source_id": p.source_id,
                "tier": tier_label(p.tier),
                "api_path": p.api_path,
                "target_fields": p.target_fields.iter().map(|f| f.as_str()).collect::<Vec<_>>(),
            }))
            .collect::<Vec<_>>(),
        "fields": aggregated,
        "search_runs": search_runs,
        "read_runs": read_runs,
        "scrape_runs": scrape_runs,
        "browser_extract_runs": browser_extract_runs,
        "browser_assist_tasks": browser_assist_tasks,
        "browser_assist_recommendations": browser_assist_recommendations(plans.as_slice()),
    });

    if request.persist_workspace {
        let workspace = request
            .workspace
            .clone()
            .unwrap_or_else(|| default_person_workspace(root, &company));
        match persist_person_workspace(&workspace, request, &payload) {
            Ok(summary) => {
                let mut payload = payload;
                payload["workspace"] = summary;
                return Ok(payload);
            }
            Err(err) => {
                let mut payload = payload;
                payload["workspace_error"] = Value::String(err.to_string());
                return Ok(payload);
            }
        }
    }

    Ok(payload)
}

// ---------------------------------------------------------------------------
// Plan
// ---------------------------------------------------------------------------

fn build_person_research_plan(request: &PersonResearchRequest) -> Vec<PersonResearchPlan> {
    // The "fields wanted" are the explicit request list, or every field
    // any registered source claims authority for in the given country.
    let wanted: Vec<FieldKey> = if request.fields.is_empty() {
        let mut all: BTreeSet<FieldKey> = BTreeSet::new();
        for module in sources::list() {
            if !module.countries().contains(&request.country) {
                continue;
            }
            for f in module.authoritative_for() {
                all.insert(*f);
            }
        }
        all.into_iter().collect()
    } else {
        request.fields.clone()
    };

    // We probe sources without a runtime root yet — we only need
    // `requires_credential` and per-module country/field tables, which the
    // SourceCtx doesn't influence. For shape_query the orchestrator passes
    // a real ctx via the search-tool's pinned_sources path.
    let mut per_source: BTreeMap<&'static str, PersonResearchPlan> = BTreeMap::new();
    for field in &wanted {
        for module in sources::list() {
            if !module.countries().contains(&request.country) {
                continue;
            }
            if !module.authoritative_for().contains(field) {
                continue;
            }
            if module.privacy_opt_in_required()
                && !is_source_opted_in(module, &request.include_private)
            {
                continue;
            }
            let entry = per_source
                .entry(module.id())
                .or_insert_with(|| PersonResearchPlan {
                    source_id: module.id(),
                    aliases: module.aliases(),
                    host_suffixes: module.host_suffixes(),
                    tier: module.tier(),
                    target_fields: Vec::new(),
                    api_path: probe_api_path(module),
                });
            if !entry.target_fields.contains(field) {
                entry.target_fields.push(*field);
            }
        }
    }

    let mut plans: Vec<PersonResearchPlan> = per_source.into_values().collect();
    // Tier ordering P → S → C. Within a tier, deterministic by id.
    plans.sort_by(|a, b| a.tier.cmp(&b.tier).then(a.source_id.cmp(b.source_id)));
    plans
}

fn is_source_opted_in(module: &'static dyn SourceModule, include_private: &[String]) -> bool {
    let id = module.id().to_ascii_lowercase();
    for raw in include_private {
        let needle = raw.trim().to_ascii_lowercase();
        if needle == id {
            return true;
        }
        if module
            .aliases()
            .iter()
            .any(|alias| alias.eq_ignore_ascii_case(&needle))
        {
            return true;
        }
    }
    false
}

/// Heuristic: a module has an API path iff `fetch_direct` returns `Some`
/// when called with a synthetic context.  We pass a fake root that
/// doesn't carry credentials, so credential-gated APIs still report
/// `Some(Err(CredentialMissing))` (which proves the API path exists),
/// while pure crawl modules return `None`.
fn probe_api_path(module: &'static dyn SourceModule) -> bool {
    let ctx = SourceCtx {
        root: Path::new(""),
        country: Some(*module.countries().first().unwrap_or(&Country::De)),
        mode: ResearchMode::NewRecord,
    };
    module.fetch_direct(&ctx, "probe").is_some()
}

fn browser_assist_recommendations(plans: &[PersonResearchPlan]) -> Vec<Value> {
    plans
        .iter()
        .filter_map(|plan| {
            let module = sources::find(plan.source_id)?;
            let recipe = module.browser_recipe()?;
            Some(json!({
                "source_id": recipe.source_id,
                "target_fields": plan.target_fields.iter().map(|field| field.as_str()).collect::<Vec<_>>(),
                "reason": "credentialed_source_browser_assist_available",
                "stream": "rxdb",
                "target_url": recipe.login_url,
                "allowed_domains": recipe.allowed_domains,
                "required_secret_name": recipe.required_secret_name,
                "verify_selector": recipe.verify_selector,
                "credential_selector": recipe.credential_selector,
                "capture_script": recipe.capture_script,
                "secret_value_in_payload": false,
            }))
        })
        .collect()
}

fn browser_assist_tasks_from_source_failures(
    plan: &PersonResearchPlan,
    failures: Option<&Value>,
) -> Vec<Value> {
    let Some(items) = failures.and_then(Value::as_array) else {
        return Vec::new();
    };
    let mut tasks = Vec::new();
    for failure in items {
        let source_matches = failure.get("source_id").and_then(Value::as_str)
            == Some(plan.source_id)
            || failure.get("requested_source").and_then(Value::as_str) == Some(plan.source_id)
            || failure
                .get("requested_source")
                .and_then(Value::as_str)
                .is_some_and(|requested| {
                    plan.aliases
                        .iter()
                        .any(|alias| alias.eq_ignore_ascii_case(requested))
                });
        if !source_matches {
            continue;
        }
        let kind = failure
            .get("kind")
            .and_then(Value::as_str)
            .unwrap_or_default();
        if !matches!(kind, "credential_missing" | "blocked") {
            continue;
        }
        let Some(browser_assist) = failure
            .get("browser_assist")
            .filter(|value| value.is_object())
        else {
            continue;
        };
        tasks.push(json!({
            "source_id": plan.source_id,
            "target_fields": plan.target_fields.iter().map(|field| field.as_str()).collect::<Vec<_>>(),
            "reason": kind,
            "error": failure.get("error").cloned().unwrap_or(Value::Null),
            "secret_name": failure.get("secret_name").cloned().unwrap_or(Value::Null),
            "status": "auth_assist_required",
            "stream": "rxdb",
            "browser_assist": browser_assist,
            "next_command": format!("ctox business-os web-stack auth-assist-request --source-id {}", plan.source_id),
            "secret_value_in_payload": false,
            "frame_data_in_payload": false,
        }));
    }
    tasks
}

// ---------------------------------------------------------------------------
// Aggregation
// ---------------------------------------------------------------------------

fn aggregate_fields(
    requested: &[FieldKey],
    plans: &[PersonResearchPlan],
    raw: BTreeMap<FieldKey, Vec<Value>>,
) -> Value {
    let mut out = serde_json::Map::new();
    let universe: Vec<FieldKey> = if requested.is_empty() {
        let mut all: BTreeSet<FieldKey> = BTreeSet::new();
        for plan in plans {
            for f in &plan.target_fields {
                all.insert(*f);
            }
        }
        all.into_iter().collect()
    } else {
        requested.to_vec()
    };

    for field in universe {
        let candidates = raw.get(&field).cloned().unwrap_or_default();
        if candidates.is_empty() {
            out.insert(
                field.as_str().to_string(),
                json!({
                    "value": Value::Null,
                    "confidence": "missing",
                    "candidates": [],
                }),
            );
            continue;
        }
        let mut ranked = candidates;
        ranked.sort_by(|a, b| {
            confidence_rank(b.get("confidence").and_then(Value::as_str)).cmp(&confidence_rank(
                a.get("confidence").and_then(Value::as_str),
            ))
        });
        let top = &ranked[0];
        out.insert(
            field.as_str().to_string(),
            json!({
                "value": top.get("value").cloned().unwrap_or(Value::Null),
                "confidence": top.get("confidence").cloned().unwrap_or(Value::Null),
                "source_id": top.get("source_id").cloned().unwrap_or(Value::Null),
                "source_url": top.get("source_url").cloned().unwrap_or(Value::Null),
                "tier": top.get("tier").cloned().unwrap_or(Value::Null),
                "note": top.get("note").cloned().unwrap_or(Value::Null),
                "candidates": ranked,
            }),
        );
    }
    Value::Object(out)
}

fn confidence_rank(raw: Option<&str>) -> u8 {
    match raw {
        Some("high") | Some("user_provided") => 3,
        Some("medium") => 2,
        Some("low") => 1,
        _ => 0,
    }
}

// ---------------------------------------------------------------------------
// Browser extract evidence
// ---------------------------------------------------------------------------

fn collect_browser_extract_evidence(
    root: &Path,
    company: &str,
    plans: &[PersonResearchPlan],
    requested: &[FieldKey],
) -> Result<(BTreeMap<FieldKey, Vec<Value>>, Vec<Value>)> {
    let db_path = root.join("runtime").join("ctox.sqlite3");
    if !db_path.is_file() {
        return Ok((BTreeMap::new(), Vec::new()));
    }

    let conn = Connection::open_with_flags(
        &db_path,
        OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )
    .with_context(|| {
        format!(
            "failed to open Business-OS RxDB store {}",
            db_path.display()
        )
    })?;

    let table_name: Option<String> = conn
        .query_row(
            "SELECT name
             FROM sqlite_master
             WHERE type = 'table'
               AND (name = 'ctox_business_os__business_commands__v1'
                    OR name LIKE '%__business_commands__v1')
             ORDER BY CASE WHEN name = 'ctox_business_os__business_commands__v1' THEN 0 ELSE 1 END,
                      name
             LIMIT 1",
            [],
            |row| row.get(0),
        )
        .optional()?;
    let Some(table_name) = table_name else {
        return Ok((BTreeMap::new(), Vec::new()));
    };

    let mut stmt = conn.prepare(&format!(
        "SELECT data FROM {}",
        quote_sql_identifier(&table_name)
    ))?;
    let rows = stmt.query_map([], |row| row.get::<_, String>(0))?;
    let mut docs: Vec<Value> = Vec::new();
    for row in rows {
        let Ok(raw) = row else {
            continue;
        };
        let Ok(doc) = serde_json::from_str::<Value>(&raw) else {
            continue;
        };
        if doc.get("command_type").and_then(Value::as_str) != Some("browser.capture.extract")
            && doc.get("type").and_then(Value::as_str) != Some("browser.capture.extract")
        {
            continue;
        }
        if doc.get("status").and_then(Value::as_str) != Some("completed") {
            continue;
        }
        if !doc
            .pointer("/result/ok")
            .and_then(Value::as_bool)
            .unwrap_or(false)
        {
            continue;
        }
        docs.push(doc);
    }
    docs.sort_by_key(|doc| {
        Reverse(
            doc.get("updated_at_ms")
                .and_then(Value::as_u64)
                .unwrap_or(0),
        )
    });
    docs.truncate(64);

    let mut evidence: BTreeMap<FieldKey, Vec<Value>> = BTreeMap::new();
    let mut runs: Vec<Value> = Vec::new();
    let mut seen: BTreeSet<String> = BTreeSet::new();

    for doc in docs {
        let result = doc.get("result").unwrap_or(&Value::Null);
        let extract = result.get("extract").unwrap_or(&Value::Null);
        let payload = doc.get("payload").unwrap_or(&Value::Null);
        let Some(source_id) = first_json_string(&[extract, result, payload, &doc], "source_id")
            .or_else(|| first_json_string(&[extract, result], "sourceId"))
        else {
            continue;
        };
        let Some(plan) = plans.iter().find(|plan| {
            plan.source_id.eq_ignore_ascii_case(&source_id)
                || plan
                    .aliases
                    .iter()
                    .any(|alias| alias.eq_ignore_ascii_case(&source_id))
        }) else {
            continue;
        };
        if !browser_extract_is_current(&doc, result, extract, payload, plan, company) {
            runs.push(json!({
                "ok": false,
                "via": "browser_extract",
                "source_id": plan.source_id,
                "command_id": first_json_string(&[&doc], "command_id")
                    .or_else(|| first_json_string(&[&doc], "id"))
                    .unwrap_or_default(),
                "evidence_rejected": true,
            }));
            continue;
        }
        let Some(fields) = extract.get("fields").and_then(Value::as_object) else {
            runs.push(browser_extract_run_summary(
                &doc, result, extract, payload, plan, 0,
            ));
            continue;
        };
        if !browser_extract_matches_company(company, extract, payload, fields) {
            continue;
        }

        let mut added = 0_usize;
        for (field_name, raw_value) in fields {
            if forbidden_browser_extract_key(field_name) {
                continue;
            }
            let Some(field) = FieldKey::from_str(field_name) else {
                continue;
            };
            if !requested.is_empty() && !requested.contains(&field) {
                continue;
            }
            if !plan.target_fields.contains(&field) {
                continue;
            }
            let Some(value) = browser_extract_scalar(raw_value) else {
                continue;
            };
            let source_url = first_json_string(&[extract, payload], "url")
                .or_else(|| {
                    payload
                        .pointer("/browser_context_artifact/browser_context/url")
                        .and_then(Value::as_str)
                        .map(str::to_string)
                })
                .unwrap_or_default();
            let command_id = first_json_string(&[&doc], "command_id")
                .or_else(|| first_json_string(&[&doc], "id"))
                .unwrap_or_default();
            let dedupe = format!(
                "{}:{}:{}:{}",
                plan.source_id,
                field.as_str(),
                source_url,
                value
            );
            if !seen.insert(dedupe) {
                continue;
            }
            evidence.entry(field).or_default().push(json!({
                "value": value,
                "confidence": "high",
                "source_id": plan.source_id,
                "source_url": source_url,
                "tier": tier_label(plan.tier),
                "via": "browser_extract",
                "note": format!(
                    "capture_script={}; command_id={}",
                    first_json_string(&[result, payload], "capture_script").unwrap_or_default(),
                    command_id
                ),
            }));
            added += 1;
        }
        runs.push(browser_extract_run_summary(
            &doc, result, extract, payload, plan, added,
        ));
    }

    Ok((evidence, runs))
}

fn browser_extract_run_summary(
    doc: &Value,
    result: &Value,
    extract: &Value,
    payload: &Value,
    plan: &PersonResearchPlan,
    field_count: usize,
) -> Value {
    json!({
        "ok": true,
        "via": "browser_extract",
        "source_id": plan.source_id,
        "command_id": first_json_string(&[doc], "command_id").or_else(|| first_json_string(&[doc], "id")).unwrap_or_default(),
        "capture_script": first_json_string(&[result, payload], "capture_script").unwrap_or_default(),
        "url": first_json_string(&[extract, payload], "url").unwrap_or_default(),
        "field_count": field_count,
        "stream": "rxdb",
        "secret_value_in_payload": false,
        "frame_data_in_payload": false,
    })
}

fn browser_extract_is_current(
    doc: &Value,
    result: &Value,
    extract: &Value,
    payload: &Value,
    plan: &PersonResearchPlan,
    company: &str,
) -> bool {
    if result.get("ok").and_then(Value::as_bool) != Some(true) {
        return false;
    }
    if result
        .get("status")
        .and_then(Value::as_str)
        .is_some_and(|status| !matches!(status, "completed" | "succeeded" | "success" | "ok"))
    {
        return false;
    }
    if result
        .get("classification")
        .and_then(Value::as_str)
        .is_some_and(|status| status != "succeeded")
    {
        return false;
    }
    if result
        .get("secret_value_in_payload")
        .and_then(Value::as_bool)
        == Some(true)
        || result.get("frame_data_in_payload").and_then(Value::as_bool) == Some(true)
    {
        return false;
    }

    let source_ids = [
        first_json_string(&[extract], "sourceId"),
        first_json_string(&[extract], "source_id"),
        first_json_string(&[result], "source_id"),
        first_json_string(&[payload], "source_id"),
    ];
    if source_ids
        .iter()
        .flatten()
        .any(|source_id| !source_id_matches_plan(source_id, plan))
        || source_ids.iter().all(Option::is_none)
    {
        return false;
    }

    let Some(source_url) = first_json_string(&[extract, payload], "url").or_else(|| {
        payload
            .pointer("/browser_context_artifact/browser_context/url")
            .and_then(Value::as_str)
            .map(str::to_string)
    }) else {
        return false;
    };
    if !valid_http_url(&source_url)
        || !url_belongs_to_source(
            &source_url,
            plan.source_id,
            plan.aliases,
            plan.host_suffixes,
        )
    {
        return false;
    }
    for candidate_url in [
        first_json_string(&[extract], "url"),
        first_json_string(&[payload], "url"),
        context_value(payload).and_then(|context| first_json_string(&[context], "url")),
    ]
    .into_iter()
    .flatten()
    {
        if !valid_http_url(&candidate_url)
            || !url_belongs_to_source(
                &candidate_url,
                plan.source_id,
                plan.aliases,
                plan.host_suffixes,
            )
        {
            return false;
        }
    }

    let context = context_value(payload).unwrap_or(&Value::Null);
    let session_id = first_json_string(&[payload, context], "session_id");
    if session_id.as_deref().unwrap_or_default().is_empty() {
        return false;
    }
    if let (Some(record_id), Some(session_id)) = (
        doc.get("record_id").and_then(Value::as_str),
        session_id.as_deref(),
    ) {
        if record_id != session_id {
            return false;
        }
    }
    if let (Some(payload_session), Some(context_session)) = (
        payload.get("session_id").and_then(Value::as_str),
        context.get("session_id").and_then(Value::as_str),
    ) {
        if payload_session != context_session {
            return false;
        }
    }
    let frame_id = first_json_string(&[payload, context], "frame_id");
    if frame_id.as_deref().unwrap_or_default().is_empty() {
        return false;
    }
    if let (Some(payload_frame), Some(context_frame)) = (
        payload.get("frame_id").and_then(Value::as_str),
        context.get("frame_id").and_then(Value::as_str),
    ) {
        if payload_frame != context_frame {
            return false;
        }
    }
    if let (Some(command_id), Some(document_command_id)) = (
        payload.get("command_id").and_then(Value::as_str),
        doc.get("command_id")
            .or_else(|| doc.get("id"))
            .and_then(Value::as_str),
    ) {
        if command_id != document_command_id {
            return false;
        }
    }
    let captured_at = first_json_number(
        &[extract, result, payload, context],
        &["captured_at_ms", "frame_captured_at_ms"],
    );
    let updated_at = doc
        .get("updated_at_ms")
        .and_then(Value::as_u64)
        .or_else(|| first_json_number(&[result, payload], &["updated_at_ms"]));
    let expires_at = first_json_number(
        &[context, payload, result],
        &["expires_at_ms", "frame_expires_at_ms"],
    );
    let now = now_ms();
    if !captured_at.is_some_and(is_fresh_timestamp)
        || !updated_at.is_some_and(is_fresh_timestamp)
        || !expires_at.is_some_and(|timestamp| timestamp > now)
    {
        return false;
    }

    let Some(fields) = extract.get("fields").and_then(Value::as_object) else {
        return false;
    };
    browser_extract_matches_company(company, extract, payload, fields)
}

fn context_value(payload: &Value) -> Option<&Value> {
    payload.pointer("/browser_context_artifact/browser_context")
}

fn first_json_number(values: &[&Value], keys: &[&str]) -> Option<u64> {
    values.iter().find_map(|value| {
        keys.iter()
            .find_map(|key| value.get(*key).and_then(timestamp_ms))
    })
}

fn browser_extract_scalar(value: &Value) -> Option<String> {
    match value {
        Value::String(raw) => {
            let trimmed = raw.trim();
            if trimmed.is_empty() {
                None
            } else {
                Some(trimmed.chars().take(500).collect())
            }
        }
        Value::Number(number) => Some(number.to_string()),
        Value::Bool(flag) => Some(flag.to_string()),
        _ => None,
    }
}

fn browser_extract_matches_company(
    company: &str,
    extract: &Value,
    payload: &Value,
    fields: &serde_json::Map<String, Value>,
) -> bool {
    let tokens = company_match_tokens(company);
    if tokens.is_empty() {
        return false;
    }
    let mut identity_haystack = String::new();
    for value in [
        first_json_string(&[extract, payload], "url"),
        first_json_string(&[extract, payload], "title"),
        payload
            .pointer("/browser_context_artifact/browser_context/url")
            .and_then(Value::as_str)
            .map(str::to_string),
        payload
            .pointer("/browser_context_artifact/browser_context/title")
            .and_then(Value::as_str)
            .map(str::to_string),
    ]
    .into_iter()
    .flatten()
    {
        identity_haystack.push(' ');
        identity_haystack.push_str(&value.to_ascii_lowercase());
    }
    for (field_name, raw_value) in fields {
        if forbidden_browser_extract_key(field_name) {
            continue;
        }
        if !matches!(
            field_name.trim().to_ascii_lowercase().as_str(),
            "company" | "company_name" | "firma_name" | "company_identity"
        ) {
            continue;
        }
        if let Some(value) = browser_extract_scalar(raw_value) {
            identity_haystack.push(' ');
            identity_haystack.push_str(&value.to_ascii_lowercase());
        }
    }
    company_identity_matches(company, &identity_haystack)
}

fn company_identity_matches(company: &str, haystack: &str) -> bool {
    let tokens = company_match_tokens(company);
    if tokens.is_empty() {
        return false;
    }
    let actual_tokens: BTreeSet<String> = haystack
        .split(|ch: char| !ch.is_ascii_alphanumeric())
        .map(str::to_ascii_lowercase)
        .filter(|token| !token.is_empty())
        .collect();
    tokens.iter().all(|token| actual_tokens.contains(token))
}

fn company_match_tokens(company: &str) -> Vec<String> {
    let legal_suffixes = [
        "ag", "at", "ch", "co", "de", "gmbh", "kg", "mbh", "se", "the", "und",
    ];
    company
        .split(|ch: char| !ch.is_ascii_alphanumeric())
        .map(str::trim)
        .filter(|token| token.len() >= 3)
        .map(str::to_ascii_lowercase)
        .filter(|token| !legal_suffixes.contains(&token.as_str()))
        .collect()
}

fn first_json_string(values: &[&Value], key: &str) -> Option<String> {
    values.iter().find_map(|value| {
        value
            .get(key)
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_string)
    })
}

fn quote_sql_identifier(identifier: &str) -> String {
    format!("\"{}\"", identifier.replace('"', "\"\""))
}

fn forbidden_browser_extract_key(key: &str) -> bool {
    matches!(
        key.trim().to_ascii_lowercase().as_str(),
        "secret"
            | "secret_value"
            | "password"
            | "token"
            | "access_token"
            | "refresh_token"
            | "cookie"
            | "cookies"
            | "frame"
            | "frame_data"
            | "data"
            | "raw"
            | "raw_html"
            | "html"
    )
}

// ---------------------------------------------------------------------------
// Helpers shared with deep_research
// ---------------------------------------------------------------------------

fn normalize_required_company(raw: &str) -> Result<String> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        anyhow::bail!("ctox_person_research requires a non-empty --company");
    }
    Ok(trimmed.to_string())
}

fn empty_plan_response(company: &str, request: &PersonResearchRequest, reason: &str) -> Value {
    json!({
        "ok": true,
        "tool": "ctox_person_research",
        "company": company,
        "country": request.country.as_iso(),
        "mode": request.mode.as_str(),
        "fields": Value::Object(Default::default()),
        "plan": [],
        "search_runs": [],
        "read_runs": [],
        "scrape_runs": [],
        "browser_extract_runs": [],
        "browser_assist_tasks": [],
        "skipped_reason": reason,
    })
}

fn tier_label(tier: Tier) -> &'static str {
    match tier {
        Tier::P => "P",
        Tier::S => "S",
        Tier::C => "C",
    }
}

fn url_belongs_to_source(url: &str, id: &str, aliases: &[&str], host_suffixes: &[&str]) -> bool {
    let Some(host) = url::Url::parse(url)
        .ok()
        .and_then(|u| u.host_str().map(str::to_owned))
    else {
        return false;
    };
    let host = host
        .trim_start_matches("www.")
        .trim_start_matches("app.")
        .trim_start_matches("api.")
        .to_ascii_lowercase();
    if host == id.to_ascii_lowercase() {
        return true;
    }
    if host.ends_with(&format!(".{}", id.to_ascii_lowercase())) {
        return true;
    }
    for alias in aliases {
        let needle = alias.to_ascii_lowercase();
        if host == needle || host.ends_with(&format!(".{needle}")) {
            return true;
        }
    }
    for suffix in host_suffixes {
        let needle = suffix.to_ascii_lowercase();
        if host == needle || host.ends_with(&format!(".{needle}")) {
            return true;
        }
    }
    false
}

/// Convert the raw `results` JSON returned by `run_ctox_web_search_tool`
/// into typed [`SourceHit`]s for snippet-mining sources.
fn parse_hits(raw: &[Value]) -> Vec<SourceHit> {
    raw.iter()
        .filter_map(|entry| {
            let url = entry.get("url").and_then(Value::as_str)?.to_string();
            let title = entry
                .get("title")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string();
            let snippet = entry
                .get("snippet")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string();
            if url.trim().is_empty() {
                None
            } else {
                Some(SourceHit {
                    title,
                    url,
                    snippet,
                })
            }
        })
        .collect()
}

fn parse_verified_hits(raw: &[Value], plan: &PersonResearchPlan, company: &str) -> Vec<SourceHit> {
    let eligible: Vec<Value> = raw
        .iter()
        .filter(|hit| search_hit_evidence_eligible(hit, plan, company))
        .cloned()
        .collect();
    parse_hits(&eligible)
}

fn search_hit_evidence_eligible(hit: &Value, plan: &PersonResearchPlan, company: &str) -> bool {
    if !evidence_gate_is_current(hit) {
        return false;
    }
    let Some(url) = hit.get("url").and_then(Value::as_str) else {
        return false;
    };
    let Some(canonical_url) = hit.get("canonical_url").and_then(Value::as_str) else {
        return false;
    };
    valid_http_url(url)
        && valid_http_url(canonical_url)
        && url_belongs_to_source(
            canonical_url,
            plan.source_id,
            plan.aliases,
            plan.host_suffixes,
        )
        && url_belongs_to_source(url, plan.source_id, plan.aliases, plan.host_suffixes)
        && company_identity_matches(
            company,
            &format!(
                "{} {}",
                hit.get("title").and_then(Value::as_str).unwrap_or_default(),
                hit.get("snippet")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
            ),
        )
}

fn web_read_evidence_eligible(
    payload: &Value,
    plan: &PersonResearchPlan,
    company: &str,
    requested_url: &str,
) -> bool {
    if !evidence_gate_is_current(payload) {
        return false;
    }
    let Some(canonical_url) = payload.get("canonical_url").and_then(Value::as_str) else {
        return false;
    };
    if !valid_http_url(requested_url)
        || !valid_http_url(canonical_url)
        || !url_belongs_to_source(
            requested_url,
            plan.source_id,
            plan.aliases,
            plan.host_suffixes,
        )
        || !url_belongs_to_source(
            canonical_url,
            plan.source_id,
            plan.aliases,
            plan.host_suffixes,
        )
    {
        return false;
    }
    let Some(extracted) = payload.get("extracted_fields") else {
        return false;
    };
    if let Some(source_id) = extracted.get("source_id").and_then(Value::as_str) {
        if !source_id_matches_plan(source_id, plan) {
            return false;
        }
    }
    company_identity_matches(
        company,
        &format!(
            "{} {} {} {}",
            payload
                .get("title")
                .and_then(Value::as_str)
                .unwrap_or_default(),
            payload
                .get("summary")
                .and_then(Value::as_str)
                .unwrap_or_default(),
            payload
                .get("page_text_excerpt")
                .and_then(Value::as_str)
                .unwrap_or_default(),
            canonical_url,
        ),
    )
}

fn entry_company_identity_matches(payload: &Value, entry: &Value, company: &str) -> bool {
    let explicit = ["company", "company_name", "firma_name", "company_identity"]
        .iter()
        .find_map(|key| entry.get(*key).and_then(Value::as_str))
        .or_else(|| {
            ["title", "summary", "page_text_excerpt"]
                .iter()
                .find_map(|key| payload.get(*key).and_then(Value::as_str))
        });
    let Some(explicit) = explicit else {
        return false;
    };
    company_identity_matches(company, explicit)
}

fn evidence_gate_is_current(value: &Value) -> bool {
    let gate = value
        .get("evidence_gate")
        .or_else(|| value.get("evidence"))
        .unwrap_or(value);
    gate.get("evidence_eligible").and_then(Value::as_bool) == Some(true)
        && gate.get("verification_status").and_then(Value::as_str) == Some("verified")
        && gate
            .get("http_status")
            .and_then(Value::as_u64)
            .is_some_and(|status| (200..300).contains(&status))
        && gate
            .get("snapshot_hash")
            .and_then(Value::as_str)
            .is_some_and(|hash| !hash.trim().is_empty())
        && gate.get("fresh").and_then(Value::as_bool) != Some(false)
        && gate
            .get("checked_at")
            .or_else(|| gate.get("checked_at_ms"))
            .and_then(timestamp_ms)
            .is_some_and(is_fresh_timestamp)
}

fn source_id_matches_plan(source_id: &str, plan: &PersonResearchPlan) -> bool {
    source_id.eq_ignore_ascii_case(plan.source_id)
        || plan
            .aliases
            .iter()
            .any(|alias| source_id.eq_ignore_ascii_case(alias))
}

fn valid_http_url(raw: &str) -> bool {
    let Ok(url) = url::Url::parse(raw.trim()) else {
        return false;
    };
    matches!(url.scheme(), "http" | "https") && url.host_str().is_some()
}

fn timestamp_ms(value: &Value) -> Option<u64> {
    let raw = value
        .as_u64()
        .or_else(|| {
            value
                .as_i64()
                .filter(|value| *value >= 0)
                .map(|value| value as u64)
        })
        .or_else(|| value.as_str()?.trim().parse::<u64>().ok());
    if let Some(raw) = raw {
        return Some(if raw < 10_000_000_000 {
            raw * 1_000
        } else {
            raw
        });
    }
    value
        .as_str()
        .and_then(|text| DateTime::parse_from_rfc3339(text.trim()).ok())
        .and_then(|date| u64::try_from(date.timestamp_millis()).ok())
}

fn is_fresh_timestamp(timestamp: u64) -> bool {
    let now = now_ms();
    timestamp <= now.saturating_add(MAX_CLOCK_SKEW_MS)
        && now.saturating_sub(timestamp) <= MAX_EVIDENCE_AGE_MS
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

fn find_terms_for_fields(fields: &[FieldKey]) -> Vec<String> {
    let mut out: BTreeSet<String> = BTreeSet::new();
    for field in fields {
        match field {
            FieldKey::Umsatz => {
                out.insert("Umsatz".into());
                out.insert("Umsatzerlöse".into());
            }
            FieldKey::Mitarbeiter => {
                out.insert("Mitarbeiter".into());
                out.insert("Arbeitnehmer".into());
            }
            FieldKey::FirmaEmail | FieldKey::PersonEmail | FieldKey::PersonEmailValidation => {
                out.insert("E-Mail".into());
                out.insert("Email".into());
            }
            FieldKey::FirmaAnschrift | FieldKey::FirmaPlz | FieldKey::FirmaOrt => {
                out.insert("Anschrift".into());
                out.insert("Sitz".into());
            }
            FieldKey::PersonFunktion | FieldKey::PersonPosition => {
                out.insert("Geschäftsführer".into());
                out.insert("Vorstand".into());
            }
            FieldKey::WzCode => {
                out.insert("WZ".into());
                out.insert("NACE".into());
            }
            _ => {}
        }
    }
    out.into_iter().collect()
}

// ---------------------------------------------------------------------------
// Workspace persistence
// ---------------------------------------------------------------------------

fn default_person_workspace(root: &Path, company: &str) -> PathBuf {
    let ts = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_else(|_| Duration::from_secs(0))
        .as_secs();
    let base = root
        .join("runtime/research/person")
        .join(format!("{ts}-{slug}", slug = slugify(company)));
    unique_workspace_dir(base)
}

/// Return `base` if free, else `base-2`, `base-3`, ... so two same-second runs
/// for the same company (or a broken clock pinned to `0`) don't clobber each
/// other's workspace.
fn unique_workspace_dir(base: PathBuf) -> PathBuf {
    if !base.exists() {
        return base;
    }
    let Some(parent) = base.parent().map(Path::to_path_buf) else {
        return base;
    };
    let name = base
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("person")
        .to_string();
    for suffix in 2..10_000 {
        let candidate = parent.join(format!("{name}-{suffix}"));
        if !candidate.exists() {
            return candidate;
        }
    }
    base
}

fn persist_person_workspace(
    workspace: &Path,
    request: &PersonResearchRequest,
    payload: &Value,
) -> Result<Value> {
    fs::create_dir_all(workspace).with_context(|| {
        format!(
            "failed to create person-research workspace {}",
            workspace.display()
        )
    })?;

    write_json_pretty(
        &workspace.join("fields.json"),
        payload.get("fields").unwrap_or(&Value::Null),
    )?;
    write_json_pretty(
        &workspace.join("plan.json"),
        payload.get("plan").unwrap_or(&Value::Null),
    )?;
    write_json_pretty(&workspace.join("search_runs.jsonl"), &Value::Null)?; // placeholder
    write_jsonl(
        &workspace.join("search_runs.jsonl"),
        payload.get("search_runs"),
    )?;
    write_jsonl(&workspace.join("read_runs.jsonl"), payload.get("read_runs"))?;
    write_jsonl(
        &workspace.join("scrape_runs.jsonl"),
        payload.get("scrape_runs"),
    )?;
    write_jsonl(
        &workspace.join("browser_extract_runs.jsonl"),
        payload.get("browser_extract_runs"),
    )?;
    write_jsonl(
        &workspace.join("browser_assist_tasks.jsonl"),
        payload.get("browser_assist_tasks"),
    )?;
    write_json_pretty(&workspace.join("envelope.json"), payload)?;

    fs::write(
        workspace.join("CONTINUE.md"),
        continuation_markdown(request, payload),
    )?;

    let manifest = json!({
        "workspace": workspace,
        "company": payload.get("company").cloned().unwrap_or(Value::Null),
        "country": payload.get("country").cloned().unwrap_or(Value::Null),
        "mode": payload.get("mode").cloned().unwrap_or(Value::Null),
        "files": {
            "fields": "fields.json",
            "plan": "plan.json",
            "search_runs": "search_runs.jsonl",
            "read_runs": "read_runs.jsonl",
            "browser_extract_runs": "browser_extract_runs.jsonl",
            "browser_assist_tasks": "browser_assist_tasks.jsonl",
            "envelope": "envelope.json",
            "continuation": "CONTINUE.md",
        }
    });
    write_json_pretty(&workspace.join("manifest.json"), &manifest)?;

    Ok(json!({
        "path": workspace,
        "manifest": workspace.join("manifest.json"),
        "continuation": workspace.join("CONTINUE.md"),
        "fields": workspace.join("fields.json"),
    }))
}

fn continuation_markdown(request: &PersonResearchRequest, payload: &Value) -> String {
    let mut out = String::new();
    out.push_str("# Person-Research continuation\n\n");
    out.push_str(&format!(
        "- company: {}\n",
        payload
            .get("company")
            .and_then(Value::as_str)
            .unwrap_or("?")
    ));
    out.push_str(&format!("- country: {}\n", request.country.as_iso()));
    out.push_str(&format!("- mode: {}\n", request.mode.as_str()));
    if let Some(plan) = payload.get("plan").and_then(Value::as_array) {
        out.push_str(&format!("- plan entries: {}\n", plan.len()));
    }
    if let Some(fields) = payload.get("fields").and_then(Value::as_object) {
        let missing = fields
            .iter()
            .filter(|(_, v)| v.get("confidence").and_then(Value::as_str) == Some("missing"))
            .count();
        out.push_str(&format!("- missing fields: {missing}\n"));
    }
    out.push_str("\nRerun with `ctox web person-research` to resume.\n");
    out
}

fn write_json_pretty(path: &Path, value: &Value) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("failed to create {}", parent.display()))?;
    }
    fs::write(path, serde_json::to_vec_pretty(value)?)
        .with_context(|| format!("failed to write {}", path.display()))
}

fn write_jsonl(path: &Path, value: Option<&Value>) -> Result<()> {
    let mut file =
        fs::File::create(path).with_context(|| format!("failed to create {}", path.display()))?;
    if let Some(Value::Array(items)) = value {
        for item in items {
            writeln!(file, "{}", serde_json::to_string(item)?)?;
        }
    }
    Ok(())
}

fn slugify(raw: &str) -> String {
    let mut out = String::with_capacity(raw.len());
    let mut last_dash = true;
    for ch in raw.chars() {
        if ch.is_ascii_alphanumeric() {
            out.push(ch.to_ascii_lowercase());
            last_dash = false;
        } else if !last_dash {
            out.push('-');
            last_dash = true;
        }
    }
    out.trim_matches('-').to_string()
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plan_excludes_tier_c_unless_opted_in() {
        let request = PersonResearchRequest {
            company: "ACME".into(),
            country: Country::De,
            mode: ResearchMode::NewRecord,
            fields: vec![FieldKey::PersonFunktion],
            include_private: Vec::new(),
            workspace: None,
            persist_workspace: false,
        };
        let plans = build_person_research_plan(&request);
        // PersonFunktion is autoritative_for LinkedIn (Tier C), XING (Tier C),
        // Northdata (Tier S), Zefix (Tier P, CH-only so excluded here),
        // DnB Hoovers (Tier C). Without opt-in only the non-C sources should
        // appear.
        for plan in &plans {
            assert!(
                plan.tier != Tier::C,
                "Tier C must not appear without opt-in (got {})",
                plan.source_id
            );
        }
    }

    #[test]
    fn plan_excludes_person_discovery_without_opt_in() {
        // person-discovery (Tier S, people-scraping) must NOT run implicitly on
        // a plain company lookup — it harvests GDPR personal data.
        let request = PersonResearchRequest {
            company: "ACME".into(),
            country: Country::De,
            mode: ResearchMode::NewRecord,
            fields: vec![FieldKey::PersonVorname],
            include_private: Vec::new(),
            workspace: None,
            persist_workspace: false,
        };
        let plans = build_person_research_plan(&request);
        assert!(
            !plans.iter().any(|p| p.source_id == "person-discovery"),
            "person-discovery must be gated behind an explicit opt-in"
        );

        // With an explicit opt-in it appears again.
        let opted = PersonResearchRequest {
            include_private: vec!["person-discovery".into()],
            ..request
        };
        let plans = build_person_research_plan(&opted);
        assert!(
            plans.iter().any(|p| p.source_id == "person-discovery"),
            "person-discovery must run when explicitly opted in"
        );
    }

    #[test]
    fn plan_includes_tier_c_when_opted_in_by_id() {
        let request = PersonResearchRequest {
            company: "ACME".into(),
            country: Country::De,
            mode: ResearchMode::NewRecord,
            fields: vec![FieldKey::PersonFunktion],
            include_private: vec!["linkedin.com".into()],
            workspace: None,
            persist_workspace: false,
        };
        let plans = build_person_research_plan(&request);
        assert!(plans.iter().any(|p| p.source_id == "linkedin.com"));
    }

    #[test]
    fn plan_includes_tier_c_when_opted_in_by_alias() {
        let request = PersonResearchRequest {
            company: "ACME".into(),
            country: Country::De,
            mode: ResearchMode::NewRecord,
            fields: vec![FieldKey::PersonFunktion],
            include_private: vec!["linkedin".into()],
            workspace: None,
            persist_workspace: false,
        };
        let plans = build_person_research_plan(&request);
        assert!(plans.iter().any(|p| p.source_id == "linkedin.com"));
    }

    #[test]
    fn plan_sorted_tier_p_first() {
        let request = PersonResearchRequest {
            company: "ACME".into(),
            country: Country::Ch,
            mode: ResearchMode::NewRecord,
            fields: vec![
                FieldKey::FirmaName,
                FieldKey::FirmaAnschrift,
                FieldKey::PersonVorname,
            ],
            include_private: vec!["linkedin".into(), "dnb".into()],
            workspace: None,
            persist_workspace: false,
        };
        let plans = build_person_research_plan(&request);
        let tiers: Vec<_> = plans.iter().map(|p| p.tier).collect();
        let mut sorted = tiers.clone();
        sorted.sort();
        assert_eq!(tiers, sorted, "tier order must be P → S → C");
        assert!(
            plans.iter().any(|p| p.source_id == "zefix.ch"),
            "Zefix must be present for CH"
        );
    }

    #[test]
    fn have_data_returns_empty_plan_response() {
        let request = PersonResearchRequest {
            company: "ACME".into(),
            country: Country::De,
            mode: ResearchMode::HaveData,
            fields: vec![FieldKey::FirmaName],
            include_private: Vec::new(),
            workspace: None,
            persist_workspace: false,
        };
        // We don't have an actual CTOX root here; the mode-check should
        // short-circuit before any IO happens.
        let payload = run_ctox_person_research_tool(Path::new("/nonexistent"), &request).unwrap();
        assert_eq!(payload["skipped_reason"], "mode_skipped");
        assert!(payload["plan"].as_array().unwrap().is_empty());
    }

    #[test]
    fn aggregate_picks_highest_confidence() {
        let mut raw: BTreeMap<FieldKey, Vec<Value>> = BTreeMap::new();
        raw.insert(
            FieldKey::Umsatz,
            vec![
                json!({"value": "440000000", "confidence": "high", "source_id": "bundesanzeiger.de"}),
                json!({"value": "445000000", "confidence": "medium", "source_id": "northdata.de"}),
            ],
        );
        let agg = aggregate_fields(&[FieldKey::Umsatz], &[], raw);
        let umsatz = agg.get("umsatz").unwrap();
        assert_eq!(umsatz["value"], "440000000");
        assert_eq!(umsatz["confidence"], "high");
        assert_eq!(umsatz["candidates"].as_array().unwrap().len(), 2);
    }

    #[test]
    fn url_belongs_to_source_handles_subdomains_and_prefixes() {
        assert!(url_belongs_to_source(
            "https://www.northdata.de/Foo+SE,Berlin",
            "northdata.de",
            &["northdata"],
            &[],
        ));
        assert!(url_belongs_to_source(
            "https://app.dnbhoovers.com/data/duns/123",
            "dnbhoovers.com",
            &["dnb"],
            &[],
        ));
        assert!(!url_belongs_to_source(
            "https://example.com/page",
            "northdata.de",
            &["northdata"],
            &[],
        ));
    }

    #[test]
    fn url_belongs_to_source_resolves_via_host_suffixes() {
        // Zefix: id=zefix.ch, but hit URLs come back from zefix.admin.ch.
        assert!(url_belongs_to_source(
            "https://www.zefix.admin.ch/de/search/entity/list/firm/154673",
            "zefix.ch",
            &["zefix"],
            &["zefix.admin.ch"],
        ));
        // D&B Direct+: id=dnbhoovers.com, API host is plus.dnb.com.
        assert!(url_belongs_to_source(
            "https://plus.dnb.com/data/duns/316840271",
            "dnbhoovers.com",
            &["dnb"],
            &["plus.dnb.com"],
        ));
        // LinkedIn API host.
        assert!(url_belongs_to_source(
            "https://api.linkedin.com/v2/people/(id:urn:li:person:123)",
            "linkedin.com",
            &["linkedin"],
            &["api.linkedin.com"],
        ));
    }

    #[test]
    fn browser_assist_recommendations_expose_recipe_without_secret_value() {
        let request = PersonResearchRequest {
            company: "ACME".into(),
            country: Country::De,
            mode: ResearchMode::NewRecord,
            fields: vec![FieldKey::PersonLinkedin],
            include_private: vec!["linkedin".into()],
            workspace: None,
            persist_workspace: false,
        };
        let plans = build_person_research_plan(&request);
        let recommendations = browser_assist_recommendations(&plans);
        let linkedin = recommendations
            .iter()
            .find(|entry| entry.get("source_id").and_then(Value::as_str) == Some("linkedin.com"))
            .expect("linkedin recommendation");
        assert_eq!(linkedin.get("stream").and_then(Value::as_str), Some("rxdb"));
        assert_eq!(
            linkedin.get("required_secret_name").and_then(Value::as_str),
            Some("LINKEDIN_SALES_NAV_TOKEN")
        );
        assert_eq!(
            linkedin
                .get("secret_value_in_payload")
                .and_then(Value::as_bool),
            Some(false)
        );
        assert!(linkedin
            .get("credential_selector")
            .and_then(Value::as_str)
            .is_some());
        assert!(linkedin
            .get("capture_script")
            .and_then(Value::as_str)
            .is_some());
        assert!(!serde_json::to_string(linkedin)
            .expect("serialize recommendation")
            .contains("secret_value\""));
    }

    #[test]
    fn browser_assist_tasks_materialize_from_source_failures() {
        let request = PersonResearchRequest {
            company: "ACME".into(),
            country: Country::De,
            mode: ResearchMode::NewRecord,
            fields: vec![FieldKey::PersonLinkedin],
            include_private: vec!["linkedin".into()],
            workspace: None,
            persist_workspace: false,
        };
        let plans = build_person_research_plan(&request);
        let plan = plans
            .iter()
            .find(|plan| plan.source_id == "linkedin.com")
            .expect("linkedin plan");
        let tasks = browser_assist_tasks_from_source_failures(
            plan,
            Some(&json!([{
                "requested_source": "linkedin.com",
                "source_id": "linkedin.com",
                "kind": "credential_missing",
                "error": "credential_missing: LINKEDIN_SALES_NAV_TOKEN",
                "secret_name": "LINKEDIN_SALES_NAV_TOKEN",
                "browser_assist": {
                    "stream": "rxdb",
                    "target_url": "https://www.linkedin.com/login",
                    "required_secret_name": "LINKEDIN_SALES_NAV_TOKEN",
                    "credential_selector": "input[name=\"session_password\"]",
                    "capture_script": "linkedin.profile_capture.v1",
                    "secret_value_in_payload": false,
                    "frame_data_in_payload": false
                }
            }])),
        );
        assert_eq!(tasks.len(), 1);
        assert_eq!(tasks[0]["status"], "auth_assist_required");
        assert_eq!(tasks[0]["stream"], "rxdb");
        assert_eq!(
            tasks[0]["next_command"],
            "ctox business-os web-stack auth-assist-request --source-id linkedin.com"
        );
        let serialized = serde_json::to_string(&tasks).unwrap();
        assert!(!serialized.contains("secret_value\":\""));
        assert!(serialized.contains("LINKEDIN_SALES_NAV_TOKEN"));
    }

    #[test]
    fn browser_extract_evidence_imports_only_typed_redacted_fields() {
        let root = std::env::temp_dir().join(format!(
            "ctox-web-stack-browser-extract-{}",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let runtime = root.join("runtime");
        fs::create_dir_all(&runtime).unwrap();
        let db_path = runtime.join("ctox.sqlite3");
        let conn = Connection::open(&db_path).unwrap();
        conn.execute(
            "CREATE TABLE ctox_business_os__business_commands__v1 (id TEXT PRIMARY KEY, data TEXT NOT NULL)",
            [],
        )
        .unwrap();
        let doc = json!({
            "id": "browser_extract_test",
            "command_id": "browser_extract_test",
            "command_type": "browser.capture.extract",
            "type": "browser.capture.extract",
            "status": "completed",
            "updated_at_ms": 42_u64,
            "payload": {
                "source_id": "linkedin.com",
                "capture_script": "linkedin.profile_capture.v1",
                "secret_value": "bad-secret",
                    "browser_context_artifact": {
                    "browser_context": {
                        "session_id": "browser-session-test",
                        "frame_id": "frame-test",
                        "frame_captured_at_ms": now_ms(),
                        "frame_expires_at_ms": now_ms() + 60_000,
                        "url": "https://www.linkedin.com/in/alice",
                        "title": "Alice Example - CEO - ACME | LinkedIn",
                        "frame_data": "bad-frame"
                    }
                }
            },
            "result": {
                "ok": true,
                "stream": "rxdb",
                "secret_value_in_payload": false,
                "frame_data_in_payload": false,
                "capture_script": "linkedin.profile_capture.v1",
                "extract": {
                    "sourceId": "linkedin.com",
                    "url": "https://www.linkedin.com/in/alice",
                    "fields": {
                        "person_linkedin": "https://www.linkedin.com/in/alice",
                        "person_funktion": "CEO",
                        "secret_value": "bad-secret",
                        "data": "bad-frame"
                    }
                }
            }
        });
        let mut doc = doc;
        doc["updated_at_ms"] = json!(now_ms());
        conn.execute(
            "INSERT INTO ctox_business_os__business_commands__v1 (id, data) VALUES (?1, ?2)",
            (
                &"browser_extract_test",
                &serde_json::to_string(&doc).unwrap(),
            ),
        )
        .unwrap();

        let request = PersonResearchRequest {
            company: "ACME".into(),
            country: Country::De,
            mode: ResearchMode::NewRecord,
            fields: vec![FieldKey::PersonLinkedin, FieldKey::PersonFunktion],
            include_private: vec!["linkedin".into()],
            workspace: None,
            persist_workspace: false,
        };
        let plans = build_person_research_plan(&request);
        let (evidence, runs) =
            collect_browser_extract_evidence(&root, "ACME", &plans, &request.fields).unwrap();

        assert_eq!(runs.len(), 1);
        assert_eq!(runs[0]["field_count"], 2);
        assert_eq!(
            evidence[&FieldKey::PersonLinkedin][0]["value"],
            "https://www.linkedin.com/in/alice"
        );
        assert_eq!(evidence[&FieldKey::PersonFunktion][0]["value"], "CEO");
        let serialized = format!("{evidence:?}{runs:?}");
        assert!(!serialized.contains("bad-secret"));
        assert!(!serialized.contains("bad-frame"));

        let plan = build_person_research_plan(&request)
            .into_iter()
            .find(|plan| plan.source_id == "linkedin.com")
            .expect("linkedin plan");
        let result = doc.get("result").unwrap();
        let extract = result.get("extract").unwrap();
        let payload = doc.get("payload").unwrap();
        assert!(browser_extract_is_current(
            &doc, result, extract, payload, &plan, "ACME"
        ));
        let mut stale = doc.clone();
        stale["updated_at_ms"] = json!(1_u64);
        assert!(!browser_extract_is_current(
            &stale,
            stale.get("result").unwrap(),
            stale["result"].get("extract").unwrap(),
            stale.get("payload").unwrap(),
            &plan,
            "ACME"
        ));
        let mut mixed_frame = doc.clone();
        mixed_frame["payload"]["frame_id"] = json!("different-frame");
        assert!(!browser_extract_is_current(
            &mixed_frame,
            mixed_frame.get("result").unwrap(),
            mixed_frame["result"].get("extract").unwrap(),
            mixed_frame.get("payload").unwrap(),
            &plan,
            "ACME"
        ));

        drop(conn);
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn browser_extract_evidence_skips_wrong_company_scope() {
        let fields = serde_json::Map::from_iter([
            (
                "person_funktion".to_string(),
                Value::String("CEO at Globex".to_string()),
            ),
            (
                "person_linkedin".to_string(),
                Value::String("https://www.linkedin.com/in/alice".to_string()),
            ),
        ]);
        assert!(!browser_extract_matches_company(
            "ACME GmbH",
            &json!({"url": "https://www.linkedin.com/in/alice"}),
            &json!({
                "browser_context_artifact": {
                    "browser_context": {
                        "title": "Alice Example - CEO - Globex | LinkedIn"
                    }
                }
            }),
            &fields,
        ));
        assert!(browser_extract_matches_company(
            "ACME GmbH",
            &json!({"url": "https://www.linkedin.com/in/alice"}),
            &json!({
                "browser_context_artifact": {
                    "browser_context": {
                        "title": "Alice Example - CEO - ACME | LinkedIn"
                    }
                }
            }),
            &fields,
        ));
    }

    #[test]
    fn search_evidence_rejects_stale_wrong_source_and_wrong_company() {
        let request = PersonResearchRequest {
            company: "ACME GmbH".into(),
            country: Country::De,
            mode: ResearchMode::NewRecord,
            fields: vec![FieldKey::FirmaName],
            include_private: Vec::new(),
            workspace: None,
            persist_workspace: false,
        };
        let plan = build_person_research_plan(&request)
            .into_iter()
            .find(|plan| plan.source_id == "northdata.de")
            .expect("northdata plan");
        let gate = json!({
            "evidence_eligible": true,
            "verification_status": "verified",
            "http_status": 200,
            "checked_at": now_ms(),
            "snapshot_hash": "sha256:test"
        });
        let base = json!({
            "url": "https://www.northdata.de/ACME",
            "canonical_url": "https://www.northdata.de/ACME",
            "title": "ACME company profile",
            "snippet": "ACME company register",
            "evidence_gate": gate
        });
        assert!(search_hit_evidence_eligible(&base, &plan, "ACME GmbH"));

        let mut stale = base.clone();
        stale["evidence_gate"]["checked_at"] = json!(1_u64);
        assert!(!search_hit_evidence_eligible(&stale, &plan, "ACME GmbH"));

        let mut wrong_source = base.clone();
        wrong_source["canonical_url"] = json!("https://evil.example/ACME");
        assert!(!search_hit_evidence_eligible(
            &wrong_source,
            &plan,
            "ACME GmbH"
        ));

        let mut wrong_company = base;
        wrong_company["title"] = json!("Globex company profile");
        wrong_company["snippet"] = json!("Globex company register");
        assert!(!search_hit_evidence_eligible(
            &wrong_company,
            &plan,
            "ACME GmbH"
        ));
    }

    #[test]
    fn slugify_handles_typical_company_names() {
        assert_eq!(slugify("WITTENSTEIN SE"), "wittenstein-se");
        assert_eq!(slugify("DO & Co. AG"), "do-co-ag");
        assert_eq!(slugify("  Foo   Bar  "), "foo-bar");
    }
}
