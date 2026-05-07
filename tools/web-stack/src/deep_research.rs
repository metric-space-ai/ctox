use anyhow::Result;
use scraper::Html;
use scraper::Selector;
use serde_json::json;
use serde_json::Value;
use std::collections::BTreeMap;
use std::collections::BTreeSet;
use std::path::Path;
use std::time::Duration;
use url::Url;

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
            Self::Standard => 40,
            Self::Exhaustive => 180,
        }
    }

    fn database_query_budget(self) -> usize {
        match self {
            Self::Quick => 3,
            Self::Standard => 12,
            Self::Exhaustive => 40,
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
    let database_runs =
        collect_scholarly_database_sources(&plans, request, &mut seen_urls, &mut sources);

    for plan in &plans {
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
        if sources.len() >= max_sources {
            break;
        }
    }

    let read_budget = request.depth.read_budget().min(max_sources);
    let mut enriched = Vec::with_capacity(sources.len());
    for mut source in sources.into_iter().take(max_sources) {
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
    Ok(json!({
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
        "figure_candidates": figure_candidates,
        "sources": enriched,
        "report_scaffold": report_scaffold(&query_text),
    }))
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
        runs.push(match query_crossref(&query, 12) {
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
        runs.push(match query_openalex(&query, 12) {
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
        runs.push(match query_semantic_scholar(&query, 8) {
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
}
