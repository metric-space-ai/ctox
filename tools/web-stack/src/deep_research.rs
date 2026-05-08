use anyhow::Result;
use scraper::Html;
use scraper::Selector;
use serde_json::json;
use serde_json::Value;
use std::collections::BTreeMap;
use std::collections::BTreeSet;
use std::fs;
use std::io::Read;
use std::io::Write;
use std::path::Path;
use std::path::PathBuf;
use std::time::Duration;
use std::time::SystemTime;
use std::time::UNIX_EPOCH;
use url::Url;

use crate::scholarly_search::run_ctox_scholarly_search_tool;
use crate::scholarly_search::ScholarlySearchProvider;
use crate::scholarly_search::ScholarlySearchRequest;
use crate::web_search::run_ctox_web_read_tool;
use crate::web_search::run_ctox_web_search_tool;
use crate::web_search::CanonicalWebSearchRequest;
use crate::web_search::ContextSize;
use crate::web_search::DirectWebReadRequest;
use crate::web_search::SearchUserLocation;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeepResearchRequest {
    pub query: String,
    pub focus: Option<String>,
    pub depth: DeepResearchDepth,
    pub max_sources: usize,
    pub include_annas_archive: bool,
    pub include_papers: bool,
    pub workspace: Option<PathBuf>,
    pub persist_workspace: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DeepResearchDepth {
    Quick,
    Standard,
    Exhaustive,
}

impl DeepResearchDepth {
    pub fn from_label(raw: &str) -> Option<Self> {
        match raw.trim().to_ascii_lowercase().as_str() {
            "quick" | "low" => Some(Self::Quick),
            "standard" | "medium" => Some(Self::Standard),
            "exhaustive" | "high" | "deep" => Some(Self::Exhaustive),
            _ => None,
        }
    }

    fn as_str(self) -> &'static str {
        match self {
            Self::Quick => "quick",
            Self::Standard => "standard",
            Self::Exhaustive => "exhaustive",
        }
    }

    fn context_size(self) -> ContextSize {
        match self {
            Self::Quick => ContextSize::Low,
            Self::Standard => ContextSize::Medium,
            Self::Exhaustive => ContextSize::High,
        }
    }

    fn query_budget(self) -> usize {
        match self {
            Self::Quick => 8,
            Self::Standard => 24,
            Self::Exhaustive => 80,
        }
    }

    fn read_budget(self) -> usize {
        match self {
            Self::Quick => 8,
            Self::Standard => 80,
            Self::Exhaustive => 260,
        }
    }

    fn database_query_budget(self) -> usize {
        match self {
            Self::Quick => 3,
            Self::Standard => 12,
            Self::Exhaustive => 40,
        }
    }

    fn snapshot_budget(self) -> usize {
        match self {
            Self::Quick => 8,
            Self::Standard => 32,
            Self::Exhaustive => 96,
        }
    }
}

impl Default for DeepResearchDepth {
    fn default() -> Self {
        Self::Standard
    }
}

#[derive(Debug, Clone)]
struct ResearchSearchPlan {
    label: &'static str,
    query: String,
    domains: Vec<String>,
    scholarly: bool,
    metadata_only: bool,
}

pub fn run_ctox_deep_research_tool(root: &Path, request: &DeepResearchRequest) -> Result<Value> {
    let query_text = normalize_required_query(&request.query)?;
    let search_query = derive_research_search_query(&query_text, request.focus.as_deref());
    let max_sources = request.max_sources.clamp(3, 300);
    let plans = build_research_search_plan(&search_query, request)
        .into_iter()
        .take(request.depth.query_budget())
        .collect::<Vec<_>>();

    let mut sources = Vec::new();
    let mut seen_urls = BTreeSet::new();
    let mut search_runs = Vec::new();

    for plan in &plans {
        if plan.label == "annas_archive_metadata" {
            run_annas_archive_plan(
                root,
                plan,
                request,
                &mut seen_urls,
                &mut sources,
                &mut search_runs,
            );
            continue;
        }
        let payload = match run_ctox_web_search_tool(
            root,
            &CanonicalWebSearchRequest {
                query: plan.query.clone(),
                external_web_access: None,
                allowed_domains: plan.domains.clone(),
                user_location: SearchUserLocation::default(),
                search_context_size: Some(request.depth.context_size()),
                search_content_types: Vec::new(),
                include_sources: true,
            },
        ) {
            Ok(payload) => payload,
            Err(err) => {
                search_runs.push(json!({
                    "label": plan.label,
                    "query": plan.query,
                    "domains": plan.domains,
                    "scholarly": plan.scholarly,
                    "metadata_only": plan.metadata_only,
                    "ok": false,
                    "error": err.to_string(),
                    "result_count": 0,
                }));
                continue;
            }
        };
        search_runs.push(json!({
            "label": plan.label,
            "query": plan.query,
            "domains": plan.domains,
            "scholarly": plan.scholarly,
            "metadata_only": plan.metadata_only,
            "ok": payload.get("ok").and_then(Value::as_bool).unwrap_or(false),
            "provider": payload.get("provider").cloned().unwrap_or(Value::Null),
            "result_count": payload.get("results").and_then(Value::as_array).map(Vec::len).unwrap_or(0),
        }));
        collect_search_sources(&payload, plan, &mut seen_urls, &mut sources);
    }
    let database_runs =
        collect_scholarly_database_sources(&plans, request, &mut seen_urls, &mut sources);

    let read_budget = request.depth.read_budget().min(max_sources);
    let selected_sources = select_balanced_sources(sources, max_sources);
    let mut enriched = Vec::with_capacity(selected_sources.len());
    for mut source in selected_sources {
        let read_url = source_read_url(&source);
        let should_read = enriched.len() < read_budget && should_attempt_source_read(&source);
        if should_read {
            if let Some(url) = read_url {
                match run_ctox_web_read_tool(
                    root,
                    &DirectWebReadRequest {
                        url: url.clone(),
                        query: Some(search_query.clone()),
                        find: build_find_terms(&search_query),
                    },
                ) {
                    Ok(read_payload) => {
                        source["read"] = json!({
                            "ok": read_payload.get("ok").and_then(Value::as_bool).unwrap_or(false),
                            "url": url,
                            "title": read_payload.get("title").cloned().unwrap_or(Value::Null),
                            "summary": read_payload.get("summary").cloned().unwrap_or(Value::Null),
                            "excerpts": read_payload.get("excerpts").cloned().unwrap_or_else(|| json!([])),
                            "find_results": read_payload.get("find_results").cloned().unwrap_or_else(|| json!([])),
                            "is_pdf": read_payload.get("is_pdf").cloned().unwrap_or(Value::Bool(false)),
                            "pdf_total_pages": read_payload.get("pdf_total_pages").cloned().unwrap_or(Value::Null),
                        });
                    }
                    Err(err) => {
                        source["read"] = json!({
                            "ok": false,
                            "url": url,
                            "error": err.to_string(),
                        });
                    }
                }
            }
        }
        enriched.push(source);
    }

    let source_mix = summarize_source_mix(&enriched);
    let figure_candidates = collect_figure_candidates(&enriched);
    let data_links = collect_data_links(&enriched);
    let sources_with_read = enriched
        .iter()
        .filter(|source| source.get("read").is_some())
        .count();
    let successful_page_reads = enriched
        .iter()
        .filter(|source| {
            source
                .get("read")
                .and_then(|read| read.get("ok"))
                .and_then(Value::as_bool)
                .unwrap_or(false)
        })
        .count();
    let failed_page_reads = sources_with_read.saturating_sub(successful_page_reads);
    let mut payload = json!({
        "ok": true,
        "tool": "ctox_deep_research",
        "query": query_text,
        "search_query": search_query,
        "focus": request.focus,
        "depth": request.depth.as_str(),
        "max_sources": max_sources,
        "source_policy": {
            "annas_archive": if request.include_annas_archive {
                "metadata-only discovery. Do not download or reproduce copyrighted full text from unauthorized mirrors; use DOI, publisher, author metadata, abstracts, and lawful open-access copies."
            } else {
                "disabled"
            },
            "paper_full_text": "Prefer publisher abstracts, DOI landing pages, PubMed/PMC, arXiv, institutional repositories, and other lawful open-access sources.",
            "synthesis": "Treat this payload as an evidence bundle; the agent must still weigh credibility, conflict, recency, feasibility, and uncertainty before writing a report."
        },
        "search_plan": plans.iter().map(|plan| json!({
            "label": plan.label,
            "query": plan.query,
            "domains": plan.domains,
            "scholarly": plan.scholarly,
            "metadata_only": plan.metadata_only,
        })).collect::<Vec<_>>(),
        "database_runs": database_runs,
        "search_runs": search_runs,
        "source_mix": source_mix,
        "research_call_counts": {
            "planned_search_queries": plans.len(),
            "executed_search_queries": search_runs.len(),
            "database_queries": database_runs.len(),
            "deduplicated_sources": enriched.len(),
            "sources_with_page_read_attempts": sources_with_read,
            "successful_page_reads": successful_page_reads,
            "failed_page_reads": failed_page_reads,
            "figure_candidates": figure_candidates.len(),
            "estimated_external_fetches": search_runs.len()
                + database_runs.len()
                + sources_with_read
                + enriched.iter().take(24).count(),
        },
        "data_links": data_links,
        "figure_candidates": figure_candidates,
        "sources": enriched,
        "report_scaffold": report_scaffold(&query_text),
    });

    if request.persist_workspace {
        match persist_research_workspace(root, request, &payload) {
            Ok(summary) => {
                payload["research_workspace"] = summary;
            }
            Err(err) => {
                payload["research_workspace_error"] = Value::String(err.to_string());
            }
        }
    }

    Ok(payload)
}

