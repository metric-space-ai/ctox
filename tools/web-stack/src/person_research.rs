//! `ctox web person-research` — high-level orchestrator for company / person
//! recherche.
//!
//! Structural sibling of [`crate::deep_research`]. Where deep-research
//! aggregates evidence across scholarly/web/data sources for free-form
//! topical queries, `person-research` aggregates **typed field evidence**
//! for one company at a time, using the registered source modules
//! ([`crate::sources`]) and the Thesen Nachrecherche source matrix
//! ([`crate::sources::EXCEL_MATRIX`]) as the routing oracle.
//!
//! The orchestrator never talks to a search engine directly: it builds
//! [`PersonResearchPlan`]s and drives the existing
//! [`run_ctox_web_search_tool`] / [`run_ctox_web_read_tool`] primitives
//! with `pinned_sources` set per plan. Per-source `extract_fields` runs
//! automatically inside `run_ctox_web_read_tool` (Phase 3), so this file
//! only has to aggregate the resulting evidence per field.

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
        return Ok(empty_plan_response(&company, request, "no_sources_for_request"));
    }

    let mut field_evidence: BTreeMap<FieldKey, Vec<Value>> = BTreeMap::new();
    let mut search_runs: Vec<Value> = Vec::with_capacity(plans.len());
    let mut read_runs: Vec<Value> = Vec::new();
    let mut scrape_runs: Vec<Value> = Vec::new();
    let mut visited_urls: BTreeSet<String> = BTreeSet::new();
    let ctox_bin = scrape_bridge::default_ctox_bin();

    for plan in &plans {
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
        }));
        if !ok {
            continue;
        }

        // Snippet-mining sources (e.g. `person-discovery`) extract typed
        // evidence directly from the search-result hits without needing to
        // fetch the gated profile pages behind them. If `extract_from_hits`
        // returns evidence, we use it and skip the per-hit read loop.
        if let Some(module) = sources::find(plan.source_id) {
            let hit_objs = parse_hits(&hits);
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
                    country: Some(request.country.as_iso().to_string()),
                },
            );
            match read_payload {
                Ok(payload) => {
                    if let Some(fields) = payload
                        .get("extracted_fields")
                        .and_then(|v| v.get("fields"))
                        .and_then(Value::as_array)
                    {
                        for entry in fields {
                            let Some(field_str) = entry.get("field").and_then(Value::as_str)
                            else {
                                continue;
                            };
                            let Some(field) = FieldKey::from_str(field_str) else {
                                continue;
                            };
                            if !request.fields.is_empty() && !request.fields.contains(&field) {
                                continue;
                            }
                            let mut tagged = entry.clone();
                            tagged["source_id"] = Value::String(plan.source_id.to_string());
                            tagged["tier"] = Value::String(tier_label(plan.tier).to_string());
                            tagged["hit_url"] = Value::String(url.to_string());
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
            if module.tier() == Tier::C && !is_tier_c_opt_in(module, &request.include_private) {
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

fn is_tier_c_opt_in(module: &'static dyn SourceModule, include_private: &[String]) -> bool {
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
            confidence_rank(b.get("confidence").and_then(Value::as_str))
                .cmp(&confidence_rank(a.get("confidence").and_then(Value::as_str)))
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

fn url_belongs_to_source(
    url: &str,
    id: &str,
    aliases: &[&str],
    host_suffixes: &[&str],
) -> bool {
    let Some(host) = url::Url::parse(url).ok().and_then(|u| u.host_str().map(str::to_owned))
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
                Some(SourceHit { title, url, snippet })
            }
        })
        .collect()
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
            FieldKey::FirmaEmail | FieldKey::PersonEmail => {
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
    root.join("runtime/research/person").join(format!(
        "{ts}-{slug}",
        slug = slugify(company)
    ))
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

    write_json_pretty(&workspace.join("fields.json"), payload.get("fields").unwrap_or(&Value::Null))?;
    write_json_pretty(&workspace.join("plan.json"), payload.get("plan").unwrap_or(&Value::Null))?;
    write_json_pretty(&workspace.join("search_runs.jsonl"), &Value::Null)?; // placeholder
    write_jsonl(&workspace.join("search_runs.jsonl"), payload.get("search_runs"))?;
    write_jsonl(&workspace.join("read_runs.jsonl"), payload.get("read_runs"))?;
    write_jsonl(&workspace.join("scrape_runs.jsonl"), payload.get("scrape_runs"))?;
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
    out.push_str(&format!("- company: {}\n", payload.get("company").and_then(Value::as_str).unwrap_or("?")));
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
    let mut file = fs::File::create(path)
        .with_context(|| format!("failed to create {}", path.display()))?;
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
            assert!(plan.tier != Tier::C, "Tier C must not appear without opt-in (got {})", plan.source_id);
        }
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
        assert!(plans.iter().any(|p| p.source_id == "zefix.ch"), "Zefix must be present for CH");
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
        let payload =
            run_ctox_person_research_tool(Path::new("/nonexistent"), &request).unwrap();
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
    fn slugify_handles_typical_company_names() {
        assert_eq!(slugify("WITTENSTEIN SE"), "wittenstein-se");
        assert_eq!(slugify("DO & Co. AG"), "do-co-ag");
        assert_eq!(slugify("  Foo   Bar  "), "foo-bar");
    }
}