fn select_balanced_sources(sources: Vec<Value>, max_sources: usize) -> Vec<Value> {
    let mut selected = Vec::new();
    let mut buckets = BTreeMap::<String, Vec<Value>>::new();
    for source in sources {
        let kind = source
            .get("source_type")
            .and_then(Value::as_str)
            .unwrap_or("unknown")
            .to_string();
        buckets.entry(kind).or_default().push(source);
    }

    let preferred = [
        "open_access_paper",
        "scholarly",
        "patent",
        "web",
        "metadata",
        "paper_metadata",
        "annas_archive_metadata",
        "unknown",
    ];
    while selected.len() < max_sources {
        let before = selected.len();
        for kind in preferred {
            if selected.len() >= max_sources {
                break;
            }
            if let Some(bucket) = buckets.get_mut(kind) {
                if !bucket.is_empty() {
                    selected.push(bucket.remove(0));
                }
            }
        }
        if selected.len() == before {
            break;
        }
    }
    selected
}

fn normalize_required_query(raw: &str) -> Result<String> {
    let trimmed = raw.split_whitespace().collect::<Vec<_>>().join(" ");
    if trimmed.is_empty() {
        anyhow::bail!("ctox deep research requires a non-empty query");
    }
    Ok(trimmed)
}

fn derive_research_search_query(raw: &str, focus: Option<&str>) -> String {
    let compact = raw.split_whitespace().collect::<Vec<_>>().join(" ");
    if compact.chars().count() <= 220 {
        return append_focus(compact, focus);
    }

    let lowered = compact.to_ascii_lowercase();
    let mut terms = Vec::<&'static str>::new();
    let mapping = [
        (
            ["blitzschutz", "lightning", "lsp"].as_slice(),
            "lightning strike protection",
        ),
        (
            ["kupfergitter", "copper mesh", "mesh"].as_slice(),
            "copper mesh",
        ),
        (
            ["cfk", "cfrp", "carbon"].as_slice(),
            "CFRP carbon fiber composite",
        ),
        (["lack", "primer", "coating"].as_slice(), "coating primer"),
        (
            ["metallische folie", "metallic foil", "folie"].as_slice(),
            "metallic foil",
        ),
        (
            ["kontaktlos", "contactless"].as_slice(),
            "contactless non-destructive testing",
        ),
        (["terahertz", "thz"].as_slice(), "terahertz imaging"),
        (
            ["eddy", "wirbelstrom", "elektrisch", "magnetisch"].as_slice(),
            "eddy current electromagnetic induction",
        ),
        (
            ["thermografie", "thermography", "induktion"].as_slice(),
            "induction thermography",
        ),
        (
            ["mikrowelle", "microwave", "mmwave"].as_slice(),
            "microwave mmWave",
        ),
        (
            ["hyperspektral", "hyperspectral"].as_slice(),
            "hyperspectral imaging",
        ),
        (
            ["roentgen", "röntgen", "x-ray", "ct"].as_slice(),
            "X-ray CT",
        ),
        (["shearografie", "shearography"].as_slice(), "shearography"),
        (["aircraft", "luftfahrt"].as_slice(), "aircraft composites"),
    ];

    for (needles, term) in mapping {
        if needles.iter().any(|needle| lowered.contains(needle)) && !terms.contains(&term) {
            terms.push(term);
        }
    }

    if terms.is_empty() {
        compact.chars().take(220).collect()
    } else {
        let core_terms = terms.iter().take(6).copied().collect::<Vec<_>>().join(" ");
        append_focus(core_terms, focus)
    }
}

fn append_focus(mut query: String, focus: Option<&str>) -> String {
    if let Some(focus) = focus.map(str::trim).filter(|value| !value.is_empty()) {
        query.push(' ');
        query.push_str(focus);
    }
    query.chars().take(260).collect()
}

fn build_research_search_plan(
    query: &str,
    request: &DeepResearchRequest,
) -> Vec<ResearchSearchPlan> {
    let mut plans = vec![
        ResearchSearchPlan {
            label: "broad_web",
            query: query.to_string(),
            domains: Vec::new(),
            scholarly: false,
            metadata_only: false,
        },
        ResearchSearchPlan {
            label: "scholarly_general",
            query: format!("{query} review state of the art feasibility limitations"),
            domains: Vec::new(),
            scholarly: true,
            metadata_only: false,
        },
        ResearchSearchPlan {
            label: "open_access",
            query: format!("{query} PDF open access preprint DOI"),
            domains: vec![
                "arxiv.org".to_string(),
                "pmc.ncbi.nlm.nih.gov".to_string(),
                "frontiersin.org".to_string(),
                "mdpi.com".to_string(),
                "osti.gov".to_string(),
            ],
            scholarly: true,
            metadata_only: false,
        },
        ResearchSearchPlan {
            label: "semantic_scholar",
            query: format!("{query} site:semanticscholar.org"),
            domains: vec!["semanticscholar.org".to_string()],
            scholarly: true,
            metadata_only: true,
        },
        ResearchSearchPlan {
            label: "crossref_doi",
            query: format!("{query} DOI Crossref"),
            domains: vec!["crossref.org".to_string(), "doi.org".to_string()],
            scholarly: true,
            metadata_only: true,
        },
    ];

    if let Some(focus) = request
        .focus
        .as_ref()
        .filter(|value| !value.trim().is_empty())
    {
        plans.push(ResearchSearchPlan {
            label: "user_focus",
            query: format!("{query} {}", focus.trim()),
            domains: Vec::new(),
            scholarly: false,
            metadata_only: false,
        });
    }

    if request.include_papers {
        plans.extend([
            ResearchSearchPlan {
                label: "eddy_current_lsp",
                query: "aircraft composite lightning strike protection layer eddy current testing copper mesh".to_string(),
                domains: Vec::new(),
                scholarly: true,
                metadata_only: false,
            },
            ResearchSearchPlan {
                label: "induction_thermography_lsp",
                query: "CFRP copper mesh lightning strike protection induction thermography eddy current pulsed thermography".to_string(),
                domains: Vec::new(),
                scholarly: true,
                metadata_only: false,
            },
            ResearchSearchPlan {
                label: "terahertz_lsp",
                query: "terahertz imaging aircraft composite lightning strike protection copper mesh coating".to_string(),
                domains: Vec::new(),
                scholarly: true,
                metadata_only: false,
            },
            ResearchSearchPlan {
                label: "microwave_composites",
                query: "microwave mmWave non-destructive testing CFRP composites hidden metal mesh".to_string(),
                domains: Vec::new(),
                scholarly: true,
                metadata_only: false,
            },
            ResearchSearchPlan {
                label: "hyperspectral_coatings",
                query: "hyperspectral imaging aircraft composite coating non destructive testing hidden defects".to_string(),
                domains: Vec::new(),
                scholarly: true,
                metadata_only: false,
            },
            ResearchSearchPlan {
                label: "xray_ct_lsp",
                query: "X-ray CT lightning strike protection copper mesh carbon fiber composite inspection".to_string(),
                domains: Vec::new(),
                scholarly: true,
                metadata_only: false,
            },
            ResearchSearchPlan {
                label: "shearography_composites",
                query: "shearography non destructive testing aircraft composite delamination lightning strike protection".to_string(),
                domains: Vec::new(),
                scholarly: true,
                metadata_only: false,
            },
            ResearchSearchPlan {
                label: "metal_foil_confounder",
                query: "metallic foil shielding terahertz microwave eddy current CFRP composite inspection".to_string(),
                domains: Vec::new(),
                scholarly: true,
                metadata_only: false,
            },
            ResearchSearchPlan {
                label: "aircraft_composites_ndt",
                query: format!(
                    "{query} aircraft composite non destructive testing terahertz eddy current thermography"
                ),
                domains: vec![
                    "sciencedirect.com".to_string(),
                    "aiaa.org".to_string(),
                    "spiedigitallibrary.org".to_string(),
                    "mdpi.com".to_string(),
                    "springer.com".to_string(),
                ],
                scholarly: true,
                metadata_only: false,
            },
            ResearchSearchPlan {
                label: "patents_and_industry",
                query: format!("{query} patent industrial inspection system"),
                domains: vec![
                    "patents.google.com".to_string(),
                    "comsol.com".to_string(),
                    "nde-ed.org".to_string(),
                ],
                scholarly: false,
                metadata_only: false,
            },
            ResearchSearchPlan {
                label: "failure_modes",
                query: format!("{query} anomaly defect delamination corrosion broken mesh hidden metal"),
                domains: Vec::new(),
                scholarly: false,
                metadata_only: false,
            },
        ]);
    }

    if request.include_annas_archive {
        plans.push(ResearchSearchPlan {
            label: "annas_archive_metadata",
            query: format!("{query}"),
            domains: vec!["annas-archive.org".to_string()],
            scholarly: true,
            metadata_only: true,
        });
    }

    plans
}

fn collect_search_sources(
    payload: &Value,
    plan: &ResearchSearchPlan,
    seen_urls: &mut BTreeSet<String>,
    sources: &mut Vec<Value>,
) {
    let Some(results) = payload.get("results").and_then(Value::as_array) else {
        return;
    };
    for result in results {
        let Some(url) = result.get("url").and_then(Value::as_str) else {
            continue;
        };
        let normalized = normalize_url_key(url);
        if normalized.is_empty() || !seen_urls.insert(normalized) {
            continue;
        }
        let source_type = classify_source(url, plan.scholarly, plan.metadata_only);
        sources.push(json!({
            "title": result.get("title").cloned().unwrap_or(Value::Null),
            "url": url,
            "domain": domain_for_url(url),
            "snippet": result.get("snippet").cloned().unwrap_or(Value::Null),
            "rank": result.get("rank").cloned().unwrap_or(Value::Null),
            "source": result.get("source").cloned().unwrap_or(Value::Null),
            "source_type": source_type,
            "search_label": plan.label,
            "scholarly": plan.scholarly,
            "metadata_only": plan.metadata_only || source_type == "annas_archive_metadata",
            "summary": result.get("summary").cloned().unwrap_or(Value::Null),
            "excerpts": result.get("excerpts").cloned().unwrap_or_else(|| json!([])),
            "is_pdf": result.get("is_pdf").cloned().unwrap_or(Value::Bool(false)),
            "pdf_total_pages": result.get("pdf_total_pages").cloned().unwrap_or(Value::Null),
        }));
    }
}

fn run_annas_archive_plan(
    root: &Path,
    plan: &ResearchSearchPlan,
    request: &DeepResearchRequest,
    seen_urls: &mut BTreeSet<String>,
    sources: &mut Vec<Value>,
    search_runs: &mut Vec<Value>,
) {
    let max_results = scholarly_max_results(request);
    // with_oa_pdf is the bridge that lets deep-research actually pull paper
    // PDFs: any AA record with a DOI is augmented with a legal Unpaywall PDF
    // URL, and that URL becomes the source's read target instead of the AA
    // metadata page.
    let scholarly_request = ScholarlySearchRequest {
        query: plan.query.clone(),
        provider: Some(ScholarlySearchProvider::AnnasArchive),
        max_results: Some(max_results),
        with_oa_pdf: true,
        ..Default::default()
    };
    match run_ctox_scholarly_search_tool(root, &scholarly_request) {
        Ok(payload) => {
            search_runs.push(json!({
                "label": plan.label,
                "query": plan.query,
                "domains": plan.domains,
                "scholarly": plan.scholarly,
                "metadata_only": plan.metadata_only,
                "ok": payload.get("ok").and_then(Value::as_bool).unwrap_or(false),
                "provider": payload.get("provider").cloned().unwrap_or(Value::Null),
                "result_count": payload
                    .get("results")
                    .and_then(Value::as_array)
                    .map(Vec::len)
                    .unwrap_or(0),
                "executed_url": payload.get("executed_url").cloned().unwrap_or(Value::Null),
            }));
            collect_scholarly_search_sources(&payload, plan, seen_urls, sources);
        }
        Err(err) => {
            search_runs.push(json!({
                "label": plan.label,
                "query": plan.query,
                "domains": plan.domains,
                "scholarly": plan.scholarly,
                "metadata_only": plan.metadata_only,
                "ok": false,
                "error": err.to_string(),
                "result_count": 0,
            }));
        }
    }
}

fn scholarly_max_results(request: &DeepResearchRequest) -> usize {
    match request.depth {
        DeepResearchDepth::Quick => 8,
        DeepResearchDepth::Standard => 16,
        DeepResearchDepth::Exhaustive => 32,
    }
}

fn collect_scholarly_search_sources(
    payload: &Value,
    plan: &ResearchSearchPlan,
    seen_urls: &mut BTreeSet<String>,
    sources: &mut Vec<Value>,
) {
    let Some(results) = payload.get("results").and_then(Value::as_array) else {
        return;
    };
    let provider = payload
        .get("provider")
        .cloned()
        .unwrap_or(Value::String("annas_archive".to_string()));
    for result in results {
        let Some(detail_url) = result.get("detail_url").and_then(Value::as_str) else {
            continue;
        };
        let oa_pdf = result
            .get("open_access_pdf")
            .and_then(Value::as_str)
            .filter(|value| !value.trim().is_empty());

        // When an OA PDF is available, point the source at the PDF URL so
        // dedup, normalisation, and the read pipeline all key off that URL.
        // Otherwise stay at the AA detail page (metadata-only).
        let source_url = oa_pdf.unwrap_or(detail_url);
        let normalized = normalize_url_key(source_url);
        if normalized.is_empty() || !seen_urls.insert(normalized) {
            continue;
        }
        let source_type = if oa_pdf.is_some() {
            "open_access_paper"
        } else {
            "annas_archive_metadata"
        };
        let metadata_only = source_type == "annas_archive_metadata";
        let snippet = scholarly_snippet(result);

        let mut entry = json!({
            "title": result.get("title").cloned().unwrap_or(Value::Null),
            "url": source_url,
            "domain": domain_for_url(source_url),
            "snippet": snippet,
            "rank": result.get("rank").cloned().unwrap_or(Value::Null),
            "source": provider.clone(),
            "source_type": source_type,
            "search_label": plan.label,
            "scholarly": true,
            "metadata_only": metadata_only,
            "scholarly_metadata": {
                "source_id": result.get("source_id").cloned().unwrap_or(Value::Null),
                "annas_archive_url": detail_url,
                "authors": result.get("authors").cloned().unwrap_or(Value::Null),
                "publisher": result.get("publisher").cloned().unwrap_or(Value::Null),
                "year": result.get("year").cloned().unwrap_or(Value::Null),
                "language": result.get("language").cloned().unwrap_or(Value::Null),
                "file_format": result.get("file_format").cloned().unwrap_or(Value::Null),
                "file_size_label": result.get("file_size_label").cloned().unwrap_or(Value::Null),
                "isbn": result.get("isbn").cloned().unwrap_or(Value::Null),
                "doi": result.get("doi").cloned().unwrap_or(Value::Null),
                "thumbnail_url": result.get("thumbnail_url").cloned().unwrap_or(Value::Null),
                "tags": result.get("tags").cloned().unwrap_or_else(|| json!([])),
            },
            "summary": Value::Null,
            "excerpts": json!([]),
            "is_pdf": Value::Bool(oa_pdf.is_some()),
            "pdf_total_pages": Value::Null,
        });
        // open_access_pdf is the field source_read_url() prefers, so set it
        // explicitly at top level when we have one.
        if let Some(pdf) = oa_pdf {
            entry["open_access_pdf"] = Value::String(pdf.to_string());
            if let Some(license) = result.get("open_access_license").cloned() {
                entry["open_access_license"] = license;
            }
        }
        sources.push(entry);
    }
}

fn scholarly_snippet(result: &Value) -> Value {
    let mut parts: Vec<String> = Vec::new();
    if let Some(authors) = result.get("authors").and_then(Value::as_str) {
        parts.push(authors.to_string());
    }
    if let Some(publisher) = result.get("publisher").and_then(Value::as_str) {
        parts.push(publisher.to_string());
    }
    if let Some(year) = result.get("year").and_then(Value::as_i64) {
        parts.push(year.to_string());
    }
    if let Some(language) = result.get("language").and_then(Value::as_str) {
        parts.push(format!("lang={language}"));
    }
    if let Some(format) = result.get("file_format").and_then(Value::as_str) {
        parts.push(format.to_string());
    }
    if let Some(size) = result.get("file_size_label").and_then(Value::as_str) {
        parts.push(size.to_string());
    }
    if let Some(isbn) = result.get("isbn").and_then(Value::as_str) {
        parts.push(format!("ISBN={isbn}"));
    }
    if let Some(doi) = result.get("doi").and_then(Value::as_str) {
        parts.push(format!("DOI={doi}"));
    }
    if let Some(snippet) = result.get("snippet").and_then(Value::as_str) {
        parts.push(snippet.to_string());
    }
    if parts.is_empty() {
        Value::Null
    } else {
        Value::String(parts.join(" · "))
    }
}

fn should_attempt_source_read(source: &Value) -> bool {
    let source_type = source
        .get("source_type")
        .and_then(Value::as_str)
        .unwrap_or_default();
    if source_type == "annas_archive_metadata" {
        return false;
    }
    source_read_url(source).is_some()
}

fn source_read_url(source: &Value) -> Option<String> {
    if let Some(pdf_url) = source
        .get("open_access_pdf")
        .and_then(Value::as_str)
        .filter(|value| !value.trim().is_empty())
    {
        return Some(pdf_url.to_string());
    }

    if let Some(pdf_url) = source
        .get("primary_location")
        .and_then(|location| location.get("pdf_url"))
        .and_then(Value::as_str)
        .filter(|value| !value.trim().is_empty())
    {
        return Some(pdf_url.to_string());
    }

    source
        .get("url")
        .and_then(Value::as_str)
        .filter(|value| !value.trim().is_empty())
        .map(ToOwned::to_owned)
}

fn collect_scholarly_database_sources(
    plans: &[ResearchSearchPlan],
    request: &DeepResearchRequest,
    seen_urls: &mut BTreeSet<String>,
    sources: &mut Vec<Value>,
) -> Vec<Value> {
    if !request.include_papers {
        return Vec::new();
    }

    let mut runs = Vec::new();
    let queries = plans
        .iter()
        .filter(|plan| plan.scholarly)
        .map(|plan| plan.query.clone())
        .take(request.depth.database_query_budget())
        .collect::<Vec<_>>();
    for query in queries {
        runs.push(match query_crossref(&query, 20) {
            Ok(items) => {
                let count = push_database_sources("crossref", items, seen_urls, sources);
                json!({
                    "database": "crossref",
                    "query": query,
                    "ok": true,
                    "result_count": count,
                })
            }
            Err(err) => json!({
                "database": "crossref",
                "query": query,
                "ok": false,
                "error": err.to_string(),
            }),
        });
        runs.push(match query_openalex(&query, 20) {
            Ok(items) => {
                let count = push_database_sources("openalex", items, seen_urls, sources);
                json!({
                    "database": "openalex",
                    "query": query,
                    "ok": true,
                    "result_count": count,
                })
            }
            Err(err) => json!({
                "database": "openalex",
                "query": query,
                "ok": false,
                "error": err.to_string(),
            }),
        });
        runs.push(match query_semantic_scholar(&query, 12) {
            Ok(items) => {
                let count = push_database_sources("semantic_scholar", items, seen_urls, sources);
                json!({
                    "database": "semantic_scholar",
                    "query": query,
                    "ok": true,
                    "result_count": count,
                })
            }
            Err(err) => json!({
                "database": "semantic_scholar",
                "query": query,
                "ok": false,
                "error": err.to_string(),
            }),
        });
    }
    runs
}

fn push_database_sources(
    database: &'static str,
    items: Vec<Value>,
    seen_urls: &mut BTreeSet<String>,
    sources: &mut Vec<Value>,
) -> usize {
    let mut pushed = 0;
    for mut item in items {
        let Some(url) = item
            .get("url")
            .and_then(Value::as_str)
            .map(ToOwned::to_owned)
        else {
            continue;
        };
        let normalized = normalize_url_key(&url);
        if normalized.is_empty() || !seen_urls.insert(normalized) {
            continue;
        }
        item["source"] = Value::String(database.to_string());
        item["source_type"] = Value::String("paper_metadata".to_string());
        item["search_label"] = Value::String(database.to_string());
        item["scholarly"] = Value::Bool(true);
        item["metadata_only"] = Value::Bool(true);
        item["domain"] = Value::String(domain_for_url(&url));
        sources.push(item);
        pushed += 1;
    }
    pushed
}

fn persist_research_workspace(
    root: &Path,
    request: &DeepResearchRequest,
    payload: &Value,
) -> Result<Value> {
    let workspace = request
        .workspace
        .clone()
        .unwrap_or_else(|| default_research_workspace(root, request, payload));
    fs::create_dir_all(&workspace)?;
    for child in ["reads", "snapshots", "synthesis", "data"] {
        fs::create_dir_all(workspace.join(child))?;
    }

    write_json_pretty(&workspace.join("evidence_bundle.json"), payload)?;
    fs::write(workspace.join("query.txt"), request.query.as_bytes())?;
    fs::write(
        workspace.join("CONTINUE.md"),
        continuation_markdown(payload),
    )?;

    if let Some(items) = payload.get("sources").and_then(Value::as_array) {
        let mut sources_jsonl = fs::File::create(workspace.join("sources.jsonl"))?;
        for (index, source) in items.iter().enumerate() {
            writeln!(sources_jsonl, "{}", serde_json::to_string(source)?)?;
            if let Some(read) = source.get("read") {
                write_json_pretty(
                    &workspace
                        .join("reads")
                        .join(format!("source-{index:04}.json")),
                    read,
                )?;
            }
        }

        let snapshot_count = persist_source_snapshots(
            &workspace.join("snapshots"),
            items,
            request.depth.snapshot_budget(),
        );
        let manifest = json!({
            "workspace": workspace,
            "query": payload.get("query").cloned().unwrap_or(Value::Null),
            "search_query": payload.get("search_query").cloned().unwrap_or(Value::Null),
            "depth": payload.get("depth").cloned().unwrap_or(Value::Null),
            "research_call_counts": payload.get("research_call_counts").cloned().unwrap_or(Value::Null),
            "source_count": items.len(),
            "read_artifact_count": items.iter().filter(|source| source.get("read").is_some()).count(),
            "snapshot_count": snapshot_count,
            "data_link_count": payload.get("data_links").and_then(Value::as_array).map(Vec::len).unwrap_or(0),
            "figure_candidate_count": payload.get("figure_candidates").and_then(Value::as_array).map(Vec::len).unwrap_or(0),
            "files": {
                "evidence_bundle": "evidence_bundle.json",
                "sources_jsonl": "sources.jsonl",
                "continuation": "CONTINUE.md",
                "reads_dir": "reads/",
                "snapshots_dir": "snapshots/",
                "synthesis_dir": "synthesis/",
                "data_dir": "data/"
            }
        });
        write_json_pretty(&workspace.join("manifest.json"), &manifest)?;
    }

    write_json_pretty(
        &workspace.join("search_runs.json"),
        payload.get("search_runs").unwrap_or(&Value::Null),
    )?;
    write_json_pretty(
        &workspace.join("database_runs.json"),
        payload.get("database_runs").unwrap_or(&Value::Null),
    )?;
    write_json_pretty(
        &workspace.join("figure_candidates.json"),
        payload.get("figure_candidates").unwrap_or(&Value::Null),
    )?;
    write_json_pretty(
        &workspace.join("data_links.json"),
        payload.get("data_links").unwrap_or(&Value::Null),
    )?;

    Ok(json!({
        "path": workspace,
        "manifest": workspace.join("manifest.json"),
        "continuation": workspace.join("CONTINUE.md"),
        "evidence_bundle": workspace.join("evidence_bundle.json"),
        "sources_jsonl": workspace.join("sources.jsonl"),
    }))
}

fn default_research_workspace(
    root: &Path,
    request: &DeepResearchRequest,
    payload: &Value,
) -> PathBuf {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .unwrap_or(0);
    let query = payload
        .get("search_query")
        .and_then(Value::as_str)
        .unwrap_or(&request.query);
    root.join("runtime")
        .join("research")
        .join("deep-research")
        .join(format!("{now}-{}", slugify(query)))
}

fn write_json_pretty(path: &Path, value: &Value) -> Result<()> {
    fs::write(path, serde_json::to_vec_pretty(value)?)?;
    Ok(())
}

fn continuation_markdown(payload: &Value) -> String {
    let counts = payload
        .get("research_call_counts")
        .cloned()
        .unwrap_or(Value::Null);
    format!(
        "# Continue Deep Research\n\n\
         Resume from this folder after context compaction or handoff.\n\n\
         1. Read `manifest.json` and `evidence_bundle.json`.\n\
         2. Inspect `sources.jsonl`, `reads/`, and `snapshots/` before synthesis.\n\
         3. Inspect `data_links.json`; follow GitHub/data links when relevant and build diagrams/tables from data if useful.\n\
         4. Write intermediate notes into `synthesis/` before producing the final report.\n\
         5. Keep source-backed claims linked to `sources.jsonl` records or DOI/URL references.\n\n\
         Research call counts:\n\n```json\n{}\n```\n",
        serde_json::to_string_pretty(&counts).unwrap_or_else(|_| "null".to_string())
    )
}

fn persist_source_snapshots(snapshot_dir: &Path, sources: &[Value], limit: usize) -> usize {
    let mut saved = 0;
    for (index, source) in sources.iter().enumerate() {
        if saved >= limit {
            break;
        }
        if source
            .get("source_type")
            .and_then(Value::as_str)
            .is_some_and(|kind| kind == "annas_archive_metadata")
        {
            continue;
        }
        let Some(url) = source_read_url(source) else {
            continue;
        };
        let Ok(snapshot) = fetch_limited_snapshot(&url, 5_000_000) else {
            continue;
        };
        let extension = snapshot_extension(&url, snapshot.content_type.as_deref());
        let target = snapshot_dir.join(format!("source-{index:04}.{extension}"));
        if fs::write(&target, &snapshot.bytes).is_ok() {
            let meta = json!({
                "source_index": index,
                "url": url,
                "content_type": snapshot.content_type,
                "bytes": snapshot.bytes.len(),
                "file": target.file_name().and_then(|name| name.to_str()).unwrap_or_default(),
            });
            let _ = write_json_pretty(
                &snapshot_dir.join(format!("source-{index:04}.metadata.json")),
                &meta,
            );
            saved += 1;
        }
    }
    saved
}

struct Snapshot {
    content_type: Option<String>,
    bytes: Vec<u8>,
}

fn fetch_limited_snapshot(url: &str, max_bytes: usize) -> Result<Snapshot> {
    let agent = ureq::AgentBuilder::new()
        .timeout(Duration::from_secs(8))
        .build();
    let response = agent
        .get(url)
        .set("User-Agent", "ctox-deep-research/0.1")
        .call()
        .map_err(anyhow::Error::from)?;
    let content_type = response.header("content-type").map(|value| {
        value
            .split(';')
            .next()
            .unwrap_or(value)
            .trim()
            .to_ascii_lowercase()
    });
    let mut reader = response.into_reader().take(max_bytes as u64 + 1);
    let mut bytes = Vec::new();
    reader.read_to_end(&mut bytes)?;
    if bytes.len() > max_bytes {
        bytes.truncate(max_bytes);
    }
    Ok(Snapshot {
        content_type,
        bytes,
    })
}

fn snapshot_extension(url: &str, content_type: Option<&str>) -> &'static str {
    if content_type.is_some_and(|value| value.contains("pdf"))
        || url.to_ascii_lowercase().contains(".pdf")
    {
        "pdf"
    } else if content_type.is_some_and(|value| value.contains("html")) {
        "html"
    } else if content_type.is_some_and(|value| value.contains("json")) {
        "json"
    } else if content_type.is_some_and(|value| value.starts_with("text/")) {
        "txt"
    } else {
        "bin"
    }
}

fn query_crossref(query: &str, limit: usize) -> Result<Vec<Value>> {
    let url = format!(
        "https://api.crossref.org/works?rows={}&query.bibliographic={}",
        limit.clamp(1, 20),
        encode_query(query)
    );
    let payload = fetch_json(&url)?;
    let items = payload
        .get("message")
        .and_then(|message| message.get("items"))
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    Ok(items
        .into_iter()
        .filter_map(|item| {
            let title = first_string(item.get("title")).unwrap_or_else(|| "Untitled".to_string());
            let url = item
                .get("URL")
                .and_then(Value::as_str)
                .map(ToOwned::to_owned)
                .or_else(|| {
                    item.get("DOI")
                        .and_then(Value::as_str)
                        .map(|doi| format!("https://doi.org/{doi}"))
                })?;
            Some(json!({
                "title": title,
                "url": url,
                "snippet": crossref_snippet(&item),
                "rank": Value::Null,
                "summary": Value::Null,
                "excerpts": [],
                "is_pdf": false,
                "pdf_total_pages": Value::Null,
                "doi": item.get("DOI").cloned().unwrap_or(Value::Null),
                "year": crossref_year(&item).map(Value::from).unwrap_or(Value::Null),
            }))
        })
        .collect())
}

fn query_openalex(query: &str, limit: usize) -> Result<Vec<Value>> {
    let url = format!(
        "https://api.openalex.org/works?per-page={}&search={}",
        limit.clamp(1, 25),
        encode_query(query)
    );
    let payload = fetch_json(&url)?;
    let items = payload
        .get("results")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    Ok(items
        .into_iter()
        .filter_map(|item| {
            let title = item
                .get("display_name")
                .and_then(Value::as_str)
                .unwrap_or("Untitled")
                .to_string();
            let url = item
                .get("doi")
                .and_then(Value::as_str)
                .map(ToOwned::to_owned)
                .or_else(|| item.get("id").and_then(Value::as_str).map(ToOwned::to_owned))?;
            Some(json!({
                "title": title,
                "url": url,
                "snippet": item.get("abstract_inverted_index").map(|_| Value::String("OpenAlex abstract metadata available".to_string())).unwrap_or(Value::Null),
                "rank": Value::Null,
                "summary": Value::Null,
                "excerpts": [],
                "is_pdf": false,
                "pdf_total_pages": Value::Null,
                "doi": item.get("doi").cloned().unwrap_or(Value::Null),
                "year": item.get("publication_year").cloned().unwrap_or(Value::Null),
                "open_access": item.get("open_access").cloned().unwrap_or(Value::Null),
                "primary_location": item.get("primary_location").cloned().unwrap_or(Value::Null),
            }))
        })
        .collect())
}

fn query_semantic_scholar(query: &str, limit: usize) -> Result<Vec<Value>> {
    let url = format!(
        "https://api.semanticscholar.org/graph/v1/paper/search?limit={}&fields=title,authors,year,url,abstract,venue,externalIds,openAccessPdf,isOpenAccess&query={}",
        limit.clamp(1, 20),
        encode_query(query)
    );
    let payload = fetch_json(&url)?;
    let items = payload
        .get("data")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    Ok(items
        .into_iter()
        .filter_map(|item| {
            let title = item
                .get("title")
                .and_then(Value::as_str)
                .unwrap_or("Untitled")
                .to_string();
            let url = item
                .get("url")
                .and_then(Value::as_str)
                .map(ToOwned::to_owned)
                .or_else(|| {
                    item.get("externalIds")
                        .and_then(|ids| ids.get("DOI"))
                        .and_then(Value::as_str)
                        .map(|doi| format!("https://doi.org/{doi}"))
                })?;
            let open_access_pdf = item
                .get("openAccessPdf")
                .and_then(|pdf| pdf.get("url"))
                .cloned()
                .unwrap_or(Value::Null);
            Some(json!({
                "title": title,
                "url": url,
                "snippet": item.get("abstract").cloned().unwrap_or(Value::Null),
                "rank": Value::Null,
                "summary": Value::Null,
                "excerpts": [],
                "is_pdf": false,
                "pdf_total_pages": Value::Null,
                "venue": item.get("venue").cloned().unwrap_or(Value::Null),
                "year": item.get("year").cloned().unwrap_or(Value::Null),
                "doi": item.get("externalIds").and_then(|ids| ids.get("DOI")).cloned().unwrap_or(Value::Null),
                "open_access_pdf": open_access_pdf,
                "is_open_access": item.get("isOpenAccess").cloned().unwrap_or(Value::Bool(false)),
            }))
        })
        .collect())
}

fn fetch_json(url: &str) -> Result<Value> {
    let agent = ureq::AgentBuilder::new()
        .timeout(Duration::from_secs(8))
        .build();
    let text = agent
        .get(url)
        .set("User-Agent", "ctox-deep-research/0.1")
        .call()
        .map_err(anyhow::Error::from)?
        .into_string()
        .map_err(anyhow::Error::from)?;
    serde_json::from_str(&text).map_err(anyhow::Error::from)
}

fn fetch_text(url: &str) -> Result<String> {
    let agent = ureq::AgentBuilder::new()
        .timeout(Duration::from_secs(8))
        .build();
    agent
        .get(url)
        .set("User-Agent", "ctox-deep-research/0.1")
        .call()
        .map_err(anyhow::Error::from)?
        .into_string()
        .map_err(anyhow::Error::from)
}

fn first_string(value: Option<&Value>) -> Option<String> {
    value
        .and_then(Value::as_array)
        .and_then(|items| items.first())
        .and_then(Value::as_str)
        .map(ToOwned::to_owned)
}

fn crossref_snippet(item: &Value) -> Value {
    let mut parts = Vec::new();
    if let Some(container) = first_string(item.get("container-title")) {
        parts.push(container);
    }
    if let Some(kind) = item.get("type").and_then(Value::as_str) {
        parts.push(kind.to_string());
    }
    if let Some(year) = crossref_year(item) {
        parts.push(year.to_string());
    }
    if parts.is_empty() {
        Value::Null
    } else {
        Value::String(parts.join("; "))
    }
}

fn crossref_year(item: &Value) -> Option<i64> {
    for key in ["published-print", "published-online", "created"] {
        let year = item
            .get(key)
            .and_then(|value| value.get("date-parts"))
            .and_then(Value::as_array)
            .and_then(|parts| parts.first())
            .and_then(Value::as_array)
            .and_then(|date| date.first())
            .and_then(Value::as_i64);
        if year.is_some() {
            return year;
        }
    }
    None
}

fn encode_query(raw: &str) -> String {
    url::form_urlencoded::byte_serialize(raw.as_bytes()).collect()
}

fn classify_source(url: &str, scholarly: bool, metadata_only: bool) -> &'static str {
    let domain = domain_for_url(url);
    if domain.contains("annas-archive.org") {
        "annas_archive_metadata"
    } else if domain.contains("semanticscholar.org") || domain.contains("crossref.org") {
        "paper_metadata"
    } else if domain.contains("arxiv.org")
        || domain.contains("pmc.ncbi.nlm.nih.gov")
        || domain.contains("frontiersin.org")
    {
        "open_access_paper"
    } else if metadata_only {
        "metadata"
    } else if scholarly
        || domain.contains("sciencedirect.com")
        || domain.contains("springer.com")
        || domain.contains("nature.com")
        || domain.contains("ieee.org")
    {
        "scholarly"
    } else if domain.contains("patents.google.com") {
        "patent"
    } else {
        "web"
    }
}

fn summarize_source_mix(sources: &[Value]) -> Value {
    let mut counts = BTreeMap::<String, usize>::new();
    for source in sources {
        if let Some(kind) = source.get("source_type").and_then(Value::as_str) {
            *counts.entry(kind.to_string()).or_default() += 1;
        }
    }
    json!(counts)
}

fn collect_data_links(sources: &[Value]) -> Vec<Value> {
    let mut links = Vec::new();
    let mut seen = BTreeSet::new();
    for (source_index, source) in sources.iter().enumerate() {
        let mut text = String::new();
        append_json_text(&mut text, source.get("url"));
        append_json_text(&mut text, source.get("snippet"));
        append_json_text(&mut text, source.get("summary"));
        if let Some(read) = source.get("read") {
            append_json_text(&mut text, read.get("url"));
            append_json_text(&mut text, read.get("summary"));
            append_json_text(&mut text, read.get("excerpts"));
            append_json_text(&mut text, read.get("find_results"));
        }
        for url in extract_urls(&text) {
            let Some(kind) = classify_data_link(&url) else {
                continue;
            };
            let normalized = normalize_url_key(&url);
            if !seen.insert(normalized) {
                continue;
            }
            links.push(json!({
                "kind": kind,
                "url": url,
                "source_index": source_index,
                "source_title": source.get("title").cloned().unwrap_or(Value::Null),
                "source_url": source.get("url").cloned().unwrap_or(Value::Null),
                "next_step": if kind == "github" {
                    "Inspect repository README, releases, issues, datasets, notebooks, and diagrams if relevant to the research question."
                } else {
                    "Inspect linked dataset or repository and extract tables/figures when useful for synthesis."
                },
            }));
        }
    }
    links
}

fn append_json_text(target: &mut String, value: Option<&Value>) {
    match value {
        Some(Value::String(text)) => {
            target.push(' ');
            target.push_str(text);
        }
        Some(Value::Array(items)) => {
            for item in items {
                append_json_text(target, Some(item));
            }
        }
        Some(Value::Object(map)) => {
            for value in map.values() {
                append_json_text(target, Some(value));
            }
        }
        _ => {}
    }
}

fn extract_urls(text: &str) -> Vec<String> {
    text.split_whitespace()
        .filter_map(|token| {
            let trimmed = token.trim_matches(|c: char| {
                matches!(
                    c,
                    '"' | '\'' | '<' | '>' | '(' | ')' | '[' | ']' | '{' | '}' | ',' | ';'
                )
            });
            if trimmed.starts_with("https://") || trimmed.starts_with("http://") {
                let url = trimmed.trim_end_matches(|c: char| matches!(c, '.' | ',' | ')' | ']'));
                Url::parse(url).ok().map(|parsed| parsed.to_string())
            } else {
                None
            }
        })
        .collect()
}

fn classify_data_link(url: &str) -> Option<&'static str> {
    let domain = domain_for_url(url);
    if domain == "github.com" || domain.ends_with(".github.com") {
        Some("github")
    } else if domain.contains("gitlab.com") {
        Some("gitlab")
    } else if domain.contains("zenodo.org")
        || domain.contains("figshare.com")
        || domain.contains("kaggle.com")
        || domain.contains("huggingface.co")
        || domain.contains("data.mendeley.com")
        || domain.contains("osf.io")
        || domain.contains("dataverse")
    {
        Some("dataset")
    } else {
        None
    }
}

fn collect_figure_candidates(sources: &[Value]) -> Vec<Value> {
    let mut figures = Vec::new();
    let mut seen = BTreeSet::new();
    let img_selector = Selector::parse("img").ok();
    let meta_selector =
        Selector::parse("meta[property='og:image'], meta[name='twitter:image']").ok();

    for source in sources.iter().take(24) {
        let Some(page_url) = source.get("url").and_then(Value::as_str) else {
            continue;
        };
        let Ok(html) = fetch_text(page_url) else {
            continue;
        };
        let doc = Html::parse_document(&html);
        if let Some(selector) = &meta_selector {
            for element in doc.select(selector).take(3) {
                if let Some(raw) = element.value().attr("content") {
                    push_figure_candidate(
                        &mut figures,
                        &mut seen,
                        page_url,
                        raw,
                        element
                            .value()
                            .attr("property")
                            .or_else(|| element.value().attr("name")),
                        source,
                    );
                }
            }
        }
        if let Some(selector) = &img_selector {
            for element in doc.select(selector).take(20) {
                let Some(raw) = element.value().attr("src") else {
                    continue;
                };
                push_figure_candidate(
                    &mut figures,
                    &mut seen,
                    page_url,
                    raw,
                    element.value().attr("alt"),
                    source,
                );
                if figures.len() >= 40 {
                    return figures;
                }
            }
        }
    }
    figures
}

fn push_figure_candidate(
    figures: &mut Vec<Value>,
    seen: &mut BTreeSet<String>,
    page_url: &str,
    raw_image_url: &str,
    caption_hint: Option<&str>,
    source: &Value,
) {
    let Some(image_url) = resolve_url(page_url, raw_image_url) else {
        return;
    };
    let normalized = normalize_url_key(&image_url);
    if !seen.insert(normalized) {
        return;
    }
    let lower = image_url.to_ascii_lowercase();
    if !(lower.ends_with(".png")
        || lower.ends_with(".jpg")
        || lower.ends_with(".jpeg")
        || lower.ends_with(".webp")
        || lower.contains("image")
        || lower.contains("figure"))
    {
        return;
    }
    figures.push(json!({
        "image_url": image_url,
        "source_page": page_url,
        "source_title": source.get("title").cloned().unwrap_or(Value::Null),
        "caption_hint": caption_hint.unwrap_or(""),
        "usage_note": "Candidate source figure. Check license/permission before embedding; otherwise cite as source-only or redraw as own schematic.",
    }));
}

fn resolve_url(base: &str, raw: &str) -> Option<String> {
    let raw = raw.trim();
    if raw.is_empty() || raw.starts_with("data:") {
        return None;
    }
    if let Ok(url) = Url::parse(raw) {
        return Some(url.to_string());
    }
    Url::parse(base)
        .ok()
        .and_then(|base_url| base_url.join(raw).ok())
        .map(|url| url.to_string())
}

fn build_find_terms(query: &str) -> Vec<String> {
    query
        .split(|c: char| !c.is_alphanumeric())
        .filter(|part| part.chars().count() >= 6)
        .take(6)
        .map(ToOwned::to_owned)
        .collect()
}

fn domain_for_url(raw: &str) -> String {
    Url::parse(raw)
        .ok()
        .and_then(|url| url.host_str().map(ToOwned::to_owned))
        .unwrap_or_default()
        .trim_start_matches("www.")
        .to_ascii_lowercase()
}

fn normalize_url_key(raw: &str) -> String {
    Url::parse(raw)
        .map(|mut url| {
            url.set_fragment(None);
            url.to_string()
        })
        .unwrap_or_else(|_| raw.trim().to_ascii_lowercase())
}

fn slugify(raw: &str) -> String {
    let mut slug = String::new();
    for ch in raw.chars() {
        if ch.is_ascii_alphanumeric() {
            slug.push(ch.to_ascii_lowercase());
        } else if (ch.is_whitespace() || matches!(ch, '-' | '_' | '/' | ':'))
            && !slug.ends_with('-')
        {
            slug.push('-');
        }
        if slug.len() >= 72 {
            break;
        }
    }
    slug.trim_matches('-').to_string().if_empty("research")
}

trait IfEmpty {
    fn if_empty(self, fallback: &str) -> String;
}

impl IfEmpty for String {
    fn if_empty(self, fallback: &str) -> String {
        if self.is_empty() {
            fallback.to_string()
        } else {
            self
        }
    }
}

fn report_scaffold(query: &str) -> Value {
    json!({
        "recommended_sections": [
            "Management Summary",
            "Problem and Inspection Geometry",
            "Assumptions and Boundary Conditions",
            "Search Strategy and Evidence Base",
            "Technology Candidates",
            "Scientific and Industrial Evidence",
            "Feasibility Assessment",
            "Experiment Design",
            "Risks, Unknowns, and Decision Gates",
            "Recommendation",
            "References"
        ],
        "evaluation_axes": [
            "contactless operation",
            "single-shot or scan throughput",
            "penetration through coating/primer/CFK",
            "sensitivity to copper mesh anomalies",
            "confounding from continuous metallic foil",
            "stand-off tolerance and field deployability",
            "safety and certification constraints",
            "TRL and integration risk"
        ],
        "synthesis_instruction": format!(
            "Write a decision-grade research report for: {query}. Cite every factual claim that depends on external evidence, separate evidence from inference, and score technologies with explicit uncertainty."
        )
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn depth_labels_accept_deep_alias() {
        assert_eq!(
            DeepResearchDepth::from_label("deep"),
            Some(DeepResearchDepth::Exhaustive)
        );
        assert_eq!(
            DeepResearchDepth::from_label("medium"),
            Some(DeepResearchDepth::Standard)
        );
    }

    #[test]
    fn plan_can_include_annas_archive_as_metadata_only() {
        let request = DeepResearchRequest {
            query: "terahertz copper mesh CFRP".to_string(),
            focus: None,
            depth: DeepResearchDepth::Exhaustive,
            max_sources: 20,
            include_annas_archive: true,
            include_papers: true,
            workspace: None,
            persist_workspace: false,
        };
        let plans = build_research_search_plan(&request.query, &request);
        let annas = plans
            .iter()
            .find(|plan| plan.label == "annas_archive_metadata")
            .expect("annas metadata plan");
        assert!(annas.metadata_only);
        assert_eq!(annas.domains, vec!["annas-archive.org"]);
    }

    #[test]
    fn classifies_current_playwright_relevant_scholarly_sources() {
        assert_eq!(
            classify_source("https://arxiv.org/abs/2401.00001", true, false),
            "open_access_paper"
        );
        assert_eq!(
            classify_source("https://annas-archive.org/search?q=test", true, true),
            "annas_archive_metadata"
        );
        assert_eq!(
            classify_source("https://patents.google.com/patent/US123", false, false),
            "patent"
        );
    }

    #[test]
    fn derives_concise_search_query_from_long_german_research_prompt() {
        let prompt = "Da es um Metallstrukturen in Kunststoff geht und unter dem Gitter scheinbar noch eine konstante metallische Folie liegt: waere es denkbar, mit elektrischen und magnetischen Feldern zu arbeiten? Der Blitzschutz wird durch ein Kupfergitter in kohlenstofffaserverstaerktem Kunststoff CFK bewerkstelligt. Zu bewerten sind Hyperspektralkamera, Terahertz Imaging, Eddy Current, Induktion, Thermografie, Mikrowelle/mmWave, Roentgen/CT und Shearografie.";
        let query = derive_research_search_query(prompt, None);
        assert!(query.contains("lightning strike protection"));
        assert!(query.contains("copper mesh"));
        assert!(query.contains("CFRP"));
        assert!(query.contains("eddy current"));
        assert!(query.chars().count() <= 260);
    }

    #[test]
    fn scholarly_metadata_sources_are_read_but_annas_archive_is_not() {
        let paper = json!({
            "source_type": "paper_metadata",
            "url": "https://doi.org/10.1234/example",
            "metadata_only": true,
        });
        let annas = json!({
            "source_type": "annas_archive_metadata",
            "url": "https://annas-archive.org/search?q=example",
            "metadata_only": true,
        });
        assert!(should_attempt_source_read(&paper));
        assert!(!should_attempt_source_read(&annas));
    }

    #[test]
    fn annas_archive_records_with_oa_pdf_become_downloadable_paper_sources() {
        // Simulates a payload from scholarly_search where one record carried a
        // DOI and got augmented with a legal Unpaywall open_access_pdf URL.
        // The deep-research collector must turn that into a readable
        // `open_access_paper` source whose top-level `url` points at the PDF
        // - which is what the existing read pipeline downloads.
        let payload = json!({
            "ok": true,
            "tool": "ctox_scholarly_search",
            "provider": "annas_archive",
            "executed_url": "https://annas-archive.org/search?q=shannon",
            "results": [
                {
                    "provider": "annas_archive",
                    "source_id": "1111111111111111111111111111aaaa",
                    "detail_url": "https://annas-archive.org/md5/1111111111111111111111111111aaaa",
                    "title": "Information Theory of Communication",
                    "authors": "C. E. Shannon",
                    "year": 1948,
                    "language": "en",
                    "file_format": "pdf",
                    "file_size_label": "1.1MB",
                    "doi": "10.1002/j.1538-7305.1948.tb01338.x",
                    "open_access_pdf": "https://example.org/papers/shannon-1948.pdf",
                    "open_access_license": "cc-by",
                    "rank": 1,
                    "tags": []
                },
                {
                    "provider": "annas_archive",
                    "source_id": "2222222222222222222222222222bbbb",
                    "detail_url": "https://annas-archive.org/md5/2222222222222222222222222222bbbb",
                    "title": "A Book Without DOI",
                    "year": 2010,
                    "language": "en",
                    "file_format": "epub",
                    "rank": 2,
                    "tags": []
                }
            ]
        });
        let plan = ResearchSearchPlan {
            label: "annas_archive_metadata",
            query: "shannon".to_string(),
            domains: vec!["annas-archive.org".to_string()],
            scholarly: true,
            metadata_only: true,
        };
        let mut seen_urls = BTreeSet::new();
        let mut sources = Vec::new();
        collect_scholarly_search_sources(&payload, &plan, &mut seen_urls, &mut sources);

        assert_eq!(sources.len(), 2);

        // First source: DOI + OA PDF -> open_access_paper, downloadable.
        let paper = &sources[0];
        assert_eq!(
            paper.get("source_type").and_then(Value::as_str),
            Some("open_access_paper"),
            "DOI-bearing record with OA PDF must become open_access_paper, got: {paper}"
        );
        assert_eq!(
            paper.get("url").and_then(Value::as_str),
            Some("https://example.org/papers/shannon-1948.pdf"),
            "top-level url must point at the OA PDF so the read pipeline downloads it"
        );
        assert_eq!(
            paper.get("open_access_pdf").and_then(Value::as_str),
            Some("https://example.org/papers/shannon-1948.pdf"),
        );
        assert_eq!(
            paper.get("open_access_license").and_then(Value::as_str),
            Some("cc-by")
        );
        assert_eq!(
            paper.get("metadata_only").and_then(Value::as_bool),
            Some(false)
        );
        assert_eq!(paper.get("is_pdf").and_then(Value::as_bool), Some(true));
        assert_eq!(
            paper
                .pointer("/scholarly_metadata/doi")
                .and_then(Value::as_str),
            Some("10.1002/j.1538-7305.1948.tb01338.x")
        );
        assert_eq!(
            paper
                .pointer("/scholarly_metadata/annas_archive_url")
                .and_then(Value::as_str),
            Some("https://annas-archive.org/md5/1111111111111111111111111111aaaa")
        );

        // Now verify the read pipeline gates: should_attempt_source_read must
        // return true for the paper, and source_read_url must return the PDF.
        assert!(
            should_attempt_source_read(paper),
            "open_access_paper must be eligible for download in the read pass"
        );
        assert_eq!(
            source_read_url(paper).as_deref(),
            Some("https://example.org/papers/shannon-1948.pdf"),
            "source_read_url must point at the OA PDF for download"
        );

        // Second source: no DOI -> stays metadata-only, NOT downloaded.
        let book = &sources[1];
        assert_eq!(
            book.get("source_type").and_then(Value::as_str),
            Some("annas_archive_metadata")
        );
        assert_eq!(
            book.get("url").and_then(Value::as_str),
            Some("https://annas-archive.org/md5/2222222222222222222222222222bbbb")
        );
        assert!(book.get("open_access_pdf").is_none());
        assert!(
            !should_attempt_source_read(book),
            "annas_archive_metadata records must NOT be downloaded"
        );
    }

    #[test]
    fn collects_github_and_dataset_links_from_sources() {
        let sources = vec![json!({
            "title": "Supplemented paper",
            "url": "https://doi.org/10.1234/example",
            "snippet": "Code: https://github.com/example/project and data https://zenodo.org/records/123",
            "read": {
                "summary": "Notebook at https://huggingface.co/datasets/example/data."
            }
        })];
        let links = collect_data_links(&sources);
        assert!(links
            .iter()
            .any(|link| link.get("kind").and_then(Value::as_str) == Some("github")));
        assert!(links
            .iter()
            .any(|link| link.get("kind").and_then(Value::as_str) == Some("dataset")));
    }

    #[test]
    fn persists_research_workspace_manifest_and_continuation() {
        let root = std::env::temp_dir().join(format!(
            "ctox_deep_research_workspace_test_{}",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let workspace = root.join("research-folder");
        let request = DeepResearchRequest {
            query: "test query".to_string(),
            focus: None,
            depth: DeepResearchDepth::Quick,
            max_sources: 3,
            include_annas_archive: false,
            include_papers: true,
            workspace: Some(workspace.clone()),
            persist_workspace: true,
        };
        let payload = json!({
            "query": "test query",
            "search_query": "test query",
            "depth": "quick",
            "research_call_counts": {"deduplicated_sources": 0},
            "sources": [],
            "search_runs": [],
            "database_runs": [],
            "figure_candidates": [],
            "data_links": [],
        });
        let summary = persist_research_workspace(&root, &request, &payload).unwrap();
        assert!(workspace.join("manifest.json").is_file());
        assert!(workspace.join("CONTINUE.md").is_file());
        assert!(workspace.join("synthesis").is_dir());
        assert_eq!(
            summary.get("path").and_then(Value::as_str),
            Some(workspace.to_str().unwrap())
        );
        let _ = fs::remove_dir_all(root);
    }
}
