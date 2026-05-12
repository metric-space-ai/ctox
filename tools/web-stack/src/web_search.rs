use anyhow::{anyhow, bail, Context, Result};
use chrono::Datelike;
use ctox_pdf_parse::{
    page_count_for_pdf_bytes, parse_pdf_bytes as parse_pdf_bytes_internal,
    LiteParseConfigOverrides, OutputFormat,
};
use regex::Regex;
use roxmltree::Document;
use scraper::{ElementRef, Html, Selector};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::BTreeMap;
use std::fs;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
#[cfg(test)]
use std::sync::Mutex;
use std::sync::OnceLock;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use url::Url;

use crate::runtime_config;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum ProviderKind {
    Auto,
    Google,
    Brave,
    DuckDuckGo,
    Bing,
    Searxng,
    AnnasArchive,
    Mock,
}

impl ProviderKind {
    fn from_config_value(raw: Option<String>) -> Self {
        match raw
            .as_deref()
            .unwrap_or("auto")
            .trim()
            .to_ascii_lowercase()
            .as_str()
        {
            "auto" | "" => Self::Auto,
            "google"
            | "playwright_google"
            | "google_playwright"
            | "google_pw"
            | "google_browser"
            | "google_bootstrap_native"
            | "google_bootstrapped"
            | "google_hybrid" => Self::Google,
            "brave" | "brave_search" => Self::Brave,
            "duckduckgo" | "ddg" => Self::DuckDuckGo,
            "bing" => Self::Bing,
            "mock" => Self::Mock,
            "searxng" => Self::Searxng,
            "annas_archive" | "annas-archive" | "anna_archive" | "anna-archive" | "annas" => {
                Self::AnnasArchive
            }
            _ => Self::Auto,
        }
    }

    fn as_str(self) -> &'static str {
        match self {
            Self::Auto => "auto",
            Self::Google => "google",
            Self::Brave => "brave",
            Self::DuckDuckGo => "duckduckgo",
            Self::Bing => "bing",
            Self::Searxng => "searxng",
            Self::AnnasArchive => "annas_archive",
            Self::Mock => "mock",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ContextSize {
    Low,
    Medium,
    High,
}

impl ContextSize {
    pub fn from_label(value: &str) -> Option<Self> {
        match value.trim().to_ascii_lowercase().as_str() {
            "low" => Some(Self::Low),
            "medium" => Some(Self::Medium),
            "high" => Some(Self::High),
            _ => None,
        }
    }

    fn from_value(value: Option<&Value>) -> Option<Self> {
        Self::from_label(value.and_then(Value::as_str)?)
    }

    fn as_str(self) -> &'static str {
        match self {
            Self::Low => "low",
            Self::Medium => "medium",
            Self::High => "high",
        }
    }

    fn top_k(self) -> usize {
        match self {
            Self::Low => 3,
            Self::Medium => 5,
            Self::High => 8,
        }
    }

    fn evidence_docs(self) -> usize {
        match self {
            Self::Low => 1,
            Self::Medium => 2,
            Self::High => 3,
        }
    }
}

#[derive(Debug, Clone, Default)]
struct SearchToolRequest {
    external_web_access: Option<bool>,
    allowed_domains: Vec<String>,
    user_location: SearchUserLocation,
    search_context_size: Option<ContextSize>,
    search_content_types: Vec<String>,
    include_sources: bool,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SearchUserLocation {
    pub country: Option<String>,
    pub region: Option<String>,
    pub city: Option<String>,
    pub timezone: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CanonicalWebSearchRequest {
    pub query: String,
    pub external_web_access: Option<bool>,
    pub allowed_domains: Vec<String>,
    pub user_location: SearchUserLocation,
    pub search_context_size: Option<ContextSize>,
    pub search_content_types: Vec<String>,
    pub include_sources: bool,
}

#[derive(Debug, Clone)]
pub struct CanonicalWebSearchExecution {
    pub injected_context: String,
    pub augmentation: WebSearchAugmentation,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct DirectWebReadRequest {
    pub url: String,
    pub query: Option<String>,
    pub find: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OpenAiWebSearchCompatMode {
    CtoxPrimary,
    Passthrough,
}

impl OpenAiWebSearchCompatMode {
    fn from_root(root: &Path) -> Self {
        match runtime_config::get(root, "CTOX_WEB_SEARCH_OPENAI_MODE")
            .as_deref()
            .unwrap_or("local_stack")
            .trim()
            .to_ascii_lowercase()
            .as_str()
        {
            "openai" | "openai_passthrough" | "passthrough" | "compat" => Self::Passthrough,
            "local_stack" | "ctox" | "ctox_primary" => Self::CtoxPrimary,
            _ => Self::CtoxPrimary,
        }
    }
}

#[derive(Debug, Clone)]
struct SearchConfig {
    enabled: bool,
    provider: ProviderKind,
    searxng_base_url: Option<String>,
    timeout_ms: u64,
    default_top_k: usize,
    max_top_k: usize,
    user_agent: String,
    default_language: Option<String>,
    default_region: Option<String>,
    default_safe_search: bool,
    cache_ttl_secs: u64,
    page_cache_ttl_secs: u64,
    max_page_bytes: usize,
    max_page_chars: usize,
    max_pdf_pages: usize,
}

impl SearchConfig {
    fn from_root(root: &Path) -> Self {
        Self {
            enabled: read_bool(root, "CTOX_WEB_SEARCH_ENABLED", true),
            provider: ProviderKind::from_config_value(runtime_config::get(
                root,
                "CTOX_WEB_SEARCH_PROVIDER",
            )),
            searxng_base_url: runtime_config::get(root, "CTOX_WEB_SEARCH_SEARXNG_BASE_URL"),
            timeout_ms: read_u64(root, "CTOX_WEB_SEARCH_TIMEOUT_MS", 7000),
            default_top_k: read_usize(root, "CTOX_WEB_SEARCH_TOP_K", 5),
            max_top_k: read_usize(root, "CTOX_WEB_SEARCH_MAX_TOP_K", 8),
            user_agent: runtime_config::get(root, "CTOX_WEB_SEARCH_USER_AGENT")
                .unwrap_or_else(|| {
                    "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/136.0.0.0 Safari/537.36".to_string()
                }),
            default_language: runtime_config::get(root, "CTOX_WEB_SEARCH_LANGUAGE"),
            default_region: runtime_config::get(root, "CTOX_WEB_SEARCH_REGION"),
            default_safe_search: read_bool(root, "CTOX_WEB_SEARCH_SAFE", true),
            cache_ttl_secs: read_u64(root, "CTOX_WEB_SEARCH_CACHE_TTL_SECS", 86_400),
            page_cache_ttl_secs: read_u64(root, "CTOX_WEB_SEARCH_PAGE_CACHE_TTL_SECS", 259_200),
            max_page_bytes: read_usize(root, "CTOX_WEB_SEARCH_MAX_PAGE_BYTES", 2_000_000),
            max_page_chars: read_usize(root, "CTOX_WEB_SEARCH_MAX_PAGE_CHARS", 16_000),
            max_pdf_pages: read_usize(root, "CTOX_WEB_SEARCH_MAX_PDF_PAGES", 12),
        }
    }
}

#[derive(Debug, Clone)]
struct SearchQuery {
    text: String,
    count: usize,
    offset: usize,
    language: Option<String>,
    region: Option<String>,
    safe_search: u8,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct SearchHit {
    title: String,
    url: String,
    snippet: String,
    source: String,
    rank: usize,
}

#[derive(Debug, Clone)]
struct SearchResponse {
    provider: String,
    hits: Vec<SearchHit>,
    evidence: Vec<EvidenceDoc>,
    executed_queries: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct EvidenceDoc {
    url: String,
    title: String,
    summary: String,
    #[serde(default)]
    is_pdf: bool,
    #[serde(default)]
    pdf_total_pages: Option<usize>,
    #[serde(default)]
    page_sections: Vec<EvidenceSection>,
    #[serde(default)]
    excerpts: Vec<String>,
    #[serde(default)]
    page_text: String,
    #[serde(default)]
    find_results: Vec<FindInPageResult>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct FindInPageResult {
    pattern: String,
    #[serde(default)]
    matches: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct EvidenceSection {
    #[serde(default)]
    page_number: Option<u32>,
    text: String,
}

#[derive(Debug, Clone)]
struct OpenedPage {
    title: String,
    summary: String,
    is_pdf: bool,
    pdf_total_pages: Option<usize>,
    page_sections: Vec<EvidenceSection>,
    excerpts: Vec<String>,
    page_text: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PageAdapterKind {
    Github,
    KnowledgeSite,
    DocsSite,
    NewsSite,
    GenericHtml,
    PlainText,
}

#[derive(Debug, Clone)]
struct FetchedPageContent {
    body: Vec<u8>,
    content_type: Option<String>,
    final_url: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct GithubApiPayload {
    kind: String,
    title: String,
    #[serde(default)]
    repo: String,
    #[serde(default)]
    path: Option<String>,
    #[serde(default)]
    description: String,
    #[serde(default)]
    readme: String,
    #[serde(default)]
    entries: Vec<String>,
    #[serde(default)]
    supplemental_files: Vec<GithubApiFile>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct GithubApiFile {
    path: String,
    #[serde(default)]
    text: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct GithubContentEntry {
    name: String,
    path: String,
    kind: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum GithubUrlKind {
    RepoRoot,
    Tree,
    Blob,
    Other,
}

#[derive(Debug, Clone)]
struct GithubUrlParts {
    owner: String,
    repo: String,
    kind: GithubUrlKind,
    ref_name: Option<String>,
    path: Option<String>,
}

const GITHUB_API_BASE: &str = "https://api.github.com";
const GITHUB_API_VERSION: &str = "2022-11-28";
const GITHUB_API_CONTENT_TYPE: &str = "application/x-ctox-github+json";

#[derive(Debug, Clone)]
struct PdfExtraction {
    total_pages: usize,
    sections: Vec<EvidenceSection>,
}

#[derive(Debug, Clone)]
pub struct WebSearchAugmentation {
    calls: Vec<WebSearchCall>,
    citations: Vec<SearchCitation>,
}

#[derive(Debug, Clone)]
struct WebSearchCall {
    id: String,
    status: &'static str,
    action: Option<WebSearchAction>,
}

#[derive(Debug, Clone)]
enum WebSearchAction {
    Search {
        query: String,
        queries: Vec<String>,
        sources: Option<Vec<WebSearchSource>>,
    },
    OpenPage {
        url: String,
    },
    FindInPage {
        url: String,
        pattern: String,
    },
}

#[derive(Debug, Clone, Serialize)]
struct WebSearchSource {
    #[serde(rename = "type")]
    kind: String,
    url: String,
}

impl WebSearchAugmentation {
    fn output_items(&self) -> impl Iterator<Item = Value> + '_ {
        self.calls.iter().map(WebSearchCall::output_item)
    }

    fn search_failure(call_id: String, query: String, queries: Vec<String>) -> Self {
        Self {
            calls: vec![WebSearchCall {
                id: call_id,
                status: "failed",
                action: Some(WebSearchAction::Search {
                    query,
                    queries,
                    sources: None,
                }),
            }],
            citations: Vec::new(),
        }
    }
}

impl WebSearchCall {
    fn output_item(&self) -> Value {
        let mut item = json!({
            "type": "web_search_call",
            "id": self.id,
            "status": self.status,
        });
        item["action"] = self
            .action
            .as_ref()
            .map(WebSearchAction::output_value)
            .unwrap_or(Value::Null);
        item
    }
}

impl WebSearchAction {
    fn output_value(&self) -> Value {
        match self {
            Self::Search {
                query,
                queries,
                sources,
            } => {
                let mut value = json!({
                    "type": "search",
                    "query": query,
                    "queries": queries,
                });
                if let Some(sources) = sources {
                    value["sources"] =
                        serde_json::to_value(sources).unwrap_or_else(|_| Value::Array(Vec::new()));
                }
                value
            }
            Self::OpenPage { url } => json!({
                "type": "open_page",
                "url": url,
            }),
            Self::FindInPage { url, pattern } => json!({
                "type": "find_in_page",
                "url": url,
                "pattern": pattern,
            }),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct SearchCitation {
    title: String,
    url: String,
}

#[derive(Debug, Serialize, Deserialize, Default)]
struct SearchCacheFile {
    entries: BTreeMap<String, SearchCacheEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct SearchCacheEntry {
    created_at_epoch: u64,
    provider: String,
    hits: Vec<SearchHit>,
    #[serde(default)]
    evidence: Vec<EvidenceDoc>,
}

#[derive(Debug, Serialize, Deserialize, Default)]
struct PageCacheFile {
    entries: BTreeMap<String, PageCacheEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct PageCacheEntry {
    created_at_epoch: u64,
    original_url: String,
    final_url: String,
    content_type: Option<String>,
    doc: EvidenceDoc,
}

struct WebSearchSession<'a> {
    root: &'a Path,
    config: &'a SearchConfig,
    request_docs: BTreeMap<String, EvidenceDoc>,
    page_cache: PageCacheFile,
    page_cache_dirty: bool,
}

pub fn execute_canonical_web_search(
    root: &Path,
    request: &CanonicalWebSearchRequest,
) -> Result<Option<CanonicalWebSearchExecution>> {
    let config = SearchConfig::from_root(root);
    if !config.enabled {
        return Ok(None);
    }

    let Some(query_text) = normalize_text(&request.query) else {
        return Ok(None);
    };

    let tool_request = canonical_request_to_tool_request(request);
    let context_size = tool_request
        .search_context_size
        .unwrap_or(ContextSize::Medium);
    let top_k = context_size
        .top_k()
        .max(config.default_top_k)
        .min(config.max_top_k.max(1));
    let query = SearchQuery {
        text: build_search_text(&query_text, &tool_request.allowed_domains),
        count: top_k,
        offset: 0,
        language: config.default_language.clone(),
        region: derive_region(&config, &tool_request.user_location),
        safe_search: if config.default_safe_search { 1 } else { 0 },
    };
    let call_id = format!("ws_ctox_{}", unix_ts());
    let query_variants = build_query_variants(&query_text, &query.text);

    let execution = match execute_search(root, &config, &tool_request, &query_text, &query) {
        Ok(result) => CanonicalWebSearchExecution {
            injected_context: render_results_context(
                &query_text,
                &tool_request,
                context_size,
                &result,
            ),
            augmentation: WebSearchAugmentation {
                calls: build_web_search_calls(&call_id, &result, tool_request.include_sources),
                citations: result
                    .hits
                    .iter()
                    .take(3)
                    .map(|hit| SearchCitation {
                        title: hit.title.clone(),
                        url: hit.url.clone(),
                    })
                    .collect(),
            },
        },
        Err(err) => CanonicalWebSearchExecution {
            injected_context: render_failure_context(&query_text, &tool_request, &err),
            augmentation: WebSearchAugmentation::search_failure(
                call_id,
                query_text,
                query_variants,
            ),
        },
    };

    Ok(Some(execution))
}

pub fn run_ctox_web_search_tool(root: &Path, request: &CanonicalWebSearchRequest) -> Result<Value> {
    let config = SearchConfig::from_root(root);
    if !config.enabled {
        return Ok(json!({
            "ok": false,
            "tool": "ctox_web_search",
            "error": "CTOX web search is disabled",
        }));
    }

    let query_text =
        normalize_text(&request.query).context("ctox_web_search requires a non-empty query")?;
    let tool_request = canonical_request_to_tool_request(request);
    let context_size = tool_request
        .search_context_size
        .unwrap_or(ContextSize::Medium);
    let top_k = context_size
        .top_k()
        .max(config.default_top_k)
        .min(config.max_top_k.max(1));
    let query = SearchQuery {
        text: build_search_text(&query_text, &tool_request.allowed_domains),
        count: top_k,
        offset: 0,
        language: config.default_language.clone(),
        region: derive_region(&config, &tool_request.user_location),
        safe_search: if config.default_safe_search { 1 } else { 0 },
    };
    let result = execute_search(root, &config, &tool_request, &query_text, &query)?;
    let context = render_results_context(&query_text, &tool_request, context_size, &result);
    Ok(ctox_web_search_payload(
        &query_text,
        &tool_request,
        context_size,
        &result,
        context,
    ))
}

pub fn run_ctox_web_read_tool(root: &Path, request: &DirectWebReadRequest) -> Result<Value> {
    let config = SearchConfig::from_root(root);
    if !config.enabled {
        return Ok(json!({
            "ok": false,
            "tool": "ctox_web_read",
            "error": "CTOX web search is disabled",
        }));
    }

    let url = normalize_text(&request.url).context("ctox_web_read requires a non-empty url")?;
    let read_query = request
        .query
        .as_deref()
        .and_then(normalize_text)
        .or_else(|| {
            request
                .find
                .first()
                .and_then(|pattern| normalize_text(pattern))
        })
        .unwrap_or_else(|| display_url(&url));
    let hit = SearchHit {
        title: display_url(&url),
        url: url.clone(),
        snippet: String::new(),
        source: if config.provider == ProviderKind::Mock {
            "mock".to_string()
        } else {
            "direct".to_string()
        },
        rank: 1,
    };
    let (doc, _) = build_evidence_doc(&config, &read_query, &hit)?;
    let mut find_results = request
        .find
        .iter()
        .filter_map(|pattern| normalize_text(pattern))
        .flat_map(|pattern| {
            build_find_in_page_results(&pattern, &doc.page_text, &doc.page_sections, &doc.excerpts)
        })
        .collect::<Vec<_>>();
    if find_results.is_empty() {
        find_results = doc.find_results.clone();
    }

    Ok(json!({
        "ok": true,
        "tool": "ctox_web_read",
        "url": url,
        "query": read_query,
        "title": doc.title,
        "summary": doc.summary,
        "is_pdf": doc.is_pdf,
        "pdf_total_pages": doc.pdf_total_pages,
        "excerpts": doc.excerpts,
        "find_results": find_results,
        "page_sections": doc.page_sections,
        "page_text_excerpt": trim_text(&doc.page_text, 4000),
        "context": render_direct_read_context(&read_query, &doc),
    }))
}

pub fn should_passthrough_openai_web_search(root: &Path, payload: &Value) -> bool {
    OpenAiWebSearchCompatMode::from_root(root) == OpenAiWebSearchCompatMode::Passthrough
        && extract_web_search_request(payload).is_some()
}

pub fn augment_responses_request(
    root: &Path,
    payload: &mut Value,
) -> Result<Option<WebSearchAugmentation>> {
    let Some(request) = canonical_web_search_request_from_responses(payload) else {
        return Ok(None);
    };
    let Some(execution) = execute_canonical_web_search(root, &request)? else {
        return Ok(None);
    };
    inject_developer_context(payload, execution.injected_context);
    strip_web_search_tools(payload);
    Ok(Some(execution.augmentation))
}

pub fn augment_responses_output(
    raw: &[u8],
    augmentation: &WebSearchAugmentation,
) -> Result<Vec<u8>> {
    let mut payload: Value =
        serde_json::from_slice(raw).context("failed to parse responses payload for web search")?;

    let Some(object) = payload.as_object_mut() else {
        return Ok(raw.to_vec());
    };
    let output = object
        .entry("output".to_string())
        .or_insert_with(|| Value::Array(Vec::new()));
    if let Some(items) = output.as_array_mut() {
        for item in augmentation
            .output_items()
            .collect::<Vec<_>>()
            .into_iter()
            .rev()
        {
            items.insert(0, item);
        }
        add_url_citations(items, &augmentation.citations);
    }

    serde_json::to_vec(&payload).context("failed to encode web-search-augmented responses payload")
}

fn ctox_web_search_payload(
    query_text: &str,
    tool_request: &SearchToolRequest,
    context_size: ContextSize,
    result: &SearchResponse,
    context: String,
) -> Value {
    let results = result
        .hits
        .iter()
        .map(|hit| {
            let evidence = find_matching_evidence_doc(&result.evidence, &hit.url);
            json!({
                "title": hit.title,
                "url": hit.url,
                "snippet": hit.snippet,
                "source": hit.source,
                "rank": hit.rank,
                "summary": evidence.map(|doc| doc.summary.clone()),
                "excerpts": evidence.map(|doc| doc.excerpts.clone()).unwrap_or_default(),
                "find_results": evidence.map(|doc| doc.find_results.clone()).unwrap_or_default(),
                "is_pdf": evidence.map(|doc| doc.is_pdf).unwrap_or(false),
                "pdf_total_pages": evidence.and_then(|doc| doc.pdf_total_pages),
            })
        })
        .collect::<Vec<_>>();
    json!({
        "ok": true,
        "tool": "ctox_web_search",
        "query": query_text,
        "provider": result.provider,
        "search_context_size": context_size.as_str(),
        "external_web_access": tool_request.external_web_access,
        "allowed_domains": tool_request.allowed_domains,
        "executed_queries": result.executed_queries,
        "results": results,
        "citations": result
            .hits
            .iter()
            .take(3)
            .map(|hit| json!({ "title": hit.title, "url": hit.url }))
            .collect::<Vec<_>>(),
        "context": context,
    })
}

fn find_matching_evidence_doc<'a>(
    docs: &'a [EvidenceDoc],
    raw_url: &str,
) -> Option<&'a EvidenceDoc> {
    let normalized = normalize_url_cache_key(raw_url);
    docs.iter()
        .find(|doc| normalize_url_cache_key(&doc.url) == normalized)
}

fn render_direct_read_context(query: &str, doc: &EvidenceDoc) -> String {
    let mut lines = vec![
        format!("CTOX opened a source page for: {query}"),
        format!("Title: {}", doc.title),
        format!("URL: {}", doc.url),
        format!("Summary: {}", doc.summary),
    ];
    if !doc.excerpts.is_empty() {
        lines.push("Key excerpts:".to_string());
        lines.extend(
            doc.excerpts
                .iter()
                .take(3)
                .map(|excerpt| format!("- {excerpt}")),
        );
    }
    lines.join("\n")
}

fn execute_search(
    root: &Path,
    config: &SearchConfig,
    tool_request: &SearchToolRequest,
    original_query: &str,
    query: &SearchQuery,
) -> Result<SearchResponse> {
    let planned_queries = plan_search_queries(original_query, &tool_request.allowed_domains);
    let cache_key = build_cache_key(query, tool_request);
    if tool_request.external_web_access == Some(false) {
        let cached = load_cached_search(root, config, &cache_key)?
            .context("cached web search was requested but no unexpired cached result exists")?;
        let hits = filter_hits_by_domain(cached.hits, &tool_request.allowed_domains);
        let evidence = filter_evidence_by_domain(cached.evidence, &tool_request.allowed_domains);
        return Ok(SearchResponse {
            provider: format!("{}-cached", cached.provider),
            evidence,
            hits,
            executed_queries: planned_queries,
        });
    }

    let mut response = search_with_query_plan(root, config, query, &planned_queries)?;
    response.hits = filter_hits_by_domain(response.hits, &tool_request.allowed_domains);
    let mut session = WebSearchSession::new(root, config)?;
    response.evidence = session.fetch_evidence(
        &query.text,
        &response.hits,
        tool_request
            .search_context_size
            .unwrap_or(ContextSize::Medium),
    );
    session.persist_page_cache()?;
    write_cached_search(root, &cache_key, &response)?;
    Ok(response)
}

fn search_with_query_plan(
    root: &Path,
    config: &SearchConfig,
    base_query: &SearchQuery,
    planned_queries: &[String],
) -> Result<SearchResponse> {
    let mut merged_hits = Vec::new();
    let mut executed_queries = Vec::new();
    let mut providers = Vec::new();
    let auto_provider = config.provider == ProviderKind::Auto;
    let provider_candidates = search_provider_candidates(root, config.provider);
    let provider_budget = auto_provider_budget(root, config.provider);
    let mut provider_cooldown_until: BTreeMap<ProviderKind, SystemTime> = BTreeMap::new();
    let mut failures = Vec::new();

    for query_text in planned_queries {
        let mut query = base_query.clone();
        query.text = query_text.clone();
        let mut accepted_response = None;
        let mut attempted_providers = 0usize;
        for provider in &provider_candidates {
            if auto_provider {
                if let Some(until) = provider_cooldown_until.get(provider) {
                    if SystemTime::now() < *until {
                        failures.push(format!("{}: skipped after rate limit", provider.as_str()));
                        continue;
                    }
                }
                if attempted_providers >= provider_budget {
                    failures.push(format!("provider budget exhausted for {}", query.text));
                    break;
                }
                attempted_providers += 1;
            }
            let response = match run_search_provider(root, config, &query, *provider) {
                Ok(response) => response,
                Err(err) if auto_provider => {
                    if is_rate_limit_error(&err) {
                        provider_cooldown_until
                            .insert(*provider, SystemTime::now() + Duration::from_secs(60));
                    }
                    failures.push(format!("{}: {err:#}", provider.as_str()));
                    continue;
                }
                Err(err) => return Err(err),
            };
            if auto_provider && !search_response_quality_ok(&query.text, &response.hits) {
                failures.push(format!(
                    "{}: low relevance for {}",
                    provider.as_str(),
                    query.text
                ));
                continue;
            }
            accepted_response = Some(response);
            break;
        }

        let Some(response) = accepted_response else {
            executed_queries.push(query.text.clone());
            continue;
        };
        if !providers.contains(&response.provider) {
            providers.push(response.provider.clone());
        }
        executed_queries.push(query.text.clone());
        for hit in response.hits {
            if merged_hits
                .iter()
                .any(|seen: &SearchHit| seen.url == hit.url)
            {
                continue;
            }
            merged_hits.push(hit);
            if merged_hits.len() >= base_query.count.max(1) {
                break;
            }
        }
        if merged_hits.len() >= base_query.count.max(1) {
            break;
        }
    }

    if merged_hits.is_empty() && !failures.is_empty() {
        bail!(
            "web search failed to produce relevant results; provider attempts: {}",
            failures.join("; ")
        );
    }

    for (index, hit) in merged_hits.iter_mut().enumerate() {
        hit.rank = index + 1;
    }

    Ok(SearchResponse {
        provider: if providers.is_empty() {
            resolve_effective_provider(root, config.provider)
                .as_str()
                .to_string()
        } else {
            providers.join("+")
        },
        hits: merged_hits,
        evidence: Vec::new(),
        executed_queries,
    })
}

fn search_provider_candidates(root: &Path, provider: ProviderKind) -> Vec<ProviderKind> {
    if provider != ProviderKind::Auto {
        return vec![provider];
    }
    let _ = root;
    vec![
        ProviderKind::Google,
        ProviderKind::Brave,
        ProviderKind::DuckDuckGo,
        ProviderKind::Bing,
    ]
}

fn auto_provider_budget(root: &Path, provider: ProviderKind) -> usize {
    if provider != ProviderKind::Auto {
        return usize::MAX;
    }
    runtime_config::get(root, "CTOX_WEB_AUTO_PROVIDER_BUDGET")
        .and_then(|raw| raw.trim().parse::<usize>().ok())
        .filter(|value| *value > 0)
        .unwrap_or(4)
}

fn is_rate_limit_error(err: &anyhow::Error) -> bool {
    let text = format!("{err:#}").to_ascii_lowercase();
    text.contains("429") || text.contains("too many requests") || text.contains("rate limit")
}

fn run_search_provider(
    root: &Path,
    config: &SearchConfig,
    query: &SearchQuery,
    provider: ProviderKind,
) -> Result<SearchResponse> {
    match provider {
        ProviderKind::Auto => unreachable!("auto provider must be expanded before execution"),
        ProviderKind::Google => google_search(root, config, query),
        ProviderKind::Brave => brave_search(config, query),
        ProviderKind::DuckDuckGo => duckduckgo_search(config, query),
        ProviderKind::Bing => bing_search(config, query),
        ProviderKind::Searxng => searxng_search(config, query),
        ProviderKind::AnnasArchive => annas_archive_search_as_web(root, query),
        ProviderKind::Mock => Ok(mock_search(query)),
    }
}

impl<'a> WebSearchSession<'a> {
    fn new(root: &'a Path, config: &'a SearchConfig) -> Result<Self> {
        Ok(Self {
            root,
            config,
            request_docs: BTreeMap::new(),
            page_cache: load_page_cache(root)?,
            page_cache_dirty: false,
        })
    }

    fn fetch_evidence(
        &mut self,
        query: &str,
        hits: &[SearchHit],
        context_size: ContextSize,
    ) -> Vec<EvidenceDoc> {
        hits.iter()
            .take(context_size.evidence_docs())
            .filter_map(|hit| self.fetch_evidence_doc(query, hit).ok())
            .collect()
    }

    fn fetch_evidence_doc(&mut self, query: &str, hit: &SearchHit) -> Result<EvidenceDoc> {
        let original_key = normalize_url_cache_key(&hit.url);
        if let Some(doc) = self.request_docs.get(&original_key).cloned() {
            return Ok(doc);
        }
        if let Some(cached_doc) = self.load_cached_page_doc(&hit.url) {
            let doc = rebuild_cached_evidence_doc(self.config, query, hit, &cached_doc);
            if !cached_pdf_doc_needs_refresh(query, &doc, self.config.max_pdf_pages) {
                self.memoize_doc_aliases([hit.url.as_str(), doc.url.as_str()], &doc);
                return Ok(doc);
            }
        }

        let (doc, content_type) = build_evidence_doc(self.config, query, hit)?;
        let canonical_url = doc.url.clone();
        self.memoize_doc_aliases([hit.url.as_str(), canonical_url.as_str()], &doc);
        self.store_page_doc(&hit.url, &canonical_url, content_type, &doc);
        Ok(doc)
    }

    fn load_cached_page_doc(&mut self, url: &str) -> Option<EvidenceDoc> {
        let key = normalize_url_cache_key(url);
        let entry = self.page_cache.entries.get(&key)?.clone();
        if unix_ts().saturating_sub(entry.created_at_epoch) > self.config.page_cache_ttl_secs {
            self.page_cache.entries.remove(&key);
            self.page_cache_dirty = true;
            return None;
        }
        Some(entry.doc)
    }

    fn memoize_doc_aliases<'b, I>(&mut self, urls: I, doc: &EvidenceDoc)
    where
        I: IntoIterator<Item = &'b str>,
    {
        for url in urls {
            let key = normalize_url_cache_key(url);
            if !key.is_empty() {
                self.request_docs.insert(key, doc.clone());
            }
        }
    }

    fn store_page_doc(
        &mut self,
        original_url: &str,
        final_url: &str,
        content_type: Option<String>,
        doc: &EvidenceDoc,
    ) {
        let entry = PageCacheEntry {
            created_at_epoch: unix_ts(),
            original_url: original_url.to_string(),
            final_url: final_url.to_string(),
            content_type,
            doc: doc.clone(),
        };

        for url in [original_url, final_url] {
            let key = normalize_url_cache_key(url);
            if !key.is_empty() {
                self.page_cache.entries.insert(key, entry.clone());
            }
        }
        self.page_cache_dirty = true;
    }

    fn persist_page_cache(&mut self) -> Result<()> {
        if !self.page_cache_dirty {
            return Ok(());
        }
        prune_expired_page_cache(&mut self.page_cache, self.config.page_cache_ttl_secs);
        write_page_cache(self.root, &self.page_cache)
    }
}
fn bing_search(config: &SearchConfig, query: &SearchQuery) -> Result<SearchResponse> {
    let mut url = Url::parse("https://www.bing.com/search").expect("static bing URL");
    {
        let mut qp = url.query_pairs_mut();
        qp.append_pair("q", &query.text);
        qp.append_pair("format", "rss");
        qp.append_pair("first", &(query.offset + 1).to_string());
        if let Some(language) = query.language.as_deref() {
            qp.append_pair("setlang", language);
        }
        if let Some(region) = query.region.as_deref() {
            qp.append_pair("cc", region);
        }
        if query.safe_search > 0 {
            qp.append_pair(
                "adlt",
                if query.safe_search > 1 {
                    "strict"
                } else {
                    "moderate"
                },
            );
        }
    }

    let response = build_agent(config)?
        .get(url.as_str())
        .set(
            "accept",
            "application/rss+xml,application/xml;q=0.9,text/xml;q=0.8,*/*;q=0.2",
        )
        .set("accept-language", "en-US,en;q=0.9")
        .call()
        .context("failed to query Bing search endpoint")?;
    let body = response
        .into_string()
        .context("failed to read Bing search response")?;
    let hits = parse_bing_rss_results(&body, query.offset, query.count)?;
    Ok(SearchResponse {
        provider: ProviderKind::Bing.as_str().to_string(),
        hits,
        evidence: Vec::new(),
        executed_queries: vec![query.text.clone()],
    })
}

fn resolve_effective_provider(root: &Path, provider: ProviderKind) -> ProviderKind {
    if provider != ProviderKind::Auto {
        return provider;
    }
    let _ = root;
    ProviderKind::Brave
}

fn brave_search(config: &SearchConfig, query: &SearchQuery) -> Result<SearchResponse> {
    let mut url = Url::parse("https://search.brave.com/search").expect("static Brave URL");
    {
        let mut qp = url.query_pairs_mut();
        qp.append_pair("q", &query.text);
        if let Some(language) = query.language.as_deref() {
            qp.append_pair("lang", language);
        }
        if let Some(region) = query.region.as_deref() {
            qp.append_pair("country", region);
        }
        qp.append_pair(
            "safesearch",
            if query.safe_search > 0 {
                "moderate"
            } else {
                "off"
            },
        );
    }

    let response = build_agent(config)?
        .get(url.as_str())
        .set(
            "accept",
            "text/html,application/xhtml+xml,application/xml;q=0.9,*/*;q=0.8",
        )
        .set("accept-language", "en-US,en;q=0.9")
        .call()
        .context("failed to query Brave search endpoint")?;
    let body = response
        .into_string()
        .context("failed to read Brave search response")?;
    let hits = parse_brave_html_results(&body, query.offset, query.count)?;
    Ok(SearchResponse {
        provider: ProviderKind::Brave.as_str().to_string(),
        hits,
        evidence: Vec::new(),
        executed_queries: vec![query.text.clone()],
    })
}

fn parse_brave_html_results(
    body: &str,
    absolute_offset: usize,
    max_count: usize,
) -> Result<Vec<SearchHit>> {
    let re = Regex::new(
        r#"(?s)title:"((?:\\.|[^"\\])*)".{0,1500}?url:"((?:\\.|[^"\\])*)".{0,1500}?description:(?:"((?:\\.|[^"\\])*)"|void 0|null)"#,
    )
    .context("invalid Brave result parser")?;
    let mut hits = Vec::new();
    for captures in re.captures_iter(body) {
        if hits.len() >= max_count.max(1) {
            break;
        }
        let title = decode_js_search_string(captures.get(1).map(|m| m.as_str()).unwrap_or(""));
        let url = decode_js_search_string(captures.get(2).map(|m| m.as_str()).unwrap_or(""));
        let snippet = decode_js_search_string(captures.get(3).map(|m| m.as_str()).unwrap_or(""));
        if title.trim().is_empty()
            || !url.starts_with("http")
            || is_search_noise_url(&url)
            || hits.iter().any(|hit: &SearchHit| hit.url == url)
        {
            continue;
        }
        hits.push(SearchHit {
            title: normalize_ws(&title),
            url,
            snippet: normalize_ws(&strip_html_tags(&snippet)),
            source: "brave".to_string(),
            rank: absolute_offset + hits.len() + 1,
        });
    }
    Ok(hits)
}

fn decode_js_search_string(raw: &str) -> String {
    if raw.is_empty() {
        return String::new();
    }
    let json_string = format!("\"{}\"", raw.replace('\n', "\\n"));
    let decoded = serde_json::from_str::<String>(&json_string).unwrap_or_else(|_| {
        raw.replace("\\/", "/")
            .replace("\\u002F", "/")
            .replace("\\\"", "\"")
            .replace("\\n", " ")
    });
    decode_xml_entities(&decoded)
}

fn strip_html_tags(input: &str) -> String {
    static TAG_RE: OnceLock<Regex> = OnceLock::new();
    let re = TAG_RE.get_or_init(|| Regex::new(r"<[^>]+>").expect("static tag regex"));
    re.replace_all(input, " ").to_string()
}

fn is_search_noise_url(raw_url: &str) -> bool {
    let Ok(url) = Url::parse(raw_url) else {
        return true;
    };
    let host = url.host_str().unwrap_or_default().to_ascii_lowercase();
    host.ends_with("brave.com")
        || host.ends_with("duckduckgo.com")
        || host.ends_with("bing.com")
        || host.ends_with("google.com")
}

fn search_response_quality_ok(query: &str, hits: &[SearchHit]) -> bool {
    if hits.is_empty() {
        return false;
    }
    let terms = significant_terms_with_numbers(query)
        .into_iter()
        .filter(|term| term.len() >= 3)
        .collect::<Vec<_>>();
    if terms.len() < 2 {
        return true;
    }
    hits.iter()
        .take(5)
        .any(|hit| score_search_hit_for_query(&terms, hit) >= 2)
}

fn score_search_hit_for_query(terms: &[String], hit: &SearchHit) -> usize {
    let haystack = format!("{} {} {}", hit.title, hit.url, hit.snippet).to_ascii_lowercase();
    terms
        .iter()
        .filter(|term| haystack.contains(term.as_str()))
        .count()
}

fn duckduckgo_search(config: &SearchConfig, query: &SearchQuery) -> Result<SearchResponse> {
    let mut url =
        Url::parse("https://html.duckduckgo.com/html/").expect("static DuckDuckGo HTML URL");
    {
        let mut qp = url.query_pairs_mut();
        qp.append_pair("q", &query.text);
        if let Some(region) = query.region.as_deref() {
            qp.append_pair("kl", &region.to_ascii_lowercase());
        }
        if query.safe_search > 0 {
            qp.append_pair("kp", if query.safe_search > 1 { "1" } else { "-1" });
        } else {
            qp.append_pair("kp", "-2");
        }
    }

    let chrome_major = parse_chrome_major_from_ua(&config.user_agent).unwrap_or(136);
    let sec_ch_ua = format!(
        "\"Chromium\";v=\"{0}\", \"Google Chrome\";v=\"{0}\", \"Not_A Brand\";v=\"99\"",
        chrome_major
    );
    let sec_ch_ua_platform = if cfg!(target_os = "macos") {
        "\"macOS\""
    } else if cfg!(target_os = "windows") {
        "\"Windows\""
    } else {
        "\"Linux\""
    };
    let accept_language = duckduckgo_accept_language(query.language.as_deref());

    let response = build_agent(config)?
        .get(url.as_str())
        .set(
            "accept",
            "text/html,application/xhtml+xml,application/xml;q=0.9,*/*;q=0.8",
        )
        .set("accept-language", &accept_language)
        .set("sec-ch-ua", &sec_ch_ua)
        .set("sec-ch-ua-mobile", "?0")
        .set("sec-ch-ua-platform", sec_ch_ua_platform)
        .set("sec-fetch-dest", "document")
        .set("sec-fetch-mode", "navigate")
        .set("sec-fetch-site", "same-site")
        .set("sec-fetch-user", "?1")
        .set("upgrade-insecure-requests", "1")
        .set("referer", "https://duckduckgo.com/")
        .call()
        .context("failed to query DuckDuckGo HTML endpoint")?;
    let body = response
        .into_string()
        .context("failed to read DuckDuckGo search response")?;
    if detect_duckduckgo_anomaly(&body) {
        bail!("DuckDuckGo returned an anti-bot interstitial");
    }
    let hits = parse_duckduckgo_html_results(&body, query.offset, query.count)?;
    Ok(SearchResponse {
        provider: ProviderKind::DuckDuckGo.as_str().to_string(),
        hits,
        evidence: Vec::new(),
        executed_queries: vec![query.text.clone()],
    })
}

fn parse_chrome_major_from_ua(user_agent: &str) -> Option<u16> {
    let marker = "Chrome/";
    let start = user_agent.find(marker)? + marker.len();
    let version = &user_agent[start..];
    version.split('.').next()?.trim().parse::<u16>().ok()
}

fn duckduckgo_accept_language(language: Option<&str>) -> String {
    let primary = match language {
        Some(value) if !value.trim().is_empty() => value.trim().to_string(),
        _ => return "en-US,en;q=0.9".to_string(),
    };
    let base = primary
        .split('-')
        .next()
        .unwrap_or(&primary)
        .to_ascii_lowercase();
    if base.eq_ignore_ascii_case(&primary) {
        format!("{primary},en;q=0.8")
    } else {
        format!("{primary},{base};q=0.9,en;q=0.8")
    }
}

fn detect_duckduckgo_anomaly(body: &str) -> bool {
    body.contains("anomaly-modal") || body.contains("Unfortunately, bots use DuckDuckGo")
}

fn parse_duckduckgo_html_results(
    body: &str,
    absolute_offset: usize,
    max_count: usize,
) -> Result<Vec<SearchHit>> {
    let html = Html::parse_document(body);
    let result_selector = Selector::parse(".result")
        .map_err(|err| anyhow!("invalid DuckDuckGo result selector: {err}"))?;
    let link_selector = Selector::parse("a.result__a, a.result-link, a[href]")
        .map_err(|err| anyhow!("invalid DuckDuckGo link selector: {err}"))?;
    let snippet_selector = Selector::parse(".result__snippet, .result-snippet")
        .map_err(|err| anyhow!("invalid DuckDuckGo snippet selector: {err}"))?;

    let mut hits = Vec::new();
    for result in html.select(&result_selector) {
        if hits.len() >= max_count.max(1) {
            break;
        }
        let Some(link) = result.select(&link_selector).find_map(|candidate| {
            candidate
                .value()
                .attr("href")
                .and_then(resolve_duckduckgo_result_url)
                .map(|url| (candidate, url))
        }) else {
            continue;
        };
        let (anchor, url) = link;
        if hits.iter().any(|hit: &SearchHit| hit.url == url) {
            continue;
        }
        let title = normalize_ws(&anchor.text().collect::<Vec<_>>().join(" "));
        let snippet = result
            .select(&snippet_selector)
            .next()
            .map(|node| normalize_ws(&node.text().collect::<Vec<_>>().join(" ")))
            .unwrap_or_default();
        hits.push(SearchHit {
            title: if title.is_empty() {
                display_url(&url)
            } else {
                title
            },
            url,
            snippet,
            source: "duckduckgo".to_string(),
            rank: absolute_offset + hits.len() + 1,
        });
    }
    Ok(hits)
}

fn resolve_duckduckgo_result_url(raw: &str) -> Option<String> {
    let trimmed = raw.trim();
    if trimmed.starts_with("http://") || trimmed.starts_with("https://") {
        return Some(trimmed.to_string());
    }
    let duckduckgo_url = if trimmed.starts_with("//") {
        format!("https:{trimmed}")
    } else {
        format!("https://duckduckgo.com{trimmed}")
    };
    let url = Url::parse(&duckduckgo_url).ok()?;
    if !url.path().contains("/l/") {
        return None;
    }
    let uddg = url
        .query_pairs()
        .find_map(|(key, value)| (key == "uddg").then(|| value.into_owned()))?;
    if uddg.starts_with("http://") || uddg.starts_with("https://") {
        Some(uddg)
    } else {
        None
    }
}

fn parse_bing_rss_results(
    body: &str,
    absolute_offset: usize,
    max_count: usize,
) -> Result<Vec<SearchHit>> {
    let doc = Document::parse(body).context("failed to parse Bing RSS payload")?;
    let mut hits = Vec::new();
    for item in doc.descendants().filter(|node| node.has_tag_name("item")) {
        if hits.len() >= max_count.max(1) {
            break;
        }
        let title = item
            .children()
            .find(|child| child.has_tag_name("title"))
            .and_then(|child| child.text())
            .map(normalize_ws)
            .unwrap_or_default();
        let url = item
            .children()
            .find(|child| child.has_tag_name("link"))
            .and_then(|child| child.text())
            .map(str::trim)
            .unwrap_or_default()
            .to_string();
        let snippet = item
            .children()
            .find(|child| child.has_tag_name("description"))
            .and_then(|child| child.text())
            .map(normalize_ws)
            .unwrap_or_default();
        if url.is_empty()
            || !url.starts_with("http")
            || hits.iter().any(|hit: &SearchHit| hit.url == url)
        {
            continue;
        }
        hits.push(SearchHit {
            title: if title.is_empty() {
                display_url(&url)
            } else {
                title
            },
            url,
            snippet,
            source: "bing".to_string(),
            rank: absolute_offset + hits.len() + 1,
        });
    }
    Ok(hits)
}

fn searxng_search(config: &SearchConfig, query: &SearchQuery) -> Result<SearchResponse> {
    let base_url = config
        .searxng_base_url
        .as_deref()
        .context("CTOX_WEB_SEARCH_SEARXNG_BASE_URL is required for searxng provider")?;
    let page_no = (query.offset / query.count.max(1)) + 1;
    let mut url = Url::parse(&format!("{}/search", base_url.trim_end_matches('/')))
        .with_context(|| format!("invalid SearXNG base URL: {}", base_url))?;
    {
        let mut qp = url.query_pairs_mut();
        qp.append_pair("q", &query.text);
        qp.append_pair("format", "json");
        qp.append_pair("categories", "general");
        qp.append_pair("pageno", &page_no.to_string());
        if let Some(language) = query.language.as_deref() {
            qp.append_pair("language", language);
        }
    }

    let response = build_agent(config)?
        .get(url.as_str())
        .call()
        .context("failed to query SearXNG")?;
    let payload: Value = serde_json::from_reader(response.into_reader())
        .context("failed to decode SearXNG search response")?;

    let hits = payload
        .get("results")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .skip(query.offset)
        .take(query.count)
        .enumerate()
        .filter_map(|(idx, item)| {
            let url = item.get("url").and_then(Value::as_str)?.to_string();
            Some(SearchHit {
                title: item
                    .get("title")
                    .and_then(Value::as_str)
                    .filter(|text| !text.trim().is_empty())
                    .unwrap_or(url.as_str())
                    .to_string(),
                snippet: item
                    .get("content")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .to_string(),
                source: item
                    .get("engine")
                    .and_then(Value::as_str)
                    .unwrap_or("searxng")
                    .to_string(),
                rank: query.offset + idx + 1,
                url,
            })
        })
        .collect();

    Ok(SearchResponse {
        provider: ProviderKind::Searxng.as_str().to_string(),
        hits,
        evidence: Vec::new(),
        executed_queries: vec![query.text.clone()],
    })
}

fn google_search(
    root: &Path,
    config: &SearchConfig,
    query: &SearchQuery,
) -> Result<SearchResponse> {
    let reference_dir = root.join(crate::browser::DEFAULT_REFERENCE_RELATIVE_DIR);
    if !reference_dir.join("node_modules/playwright").is_dir() {
        bail!(
            "playwright_google requires Playwright in {}. Run `ctox web browser-prepare --install-reference --install-browser` first.",
            reference_dir.display()
        );
    }
    let node_path = crate::browser::find_command_on_path("node")
        .context("playwright_google requires node on PATH")?;
    let runner_source = include_str!("../assets/google_browser_runner.mjs");
    // ESM resolves 'playwright' relative to the script file, so the runner must
    // live inside the reference dir where node_modules sits.
    let runner_path = reference_dir.join(format!(
        ".ctox-google-runner-{}-{}.mjs",
        std::process::id(),
        unix_ts()
    ));
    fs::write(&runner_path, runner_source)
        .with_context(|| format!("failed to write {}", runner_path.display()))?;

    let state_dir = root.join("runtime").join("google_browser_state");
    let payload = json!({
        "query": query.text,
        "language": query.language.clone().unwrap_or_else(|| "de-DE".to_string()),
        "region": query.region.clone().unwrap_or_else(|| "DE".to_string()),
        "stateDir": state_dir.to_string_lossy(),
        "maxResults": query.count.max(1).min(20),
        "timeoutMs": config.timeout_ms.max(5_000).min(120_000),
        "headless": true,
        "userAgent": config.user_agent,
    });
    let payload_bytes = serde_json::to_vec(&payload)
        .context("failed to encode playwright_google runner payload")?;

    let mut command = Command::new(&node_path);
    command
        .current_dir(&reference_dir)
        .env(
            "PLAYWRIGHT_BROWSERS_PATH",
            crate::browser::playwright_browser_cache_dir(&reference_dir),
        )
        .arg(&runner_path)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());

    let mut child = command
        .spawn()
        .context("failed to spawn playwright_google runner")?;
    child
        .stdin
        .as_mut()
        .context("playwright_google runner stdin was not piped")?
        .write_all(&payload_bytes)
        .context("failed to write playwright_google runner payload")?;
    drop(child.stdin.take());

    let deadline =
        SystemTime::now() + Duration::from_millis(config.timeout_ms.saturating_add(15_000));
    let output = loop {
        if child
            .try_wait()
            .context("failed to poll playwright_google runner")?
            .is_some()
        {
            break child
                .wait_with_output()
                .context("failed to collect playwright_google runner output")?;
        }
        if SystemTime::now() >= deadline {
            let _ = child.kill();
            let _ = child.wait();
            let _ = fs::remove_file(&runner_path);
            bail!(
                "playwright_google runner timed out after {}ms",
                config.timeout_ms.saturating_add(15_000)
            );
        }
        std::thread::sleep(Duration::from_millis(100));
    };
    let _ = fs::remove_file(&runner_path);

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
        let detail = if !stderr.is_empty() { stderr } else { stdout };
        bail!("playwright_google runner failed: {detail}");
    }

    #[derive(Deserialize)]
    struct RunnerHit {
        title: String,
        url: String,
        #[serde(default)]
        snippet: String,
    }
    #[derive(Deserialize)]
    struct RunnerOutcome {
        #[serde(default)]
        ok: bool,
        #[serde(default)]
        results: Vec<RunnerHit>,
        #[serde(default)]
        error: Option<String>,
    }

    let outcome: RunnerOutcome = serde_json::from_slice(&output.stdout).with_context(|| {
        format!(
            "playwright_google runner produced invalid JSON: {}",
            String::from_utf8_lossy(&output.stdout)
                .chars()
                .take(400)
                .collect::<String>()
        )
    })?;
    if !outcome.ok {
        bail!(
            "playwright_google runner did not return results: {}",
            outcome.error.unwrap_or_else(|| "unknown error".to_string())
        );
    }
    let hits: Vec<SearchHit> = outcome
        .results
        .into_iter()
        .enumerate()
        .map(|(index, hit)| SearchHit {
            title: hit.title,
            url: hit.url,
            snippet: hit.snippet,
            source: ProviderKind::Google.as_str().to_string(),
            rank: query.offset + index + 1,
        })
        .collect();
    Ok(SearchResponse {
        provider: ProviderKind::Google.as_str().to_string(),
        hits,
        evidence: Vec::new(),
        executed_queries: vec![query.text.clone()],
    })
}

fn annas_archive_search_as_web(root: &Path, query: &SearchQuery) -> Result<SearchResponse> {
    let request = crate::scholarly_search::ScholarlySearchRequest {
        query: query.text.clone(),
        provider: Some(crate::scholarly_search::ScholarlySearchProvider::AnnasArchive),
        max_results: Some(query.count.max(1)),
        page: Some(query.offset / query.count.max(1) + 1),
        ..Default::default()
    };
    let response = crate::scholarly_search::execute_scholarly_search(root, &request)?;
    let hits = response
        .results
        .into_iter()
        .enumerate()
        .map(|(idx, item)| {
            let mut snippet_parts: Vec<String> = Vec::new();
            if let Some(authors) = item.authors.as_deref() {
                snippet_parts.push(authors.to_string());
            }
            if let Some(publisher) = item.publisher.as_deref() {
                snippet_parts.push(publisher.to_string());
            }
            if let Some(year) = item.year {
                snippet_parts.push(year.to_string());
            }
            if let Some(language) = item.language.as_deref() {
                snippet_parts.push(format!("lang={language}"));
            }
            if let Some(format) = item.file_format.as_deref() {
                snippet_parts.push(format.to_string());
            }
            if let Some(size) = item.file_size_label.as_deref() {
                snippet_parts.push(size.to_string());
            }
            if let Some(isbn) = item.isbn.as_deref() {
                snippet_parts.push(format!("ISBN={isbn}"));
            }
            if let Some(doi) = item.doi.as_deref() {
                snippet_parts.push(format!("DOI={doi}"));
            }
            if let Some(snippet) = item.snippet.as_deref() {
                snippet_parts.push(snippet.to_string());
            }
            SearchHit {
                title: item.title,
                url: item.detail_url,
                snippet: snippet_parts.join(" · "),
                source: "annas_archive".to_string(),
                rank: query.offset + idx + 1,
            }
        })
        .collect();
    Ok(SearchResponse {
        provider: ProviderKind::AnnasArchive.as_str().to_string(),
        hits,
        evidence: Vec::new(),
        executed_queries: vec![query.text.clone()],
    })
}

fn mock_search(query: &SearchQuery) -> SearchResponse {
    SearchResponse {
        provider: ProviderKind::Mock.as_str().to_string(),
        hits: vec![SearchHit {
            title: format!("Mock result for {}", query.text),
            url: "https://example.com/mock-result".to_string(),
            snippet: format!(
                "Synthetic result generated by CTOX for '{}' to exercise search, open_page, and find_in_page.",
                trim_text(&query.text, 80)
            ),
            source: "mock".to_string(),
            rank: 1,
        }],
        evidence: Vec::new(),
        executed_queries: vec![query.text.clone()],
    }
}

#[cfg(test)]
fn fetch_evidence_doc(config: &SearchConfig, query: &str, hit: &SearchHit) -> Result<EvidenceDoc> {
    let (doc, _) = build_evidence_doc(config, query, hit)?;
    Ok(doc)
}

fn build_evidence_doc(
    config: &SearchConfig,
    query: &str,
    hit: &SearchHit,
) -> Result<(EvidenceDoc, Option<String>)> {
    let fetched = fetch_page_content(config, query, hit)?;
    let canonical_url = canonical_page_url(hit, &fetched);
    let opened_page = if is_pdf_content(hit, &fetched) {
        extract_pdf_opened_page(config, query, hit, &fetched)?
    } else {
        extract_opened_page(query, hit, &String::from_utf8_lossy(&fetched.body))
    };
    Ok((
        build_query_evidence_doc(config, query, hit, canonical_url, opened_page),
        fetched.content_type,
    ))
}

fn build_query_evidence_doc(
    config: &SearchConfig,
    query: &str,
    hit: &SearchHit,
    canonical_url: String,
    opened_page: OpenedPage,
) -> EvidenceDoc {
    rebuild_cached_evidence_doc(
        config,
        query,
        hit,
        &EvidenceDoc {
            url: canonical_url,
            title: opened_page.title,
            summary: trim_text(&opened_page.summary, 360),
            is_pdf: opened_page.is_pdf,
            pdf_total_pages: opened_page.pdf_total_pages,
            page_sections: opened_page.page_sections,
            excerpts: opened_page.excerpts,
            page_text: opened_page.page_text,
            find_results: Vec::new(),
        },
    )
}

fn rebuild_cached_evidence_doc(
    config: &SearchConfig,
    query: &str,
    hit: &SearchHit,
    cached: &EvidenceDoc,
) -> EvidenceDoc {
    let page_text = trim_text(&cached.page_text, config.max_page_chars);
    let excerpts = if cached.is_pdf {
        best_pdf_paragraphs_for_query(query, &cached.page_sections, 3, &page_text)
    } else {
        let paragraphs = clean_candidate_paragraphs(split_plaintext_paragraphs(&page_text));
        let best = best_paragraphs_for_query(query, &paragraphs, 3);
        if best.is_empty() && is_github_url(&cached.url) && !cached.excerpts.is_empty() {
            cached.excerpts.clone()
        } else {
            best
        }
    };
    let summary = if excerpts.is_empty() {
        if cached.summary.trim().is_empty() {
            fallback_summary(hit, &page_text)
        } else {
            trim_text(&cached.summary, 360)
        }
    } else {
        trim_text(&excerpts.join(" "), 360)
    };
    let find_results =
        build_find_in_page_results(query, &page_text, &cached.page_sections, &excerpts);

    EvidenceDoc {
        url: cached.url.clone(),
        title: cached.title.clone(),
        summary,
        is_pdf: cached.is_pdf,
        pdf_total_pages: cached.pdf_total_pages,
        page_sections: cached.page_sections.clone(),
        excerpts,
        page_text,
        find_results,
    }
}

fn cached_pdf_doc_needs_refresh(query: &str, doc: &EvidenceDoc, max_pdf_pages: usize) -> bool {
    if !doc.is_pdf {
        return false;
    }
    let hinted_pages = extract_pdf_page_hints(query, doc.pdf_total_pages);
    if !hinted_pages.is_empty() {
        return hinted_pages.iter().any(|page| {
            !doc.page_sections
                .iter()
                .any(|section| section.page_number == Some(*page))
        });
    }
    !query_terms(query).is_empty()
        && doc.find_results.is_empty()
        && doc.page_sections.len() < max_pdf_pages.max(1)
}

fn canonical_page_url(hit: &SearchHit, fetched: &FetchedPageContent) -> String {
    let final_url = fetched.final_url.trim();
    if final_url.is_empty() {
        hit.url.clone()
    } else {
        final_url.to_string()
    }
}

fn fetch_page_content(
    config: &SearchConfig,
    query: &str,
    hit: &SearchHit,
) -> Result<FetchedPageContent> {
    if config.provider == ProviderKind::Mock && hit.source == "mock" {
        if hit.url.to_ascii_lowercase().ends_with(".pdf") {
            return Ok(FetchedPageContent {
                body: mock_pdf_bytes(query, hit),
                content_type: Some("application/pdf".to_string()),
                final_url: hit.url.clone(),
            });
        }
        return Ok(FetchedPageContent {
            body: mock_open_page_html(query, hit).into_bytes(),
            content_type: Some("text/html".to_string()),
            final_url: hit.url.clone(),
        });
    }

    if let Some(optimized) = fetch_platform_optimized_content(config, query, hit)? {
        return Ok(optimized);
    }

    let response = build_agent(config)?
        .get(&hit.url)
        .set(
            "accept",
            "text/html,application/xhtml+xml,application/xml,text/plain,application/pdf;q=0.9,*/*;q=0.3",
        )
        .call()
        .with_context(|| format!("failed to fetch evidence page {}", hit.url))?;
    let content_type = response.header("content-type").map(ToString::to_string);
    if content_type_is_disallowed(content_type.as_deref()) {
        return Err(anyhow!(
            "evidence page {} returned unsupported content type {:?}",
            hit.url,
            content_type
        ));
    }
    let final_url = response.get_url().to_string();
    let mut body = Vec::new();
    response
        .into_reader()
        .take(config.max_page_bytes.max(4096) as u64 + 1)
        .read_to_end(&mut body)
        .with_context(|| format!("failed to read evidence page {}", hit.url))?;
    if body.len() > config.max_page_bytes.max(4096) {
        return Err(anyhow!(
            "evidence page {} exceeded {} bytes",
            hit.url,
            config.max_page_bytes
        ));
    }
    Ok(FetchedPageContent {
        body,
        content_type,
        final_url,
    })
}

fn fetch_platform_optimized_content(
    config: &SearchConfig,
    query: &str,
    hit: &SearchHit,
) -> Result<Option<FetchedPageContent>> {
    if let Ok(Some(content)) = fetch_github_api_content(config, query, hit) {
        return Ok(Some(content));
    }
    if let Some(content) = fetch_wikipedia_extract(config, hit)? {
        return Ok(Some(content));
    }
    fetch_arxiv_abstract(config, hit)
}

fn fetch_wikipedia_extract(
    config: &SearchConfig,
    hit: &SearchHit,
) -> Result<Option<FetchedPageContent>> {
    let Ok(parsed) = Url::parse(&hit.url) else {
        return Ok(None);
    };
    let host = parsed.host_str().unwrap_or_default().to_ascii_lowercase();
    if !host.ends_with("wikipedia.org") {
        return Ok(None);
    }
    let Some(raw_title) = parsed.path().strip_prefix("/wiki/") else {
        return Ok(None);
    };
    if raw_title.is_empty() || raw_title.contains(':') {
        return Ok(None);
    }

    let title = percent_decode_lossy(raw_title);
    let api_url = format!(
        "{}://{}/w/api.php?action=query&prop=extracts&explaintext=1&exsectionformat=plain&redirects=1&format=json&formatversion=2&titles={}",
        parsed.scheme(),
        host,
        percent_encode_query_value(&title)
    );
    let response = build_agent(config)?
        .get(&api_url)
        .set("accept", "application/json")
        .call()
        .with_context(|| format!("failed to fetch wikipedia extract for {}", hit.url))?;
    let payload: Value = serde_json::from_reader(response.into_reader())
        .context("failed to decode wikipedia extract response")?;
    let page = payload
        .get("query")
        .and_then(|value| value.get("pages"))
        .and_then(Value::as_array)
        .and_then(|pages| pages.first());
    let extract = page
        .and_then(|value| value.get("extract"))
        .and_then(Value::as_str)
        .unwrap_or_default()
        .trim();
    if extract.is_empty() {
        return Ok(None);
    }
    let title = page
        .and_then(|value| value.get("title"))
        .and_then(Value::as_str)
        .filter(|value| !value.trim().is_empty())
        .unwrap_or(&title);
    let body = format!("{title}\n\n{extract}\n");
    Ok(Some(FetchedPageContent {
        body: body.into_bytes(),
        content_type: Some("text/plain".to_string()),
        final_url: hit.url.clone(),
    }))
}

fn fetch_arxiv_abstract(
    config: &SearchConfig,
    hit: &SearchHit,
) -> Result<Option<FetchedPageContent>> {
    let Ok(parsed) = Url::parse(&hit.url) else {
        return Ok(None);
    };
    let host = parsed.host_str().unwrap_or_default().to_ascii_lowercase();
    if host != "arxiv.org" && host != "www.arxiv.org" {
        return Ok(None);
    }
    let Some(raw_id) = parsed.path().strip_prefix("/abs/") else {
        return Ok(None);
    };
    let paper_id = raw_id.trim();
    if paper_id.is_empty() {
        return Ok(None);
    }

    let api_url = format!(
        "https://export.arxiv.org/api/query?id_list={}",
        percent_encode_query_value(paper_id)
    );
    let response = build_agent(config)?
        .get(&api_url)
        .set(
            "accept",
            "application/atom+xml,application/xml,text/xml;q=0.9,*/*;q=0.1",
        )
        .call()
        .with_context(|| format!("failed to fetch arXiv metadata for {}", hit.url))?;
    let xml = {
        let mut raw = String::new();
        let reader = response.into_reader();
        reader
            .take(config.max_page_bytes.max(4096) as u64)
            .read_to_string(&mut raw)
            .context("failed to read arXiv metadata response")?;
        raw
    };
    let title = extract_first_xml_tag_text(&xml, "title")
        .filter(|value| !value.eq_ignore_ascii_case("arxiv query results"))
        .unwrap_or_else(|| hit.title.clone());
    let summary = extract_first_xml_tag_text_after(&xml, "summary", "<entry")
        .or_else(|| extract_first_xml_tag_text(&xml, "summary"))
        .unwrap_or_default();
    if summary.trim().is_empty() {
        return Ok(None);
    }
    let body = format!("{title}\n\n{summary}\n");
    Ok(Some(FetchedPageContent {
        body: body.into_bytes(),
        content_type: Some("text/plain".to_string()),
        final_url: hit.url.clone(),
    }))
}

fn fetch_github_api_content(
    config: &SearchConfig,
    query: &str,
    hit: &SearchHit,
) -> Result<Option<FetchedPageContent>> {
    let Some(parts) = parse_github_url_parts(&hit.url) else {
        return Ok(None);
    };
    if !matches!(
        parts.kind,
        GithubUrlKind::RepoRoot | GithubUrlKind::Tree | GithubUrlKind::Blob
    ) {
        return Ok(None);
    }

    let (default_branch, description) =
        fetch_github_repo_metadata(config, &parts).unwrap_or_else(|_| {
            (
                parts.ref_name.clone().unwrap_or_else(|| "main".to_string()),
                String::new(),
            )
        });
    let ref_name = parts
        .ref_name
        .clone()
        .filter(|value| !value.trim().is_empty())
        .unwrap_or(default_branch);

    let payload = match parts.kind {
        GithubUrlKind::RepoRoot => {
            let entries =
                fetch_github_directory_entries(config, &parts.owner, &parts.repo, None, &ref_name)?;
            let readme = fetch_github_optional_file(
                config,
                &parts.owner,
                &parts.repo,
                &ref_name,
                select_github_readme_path(&entries).as_deref(),
            )?;
            let supplemental_paths = select_github_supplemental_paths(query, &entries, None);
            let mut supplemental_files = fetch_github_directory_summaries(
                config,
                &parts.owner,
                &parts.repo,
                &ref_name,
                query,
                &entries,
                None,
                2,
            );
            supplemental_files.extend(fetch_github_supplemental_files(
                config,
                &parts.owner,
                &parts.repo,
                &ref_name,
                &supplemental_paths,
            ));
            GithubApiPayload {
                kind: "repo_root".to_string(),
                title: format!("GitHub repo: {}/{}", parts.owner, parts.repo),
                repo: format!("{}/{}", parts.owner, parts.repo),
                path: None,
                description,
                readme,
                entries: render_github_entries(&entries),
                supplemental_files,
            }
        }
        GithubUrlKind::Tree => {
            let tree_path = parts.path.clone();
            let entries = fetch_github_directory_entries(
                config,
                &parts.owner,
                &parts.repo,
                tree_path.as_deref(),
                &ref_name,
            )?;
            let readme = fetch_github_optional_file(
                config,
                &parts.owner,
                &parts.repo,
                &ref_name,
                select_github_readme_path(&entries).as_deref(),
            )?;
            let supplemental_paths =
                select_github_supplemental_paths(query, &entries, tree_path.as_deref());
            let mut supplemental_files = fetch_github_directory_summaries(
                config,
                &parts.owner,
                &parts.repo,
                &ref_name,
                query,
                &entries,
                tree_path.as_deref(),
                2,
            );
            supplemental_files.extend(fetch_github_supplemental_files(
                config,
                &parts.owner,
                &parts.repo,
                &ref_name,
                &supplemental_paths,
            ));
            GithubApiPayload {
                kind: "tree".to_string(),
                title: format!(
                    "GitHub tree: {}",
                    tree_path
                        .clone()
                        .unwrap_or_else(|| format!("{}/{}", parts.owner, parts.repo))
                ),
                repo: format!("{}/{}", parts.owner, parts.repo),
                path: tree_path,
                description,
                readme,
                entries: render_github_entries(&entries),
                supplemental_files,
            }
        }
        GithubUrlKind::Blob => {
            let path = parts.path.clone().context("github blob URL missing path")?;
            let text = fetch_github_file_text(config, &parts.owner, &parts.repo, &ref_name, &path)?;
            GithubApiPayload {
                kind: "blob".to_string(),
                title: format!("GitHub file: {path}"),
                repo: format!("{}/{}", parts.owner, parts.repo),
                path: Some(path.clone()),
                description,
                readme: String::new(),
                entries: Vec::new(),
                supplemental_files: vec![GithubApiFile { path, text }],
            }
        }
        GithubUrlKind::Other => return Ok(None),
    };

    Ok(Some(FetchedPageContent {
        body: serde_json::to_vec(&payload).context("failed to encode GitHub API payload")?,
        content_type: Some(GITHUB_API_CONTENT_TYPE.to_string()),
        final_url: hit.url.clone(),
    }))
}

fn parse_github_url_parts(raw: &str) -> Option<GithubUrlParts> {
    let parsed = Url::parse(raw).ok()?;
    if !parsed
        .host_str()
        .is_some_and(|host| host.eq_ignore_ascii_case("github.com"))
    {
        return None;
    }
    let segments = parsed
        .path_segments()?
        .filter(|segment| !segment.is_empty())
        .collect::<Vec<_>>();
    if segments.len() < 2 {
        return None;
    }

    let owner = segments[0].to_string();
    let repo = segments[1].trim_end_matches(".git").to_string();
    let (kind, ref_name, path) = match segments.get(2).copied() {
        None => (GithubUrlKind::RepoRoot, None, None),
        Some("tree") => (
            GithubUrlKind::Tree,
            segments.get(3).map(|value| (*value).to_string()),
            if segments.len() > 4 {
                Some(segments[4..].join("/"))
            } else {
                None
            },
        ),
        Some("blob") => (
            GithubUrlKind::Blob,
            segments.get(3).map(|value| (*value).to_string()),
            if segments.len() > 4 {
                Some(segments[4..].join("/"))
            } else {
                None
            },
        ),
        _ => (GithubUrlKind::Other, None, None),
    };

    Some(GithubUrlParts {
        owner,
        repo,
        kind,
        ref_name,
        path,
    })
}

fn fetch_github_repo_metadata(
    config: &SearchConfig,
    parts: &GithubUrlParts,
) -> Result<(String, String)> {
    let url = format!("{}/repos/{}/{}", GITHUB_API_BASE, parts.owner, parts.repo);
    let payload = github_api_get_json(config, &url)?;
    let default_branch = payload
        .get("default_branch")
        .and_then(Value::as_str)
        .filter(|value| !value.trim().is_empty())
        .unwrap_or("main")
        .to_string();
    let description = payload
        .get("description")
        .and_then(Value::as_str)
        .map(normalize_ws)
        .unwrap_or_default();
    Ok((default_branch, description))
}

fn fetch_github_directory_entries(
    config: &SearchConfig,
    owner: &str,
    repo: &str,
    path: Option<&str>,
    ref_name: &str,
) -> Result<Vec<GithubContentEntry>> {
    fetch_github_directory_entries_via_api(config, owner, repo, path, ref_name)
        .or_else(|_| fetch_github_directory_entries_via_html(config, owner, repo, path, ref_name))
}

fn fetch_github_directory_entries_via_api(
    config: &SearchConfig,
    owner: &str,
    repo: &str,
    path: Option<&str>,
    ref_name: &str,
) -> Result<Vec<GithubContentEntry>> {
    let path_suffix = path
        .filter(|value| !value.trim().is_empty())
        .map(|value| format!("/{}", encode_url_path_segments(value)))
        .unwrap_or_default();
    let url = format!(
        "{}/repos/{}/{}/contents{}?ref={}",
        GITHUB_API_BASE,
        owner,
        repo,
        path_suffix,
        percent_encode_query_value(ref_name)
    );
    let payload = github_api_get_json(config, &url)?;
    let Some(items) = payload.as_array() else {
        return Ok(Vec::new());
    };
    Ok(items
        .iter()
        .filter_map(|item| {
            Some(GithubContentEntry {
                name: item.get("name").and_then(Value::as_str)?.to_string(),
                path: item.get("path").and_then(Value::as_str)?.to_string(),
                kind: item
                    .get("type")
                    .and_then(Value::as_str)
                    .unwrap_or("file")
                    .to_string(),
            })
        })
        .collect())
}

fn fetch_github_directory_entries_via_html(
    config: &SearchConfig,
    owner: &str,
    repo: &str,
    path: Option<&str>,
    ref_name: &str,
) -> Result<Vec<GithubContentEntry>> {
    let url = match path.filter(|value| !value.trim().is_empty()) {
        Some(path) => format!(
            "https://github.com/{}/{}/tree/{}/{}",
            owner,
            repo,
            ref_name,
            encode_url_path_segments(path)
        ),
        None => format!("https://github.com/{}/{}", owner, repo),
    };
    let response = build_agent(config)?
        .get(&url)
        .set(
            "accept",
            "text/html,application/xhtml+xml,application/xml;q=0.9,*/*;q=0.8",
        )
        .call()
        .with_context(|| format!("failed to fetch GitHub tree page {}", url))?;
    let body = response
        .into_string()
        .with_context(|| format!("failed to read GitHub tree page {}", url))?;
    let payload =
        github_embedded_payload(&body).context("github tree page missing embedded payload")?;
    github_tree_entries_from_payload(&payload).context("github tree page missing tree items")
}

fn fetch_github_optional_file(
    config: &SearchConfig,
    owner: &str,
    repo: &str,
    ref_name: &str,
    path: Option<&str>,
) -> Result<String> {
    let Some(path) = path else {
        return Ok(String::new());
    };
    fetch_github_file_text(config, owner, repo, ref_name, path).or_else(|_| Ok(String::new()))
}

fn fetch_github_file_text(
    config: &SearchConfig,
    owner: &str,
    repo: &str,
    ref_name: &str,
    path: &str,
) -> Result<String> {
    fetch_github_file_text_via_api(config, owner, repo, ref_name, path)
        .or_else(|_| fetch_github_file_text_via_raw(config, owner, repo, ref_name, path))
}

fn fetch_github_file_text_via_api(
    config: &SearchConfig,
    owner: &str,
    repo: &str,
    ref_name: &str,
    path: &str,
) -> Result<String> {
    let encoded_path = encode_url_path_segments(path);
    let url = format!(
        "{}/repos/{}/{}/contents/{}?ref={}",
        GITHUB_API_BASE,
        owner,
        repo,
        encoded_path,
        percent_encode_query_value(ref_name)
    );
    github_fetch_text(
        config,
        &url,
        "application/vnd.github.raw+json,application/vnd.github.raw,text/plain;q=0.9,*/*;q=0.1",
    )
}

fn fetch_github_file_text_via_raw(
    config: &SearchConfig,
    owner: &str,
    repo: &str,
    ref_name: &str,
    path: &str,
) -> Result<String> {
    let url = format!(
        "https://raw.githubusercontent.com/{}/{}/{}/{}",
        owner,
        repo,
        percent_encode_query_value(ref_name),
        encode_url_path_segments(path)
    );
    github_fetch_text(config, &url, "text/plain;q=0.9,*/*;q=0.1")
}

fn fetch_github_supplemental_files(
    config: &SearchConfig,
    owner: &str,
    repo: &str,
    ref_name: &str,
    paths: &[String],
) -> Vec<GithubApiFile> {
    paths
        .iter()
        .filter_map(|path| {
            fetch_github_file_text(config, owner, repo, ref_name, path)
                .ok()
                .filter(|text| !text.trim().is_empty())
                .map(|text| GithubApiFile {
                    path: path.clone(),
                    text,
                })
        })
        .collect()
}

fn fetch_github_directory_summaries(
    config: &SearchConfig,
    owner: &str,
    repo: &str,
    ref_name: &str,
    query: &str,
    entries: &[GithubContentEntry],
    base_path: Option<&str>,
    max_depth: usize,
) -> Vec<GithubApiFile> {
    let mut files = Vec::new();
    let mut visited = Vec::new();
    for dir in select_github_directory_paths(query, entries, base_path).into_iter() {
        collect_github_directory_summary(
            config,
            owner,
            repo,
            ref_name,
            query,
            &dir,
            1,
            max_depth.max(1),
            &mut visited,
            &mut files,
        );
        if files.len() >= 4 {
            break;
        }
    }
    files
}

fn collect_github_directory_summary(
    config: &SearchConfig,
    owner: &str,
    repo: &str,
    ref_name: &str,
    query: &str,
    path: &str,
    depth: usize,
    max_depth: usize,
    visited: &mut Vec<String>,
    files: &mut Vec<GithubApiFile>,
) {
    if depth > max_depth || visited.iter().any(|seen| seen == path) || files.len() >= 4 {
        return;
    }
    visited.push(path.to_string());

    let Ok(entries) = fetch_github_directory_entries(config, owner, repo, Some(path), ref_name)
    else {
        return;
    };
    if entries.is_empty() {
        return;
    }

    let text = render_github_entries(&entries).join("\n");
    files.push(GithubApiFile {
        path: format!("{}/", path.trim_end_matches('/')),
        text,
    });

    if depth >= max_depth || files.len() >= 4 {
        return;
    }

    for child_dir in select_github_directory_paths(query, &entries, Some(path)).into_iter() {
        collect_github_directory_summary(
            config,
            owner,
            repo,
            ref_name,
            query,
            &child_dir,
            depth + 1,
            max_depth,
            visited,
            files,
        );
        if files.len() >= 4 {
            break;
        }
    }
}

fn select_github_directory_paths(
    query: &str,
    entries: &[GithubContentEntry],
    base_path: Option<&str>,
) -> Vec<String> {
    let query_terms = query_terms(query);
    let file_like = query
        .split_whitespace()
        .map(|term| term.trim_matches(|ch: char| !ch.is_alphanumeric() && ch != '.'))
        .find(|term| term.contains('.'))
        .map(|term| term.to_ascii_lowercase());
    let mut scored = entries
        .iter()
        .filter(|entry| matches!(entry.kind.as_str(), "dir" | "directory" | "tree"))
        .map(|entry| {
            let lowered = entry.path.to_ascii_lowercase();
            let mut score = 0usize;
            for term in &query_terms {
                if lowered.contains(term.as_str()) {
                    score += 220;
                }
            }
            if lowered.contains("exercise") && query.to_ascii_lowercase().contains("exercise") {
                score += 240;
            }
            if lowered.contains("intro") && query.to_ascii_lowercase().contains("intro") {
                score += 200;
            }
            if file_like.is_some() && lowered.contains("exercise") {
                score += 120;
            }
            for (marker, weight) in [
                ("exercises", 160usize),
                ("src", 120),
                ("docs", 90),
                ("examples", 80),
                ("website", 50),
            ] {
                if lowered.ends_with(marker) || lowered.contains(&format!("/{marker}")) {
                    score += weight;
                }
            }
            if base_path.is_some() {
                score += 20;
            }
            (score, entry.path.clone())
        })
        .filter(|(score, _)| *score > 0)
        .collect::<Vec<_>>();
    scored.sort_by(|left, right| right.0.cmp(&left.0).then_with(|| left.1.cmp(&right.1)));

    let mut selected = Vec::new();
    for (_, path) in scored {
        if !selected.iter().any(|seen: &String| seen == &path) {
            selected.push(path);
        }
        if selected.len() >= 2 {
            break;
        }
    }
    selected
}

fn select_github_readme_path(entries: &[GithubContentEntry]) -> Option<String> {
    entries.iter().find_map(|entry| {
        if entry.kind == "file" && entry.name.to_ascii_lowercase().starts_with("readme.") {
            Some(entry.path.clone())
        } else {
            None
        }
    })
}

fn render_github_entries(entries: &[GithubContentEntry]) -> Vec<String> {
    entries
        .iter()
        .map(|entry| format!("{}: {}", entry.kind, entry.path))
        .collect()
}

fn select_github_supplemental_paths(
    query: &str,
    entries: &[GithubContentEntry],
    base_path: Option<&str>,
) -> Vec<String> {
    let mut selected = Vec::new();
    let query_terms = query_terms(query);
    let command_oriented = is_command_oriented_github_query(query);

    let candidate_specs = [
        "README.md",
        "INSTALL.md",
        "install.sh",
        "install.ps1",
        "src/main.rs",
        "src/lib.rs",
        "Cargo.toml",
        "package.json",
    ];
    for spec in candidate_specs {
        if selected.len() >= 4 {
            break;
        }
        let wanted = prefixed_github_path(base_path, spec);
        if spec.starts_with("README") {
            continue;
        }
        if !command_oriented
            && matches!(
                spec,
                "install.sh" | "install.ps1" | "src/main.rs" | "src/lib.rs" | "Cargo.toml"
            )
        {
            continue;
        }
        if github_candidate_path_possible(entries, &wanted) && !selected.contains(&wanted) {
            selected.push(wanted);
        }
    }

    for entry in entries {
        if selected.len() >= 4 {
            break;
        }
        if entry.kind != "file" {
            continue;
        }
        let lowered = entry.path.to_ascii_lowercase();
        if selected.iter().any(|path| path == &entry.path)
            || lowered.starts_with("readme.")
            || lowered.ends_with("/readme.md")
        {
            continue;
        }
        if command_oriented && !github_path_command_relevant(&entry.path) {
            continue;
        }
        if query_terms
            .iter()
            .any(|term| lowered.contains(term.as_str()))
        {
            selected.push(entry.path.clone());
        }
    }

    selected
}

fn prefixed_github_path(base_path: Option<&str>, suffix: &str) -> String {
    let prefix = base_path
        .map(|value| value.trim_matches('/'))
        .filter(|value| !value.is_empty())
        .map(|value| format!("{value}/"))
        .unwrap_or_default();
    format!("{prefix}{suffix}")
}

fn github_entry_exists(entries: &[GithubContentEntry], wanted_path: &str) -> bool {
    entries.iter().any(|entry| entry.path == wanted_path)
}

fn github_candidate_path_possible(entries: &[GithubContentEntry], wanted_path: &str) -> bool {
    if github_entry_exists(entries, wanted_path) {
        return true;
    }
    let Some((parent, _)) = wanted_path.rsplit_once('/') else {
        return false;
    };
    entries.iter().any(|entry| {
        matches!(entry.kind.as_str(), "dir" | "directory" | "tree") && entry.path == parent
    })
}

fn is_command_oriented_github_query(query: &str) -> bool {
    let lowered = query.to_ascii_lowercase();
    [
        "start",
        "getting started",
        "get started",
        "install",
        "setup",
        "initialize",
        "initialise",
        "init",
        "run",
        "command",
        "usage",
        "cli",
        "launch",
        "begin",
    ]
    .iter()
    .any(|marker| lowered.contains(marker))
}

fn github_path_command_relevant(path: &str) -> bool {
    let lowered = path.to_ascii_lowercase();
    lowered.contains("install")
        || lowered.ends_with("cargo.toml")
        || lowered.ends_with("package.json")
        || lowered.ends_with("/main.rs")
        || lowered.ends_with("/lib.rs")
        || lowered.ends_with("/cli.rs")
        || lowered.ends_with("/main.py")
        || lowered.ends_with("/main.go")
        || lowered.ends_with("/main.ts")
}

fn encode_url_path_segments(path: &str) -> String {
    path.split('/')
        .filter(|segment| !segment.is_empty())
        .map(percent_encode_query_value)
        .collect::<Vec<_>>()
        .join("/")
}

fn github_api_get_json(config: &SearchConfig, url: &str) -> Result<Value> {
    let response = build_agent(config)?
        .get(url)
        .set(
            "accept",
            "application/vnd.github+json,application/json;q=0.9",
        )
        .set("x-github-api-version", GITHUB_API_VERSION)
        .call()
        .with_context(|| format!("failed to query GitHub API {}", url))?;
    serde_json::from_reader(response.into_reader())
        .with_context(|| format!("failed to decode GitHub API payload {}", url))
}

fn github_fetch_text(config: &SearchConfig, url: &str, accept: &str) -> Result<String> {
    let response = build_agent(config)?
        .get(url)
        .set("accept", accept)
        .set("x-github-api-version", GITHUB_API_VERSION)
        .call()
        .with_context(|| format!("failed to fetch GitHub content {}", url))?;
    let mut raw = String::new();
    response
        .into_reader()
        .take((config.max_page_bytes.max(4096) / 2).max(4096) as u64)
        .read_to_string(&mut raw)
        .with_context(|| format!("failed to read GitHub content {}", url))?;
    Ok(raw)
}

fn content_type_is_disallowed(content_type: Option<&str>) -> bool {
    let Some(content_type) = content_type else {
        return false;
    };
    let lowered = content_type.to_ascii_lowercase();
    if lowered.contains("application/pdf")
        || lowered.starts_with("text/")
        || lowered.contains("html")
        || lowered.contains("xml")
        || lowered.contains("json")
    {
        return false;
    }
    lowered.starts_with("image/")
        || lowered.starts_with("audio/")
        || lowered.starts_with("video/")
        || lowered.contains("application/zip")
        || lowered.contains("application/gzip")
        || lowered.contains("application/octet-stream")
}

fn mock_open_page_html(query: &str, hit: &SearchHit) -> String {
    let trimmed_query = trim_text(query, 120);
    format!(
        r#"<!doctype html>
<html lang="en">
  <head>
    <title>{title}</title>
  </head>
  <body>
    <header>
      <p>Home Pricing Docs Support</p>
      <p>We use cookies to improve this service.</p>
    </header>
    <main>
      <article>
        <h1>{title}</h1>
        <p>CTOX generated this synthetic page for the query "{query}". It exists so the web_search runtime can perform a real open_page step during local and remote smoke tests.</p>
        <p>The important confirmation token is CTOX_REMOTE_WEB_OK. The page repeats CTOX_REMOTE_WEB_OK so find_in_page can discover it inside opened page content.</p>
        <p>This mock page also mentions web search, source reading, and page exploration to exercise the same evidence path used for real URLs.</p>
        <p>Canonical source URL: {url}</p>
      </article>
    </main>
    <footer>
      <p>Privacy Policy Terms Cookie Settings All rights reserved.</p>
    </footer>
  </body>
</html>"#,
        title = hit.title,
        query = trimmed_query,
        url = hit.url,
    )
}

fn mock_pdf_bytes(query: &str, hit: &SearchHit) -> Vec<u8> {
    let escaped_query = pdf_escape_text(&trim_text(query, 80));
    let escaped_title = pdf_escape_text(&hit.title);
    let escaped_url = pdf_escape_text(&hit.url);
    let page_text = format!(
        "Mock PDF {} {} CTOX_REMOTE_WEB_OK source {}",
        escaped_title, escaped_query, escaped_url
    );
    mock_pdf_bytes_with_pages(&[page_text])
}

fn mock_pdf_bytes_with_pages<S: AsRef<str>>(pages: &[S]) -> Vec<u8> {
    let page_count = pages.len().max(1);
    let pages_obj_num = 2usize;
    let font_obj_num = 3usize;
    let first_page_obj_num = 4usize;

    let mut objects = Vec::new();
    objects.push(format!("<< /Type /Catalog /Pages {} 0 R >>", pages_obj_num));

    let kids = (0..page_count)
        .map(|index| format!("{} 0 R", first_page_obj_num + index * 2))
        .collect::<Vec<_>>()
        .join(" ");
    objects.push(format!(
        "<< /Type /Pages /Kids [{}] /Count {} >>",
        kids, page_count
    ));
    objects.push("<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>".to_string());

    for (index, page) in pages.iter().enumerate() {
        let page_obj_num = first_page_obj_num + index * 2;
        let contents_obj_num = page_obj_num + 1;
        let stream = format!(
            "BT /F1 12 Tf 72 720 Td ({}) Tj ET",
            pdf_escape_text(page.as_ref())
        );
        objects.push(format!(
            "<< /Type /Page /Parent {} 0 R /MediaBox [0 0 612 792] /Resources << /Font << /F1 {} 0 R >> >> /Contents {} 0 R >>",
            pages_obj_num, font_obj_num, contents_obj_num
        ));
        objects.push(format!(
            "<< /Length {} >>\nstream\n{}\nendstream",
            stream.len(),
            stream
        ));
    }

    let mut pdf = String::from("%PDF-1.4\n");
    let mut offsets = Vec::with_capacity(objects.len() + 1);
    offsets.push(0usize);

    for (index, object) in objects.iter().enumerate() {
        offsets.push(pdf.len());
        pdf.push_str(&format!("{} 0 obj\n{}\nendobj\n", index + 1, object));
    }

    let xref_offset = pdf.len();
    pdf.push_str(&format!("xref\n0 {}\n", offsets.len()));
    pdf.push_str("0000000000 65535 f \n");
    for offset in offsets.iter().skip(1) {
        pdf.push_str(&format!("{offset:010} 00000 n \n"));
    }
    pdf.push_str(&format!(
        "trailer\n<< /Root 1 0 R /Size {} >>\nstartxref\n{}\n%%EOF\n",
        offsets.len(),
        xref_offset
    ));

    pdf.into_bytes()
}

fn pdf_escape_text(input: &str) -> String {
    input
        .replace('\\', "\\\\")
        .replace('(', "\\(")
        .replace(')', "\\)")
}

fn is_pdf_content(hit: &SearchHit, fetched: &FetchedPageContent) -> bool {
    hit.url.to_ascii_lowercase().ends_with(".pdf")
        || fetched.final_url.to_ascii_lowercase().ends_with(".pdf")
        || fetched
            .content_type
            .as_deref()
            .map(|value| value.to_ascii_lowercase().contains("application/pdf"))
            .unwrap_or(false)
        || fetched.body.starts_with(b"%PDF-")
}

fn extract_opened_page(query: &str, hit: &SearchHit, body: &str) -> OpenedPage {
    match detect_page_adapter(hit, body) {
        PageAdapterKind::Github => extract_github_opened_page(query, hit, body)
            .or_else(|| extract_github_discussion_opened_page(query, hit, body))
            .unwrap_or_else(|| extract_html_opened_page(query, hit, body)),
        PageAdapterKind::KnowledgeSite => extract_knowledge_opened_page(query, hit, body)
            .unwrap_or_else(|| extract_html_opened_page(query, hit, body)),
        PageAdapterKind::DocsSite => extract_docs_opened_page(query, hit, body)
            .unwrap_or_else(|| extract_html_opened_page(query, hit, body)),
        PageAdapterKind::NewsSite => extract_news_opened_page(query, hit, body)
            .unwrap_or_else(|| extract_html_opened_page(query, hit, body)),
        PageAdapterKind::GenericHtml => extract_html_opened_page(query, hit, body),
        PageAdapterKind::PlainText => extract_text_opened_page(query, hit, body),
    }
}

fn looks_like_html(body: &str) -> bool {
    let lowered = body.to_ascii_lowercase();
    lowered.contains("<html")
        || lowered.contains("<body")
        || lowered.contains("<article")
        || lowered.contains("<main")
        || lowered.contains("<p>")
}

fn detect_page_adapter(hit: &SearchHit, body: &str) -> PageAdapterKind {
    if is_github_url(&hit.url) {
        return PageAdapterKind::Github;
    }
    if !looks_like_html(body) {
        return PageAdapterKind::PlainText;
    }
    if is_knowledge_url(&hit.url) || body_looks_like_knowledge_site(body) {
        return PageAdapterKind::KnowledgeSite;
    }
    if is_docs_url(&hit.url) || body_looks_like_docs_site(body) {
        return PageAdapterKind::DocsSite;
    }
    if is_news_url(&hit.url) || body_looks_like_news_site(body) {
        return PageAdapterKind::NewsSite;
    }
    PageAdapterKind::GenericHtml
}

fn extract_html_opened_page(query: &str, hit: &SearchHit, body: &str) -> OpenedPage {
    let doc = Html::parse_document(body);
    let title = select_text(&doc, "title, h1").unwrap_or_else(|| hit.title.clone());
    let mut paragraphs = select_relevant_html_blocks(&doc);
    if paragraphs.is_empty() {
        paragraphs.push(hit.snippet.clone());
    }
    let excerpts = best_paragraphs_for_query(query, &paragraphs, 3);
    let summary = if excerpts.is_empty() {
        hit.snippet.clone()
    } else {
        excerpts.join(" ")
    };
    let page_text = if paragraphs.is_empty() {
        summary.clone()
    } else {
        paragraphs.join("\n\n")
    };
    OpenedPage {
        title,
        summary,
        is_pdf: false,
        pdf_total_pages: None,
        page_sections: Vec::new(),
        excerpts,
        page_text,
    }
}

fn is_knowledge_url(url: &str) -> bool {
    let Ok(parsed) = Url::parse(url) else {
        return false;
    };
    let host = parsed.host_str().unwrap_or_default().to_ascii_lowercase();
    host.ends_with("wikipedia.org")
        || host == "arxiv.org"
        || host.ends_with(".arxiv.org")
        || host == "pubmed.ncbi.nlm.nih.gov"
        || host.ends_with(".pubmed.ncbi.nlm.nih.gov")
        || host == "ncbi.nlm.nih.gov"
        || host.ends_with(".ncbi.nlm.nih.gov")
}

fn body_looks_like_knowledge_site(body: &str) -> bool {
    let lowered = body.to_ascii_lowercase();
    [
        "mw-parser-output",
        "mw-content-text",
        "citation_title",
        "citation_abstract",
        "abstract mathjax",
        "pubmed",
        "scholarlyarticle",
    ]
    .iter()
    .any(|marker| lowered.contains(marker))
}

fn extract_knowledge_opened_page(query: &str, hit: &SearchHit, body: &str) -> Option<OpenedPage> {
    let doc = Html::parse_document(body);
    let title = select_text(&doc, "h1")
        .or_else(|| {
            select_attr(
                &doc,
                "meta[name='citation_title'], meta[property='og:title']",
                "content",
            )
        })
        .or_else(|| select_text(&doc, "title"))
        .unwrap_or_else(|| hit.title.clone());
    let mut paragraphs = select_knowledge_html_blocks(&doc);
    if paragraphs.is_empty() {
        paragraphs = select_relevant_html_blocks(&doc);
    }
    if paragraphs.is_empty() {
        return None;
    }

    let excerpts = best_paragraphs_for_query(query, &paragraphs, 3);
    let page_text = paragraphs.join("\n\n");
    let summary = if excerpts.is_empty() {
        fallback_summary(hit, &page_text)
    } else {
        excerpts.join(" ")
    };

    Some(OpenedPage {
        title,
        summary,
        is_pdf: false,
        pdf_total_pages: None,
        page_sections: Vec::new(),
        excerpts,
        page_text,
    })
}

fn is_docs_url(url: &str) -> bool {
    let Ok(parsed) = Url::parse(url) else {
        return false;
    };
    let host = parsed.host_str().unwrap_or_default().to_ascii_lowercase();
    host.starts_with("docs.")
        || host.contains("readthedocs.io")
        || host.contains("gitbook.io")
        || host == "docs.gitbook.com"
        || host == "docusaurus.io"
        || host == "www.mkdocs.org"
        || parsed
            .path_segments()
            .map(|segments| {
                segments
                    .take(1)
                    .any(|segment| segment.eq_ignore_ascii_case("docs"))
            })
            .unwrap_or(false)
}

fn body_looks_like_docs_site(body: &str) -> bool {
    let lowered = body.to_ascii_lowercase();
    [
        "theme-doc-markdown",
        "docusaurus",
        "content__default",
        "md-content",
        "rst-content",
        "gitbook",
        "documentation",
        "docs-content",
        "docmaincontainer",
        "vp-doc",
    ]
    .iter()
    .any(|marker| lowered.contains(marker))
}

fn extract_docs_opened_page(query: &str, hit: &SearchHit, body: &str) -> Option<OpenedPage> {
    let doc = Html::parse_document(body);
    let title = select_text(&doc, "h1, title").unwrap_or_else(|| hit.title.clone());
    let mut paragraphs = select_docs_html_blocks(&doc);
    if paragraphs.is_empty() {
        paragraphs = select_relevant_html_blocks(&doc);
    }
    if paragraphs.is_empty() {
        return None;
    }

    let excerpts = best_paragraphs_for_query(query, &paragraphs, 3);
    let page_text = paragraphs.join("\n\n");
    let summary = if excerpts.is_empty() {
        fallback_summary(hit, &page_text)
    } else {
        excerpts.join(" ")
    };

    Some(OpenedPage {
        title,
        summary,
        is_pdf: false,
        pdf_total_pages: None,
        page_sections: Vec::new(),
        excerpts,
        page_text,
    })
}

fn is_news_url(url: &str) -> bool {
    let Ok(parsed) = Url::parse(url) else {
        return false;
    };
    let host = parsed.host_str().unwrap_or_default().to_ascii_lowercase();
    host.ends_with("reuters.com")
        || host.ends_with("apnews.com")
        || host.ends_with("bbc.com")
        || host.ends_with("cnn.com")
        || host.ends_with("theverge.com")
        || host.ends_with("techcrunch.com")
        || host.ends_with("wired.com")
        || host.ends_with("nytimes.com")
        || host.ends_with("washingtonpost.com")
        || host.ends_with("theguardian.com")
}

fn body_looks_like_news_site(body: &str) -> bool {
    let lowered = body.to_ascii_lowercase();
    [
        "article:published_time",
        "property=\"og:type\" content=\"article",
        "property='og:type' content='article",
        "\"@type\":\"newsarticle\"",
        "\"@type\": \"newsarticle\"",
        "class=\"byline",
        "class=\"article-body",
        "class=\"story-body",
        "article-body",
        "story-body",
        "dateline",
    ]
    .iter()
    .any(|marker| lowered.contains(marker))
}

fn extract_news_opened_page(query: &str, hit: &SearchHit, body: &str) -> Option<OpenedPage> {
    let doc = Html::parse_document(body);
    let title = select_text(&doc, "h1")
        .or_else(|| {
            select_attr(
                &doc,
                "meta[property='og:title'], meta[name='twitter:title']",
                "content",
            )
        })
        .or_else(|| select_text(&doc, "title"))
        .unwrap_or_else(|| hit.title.clone());
    let mut paragraphs = select_news_html_blocks(&doc);
    if paragraphs.is_empty() {
        paragraphs = select_relevant_html_blocks(&doc);
    }
    if paragraphs.is_empty() {
        return None;
    }

    let excerpts = best_paragraphs_for_query(query, &paragraphs, 3);
    let page_text = paragraphs.join("\n\n");
    let summary = if excerpts.is_empty() {
        fallback_summary(hit, &page_text)
    } else {
        excerpts.join(" ")
    };

    Some(OpenedPage {
        title,
        summary,
        is_pdf: false,
        pdf_total_pages: None,
        page_sections: Vec::new(),
        excerpts,
        page_text,
    })
}

fn is_github_url(url: &str) -> bool {
    Url::parse(url)
        .ok()
        .and_then(|parsed| {
            parsed
                .host_str()
                .map(|host| host.eq_ignore_ascii_case("github.com"))
        })
        .unwrap_or(false)
}

fn extract_github_opened_page(query: &str, hit: &SearchHit, body: &str) -> Option<OpenedPage> {
    if let Some(payload) = github_api_payload(body) {
        if let Some(opened) = github_api_payload_opened_page(query, hit, &payload) {
            return Some(opened);
        }
    }

    let payload = github_embedded_payload(body)?;

    if let Some(code_blob) = github_code_blob_opened_page(query, hit, &payload) {
        return Some(code_blob);
    }

    if let Some(markdown) = github_markdown_opened_page(query, hit, &payload) {
        return Some(markdown);
    }

    if let Some(tree) = github_tree_opened_page(query, hit, &payload) {
        return Some(tree);
    }

    None
}

fn github_api_payload(body: &str) -> Option<GithubApiPayload> {
    let payload: GithubApiPayload = serde_json::from_str(body).ok()?;
    if matches!(payload.kind.as_str(), "repo_root" | "tree" | "blob") {
        Some(payload)
    } else {
        None
    }
}

fn github_api_payload_opened_page(
    query: &str,
    hit: &SearchHit,
    payload: &GithubApiPayload,
) -> Option<OpenedPage> {
    match payload.kind.as_str() {
        "blob" => github_api_blob_opened_page(query, hit, payload),
        "repo_root" | "tree" => github_api_repo_opened_page(query, hit, payload),
        _ => None,
    }
}

fn github_api_blob_opened_page(
    query: &str,
    hit: &SearchHit,
    payload: &GithubApiPayload,
) -> Option<OpenedPage> {
    let file = payload.supplemental_files.first()?;
    let lines = file
        .text
        .lines()
        .map(|line| line.trim_end_matches('\r').to_string())
        .collect::<Vec<_>>();
    if lines.is_empty() {
        return None;
    }
    let title = if payload.title.trim().is_empty() {
        format!(
            "GitHub file: {}",
            payload.path.as_deref().unwrap_or(hit.title.as_str())
        )
    } else {
        payload.title.clone()
    };
    let page_text = render_github_code_lines(&lines);
    let excerpts = best_github_code_snippets_for_query(query, &lines);
    let summary = excerpts
        .first()
        .cloned()
        .unwrap_or_else(|| fallback_summary(hit, &page_text));

    Some(OpenedPage {
        title,
        summary,
        is_pdf: false,
        pdf_total_pages: None,
        page_sections: Vec::new(),
        excerpts,
        page_text,
    })
}

fn github_api_repo_opened_page(
    query: &str,
    hit: &SearchHit,
    payload: &GithubApiPayload,
) -> Option<OpenedPage> {
    let mut sections = Vec::new();
    let mut summary_candidates = Vec::new();

    if !payload.description.trim().is_empty() {
        let description = format!("Repository description: {}", payload.description);
        sections.push(description.clone());
        summary_candidates.push(description);
    }

    if !payload.entries.is_empty() {
        sections.push(format!(
            "Repository entries:\n{}",
            payload.entries.join("\n")
        ));
        summary_candidates.extend(payload.entries.clone());
    }

    let readme_blocks = if payload.readme.trim().is_empty() {
        Vec::new()
    } else {
        clean_candidate_paragraphs(split_plaintext_paragraphs(&payload.readme))
    };

    let mut code_excerpts = Vec::new();
    for file in &payload.supplemental_files {
        if file.text.trim().is_empty() {
            continue;
        }
        let lines = file
            .text
            .lines()
            .map(|line| line.trim_end_matches('\r').to_string())
            .collect::<Vec<_>>();
        if lines.is_empty() {
            continue;
        }
        let is_code = github_path_likely_code(&file.path);
        let rendered = if is_code {
            let rendered = render_github_code_lines(&lines);
            code_excerpts.extend(best_github_code_snippets_for_query(query, &lines));
            rendered
        } else if github_text_looks_like_tree_summary(&file.path, &file.text) {
            let tree_lines = file
                .text
                .lines()
                .map(normalize_ws)
                .filter(|line| !line.is_empty())
                .collect::<Vec<_>>();
            summary_candidates.extend(tree_lines.clone());
            tree_lines.join("\n")
        } else {
            let paragraphs = clean_candidate_paragraphs(split_plaintext_paragraphs(&file.text));
            summary_candidates.extend(paragraphs.clone());
            paragraphs.join("\n\n")
        };
        if rendered.trim().is_empty() {
            continue;
        }
        sections.push(format!("File: {}\n{}", file.path, rendered));
        summary_candidates.push(format!("File {} {}", file.path, trim_text(&rendered, 400)));
    }

    if !readme_blocks.is_empty() {
        let compact_readme = readme_blocks.iter().take(8).cloned().collect::<Vec<_>>();
        sections.push(format!("README\n{}", compact_readme.join("\n\n")));
        summary_candidates.extend(compact_readme);
    }

    if sections.is_empty() {
        return None;
    }

    let page_text = sections.join("\n\n");
    let mut excerpts = Vec::new();
    excerpts.extend(code_excerpts);
    excerpts.extend(best_paragraphs_for_query(query, &readme_blocks, 2));
    excerpts.extend(best_github_tree_entries_for_query(query, &payload.entries));
    excerpts.extend(best_paragraphs_for_query(query, &summary_candidates, 3));
    let excerpts = dedupe_texts(excerpts)
        .into_iter()
        .take(4)
        .collect::<Vec<_>>();
    let summary = excerpts
        .first()
        .cloned()
        .unwrap_or_else(|| fallback_summary(hit, &page_text));

    Some(OpenedPage {
        title: if payload.title.trim().is_empty() {
            hit.title.clone()
        } else {
            payload.title.clone()
        },
        summary,
        is_pdf: false,
        pdf_total_pages: None,
        page_sections: Vec::new(),
        excerpts,
        page_text,
    })
}

fn github_path_likely_code(path: &str) -> bool {
    let lowered = path.to_ascii_lowercase();
    [
        ".rs", ".py", ".js", ".ts", ".tsx", ".jsx", ".go", ".java", ".c", ".cc", ".cpp", ".h",
        ".hpp", ".rb", ".php", ".swift", ".kt", ".m", ".mm", ".sh", ".bash", ".zsh", ".ps1",
        ".toml", ".yaml", ".yml", ".json", ".lock",
    ]
    .iter()
    .any(|suffix| lowered.ends_with(suffix))
}

fn github_text_looks_like_tree_summary(path: &str, text: &str) -> bool {
    path.ends_with('/')
        || text
            .lines()
            .map(str::trim)
            .filter(|line| !line.is_empty())
            .all(|line| {
                line.starts_with("directory: ")
                    || line.starts_with("dir: ")
                    || line.starts_with("tree: ")
                    || line.starts_with("file: ")
                    || line.starts_with("symlink_file: ")
            })
}

fn github_embedded_payload(body: &str) -> Option<Value> {
    static EMBEDDED_DATA_RE: OnceLock<Regex> = OnceLock::new();
    let re = EMBEDDED_DATA_RE.get_or_init(|| {
        Regex::new(
            r#"(?s)<script type="application/json" data-target="react-app\.embeddedData">(.*?)</script>"#,
        )
        .expect("valid github embedded-data regex")
    });
    let raw_json = re.captures(body)?.get(1)?.as_str();
    let value: Value = serde_json::from_str(raw_json).ok()?;
    value.get("payload").cloned()
}

fn github_tree_items_from_payload<'a>(payload: &'a Value) -> Option<&'a [Value]> {
    payload
        .get("codeViewRepoRoute")
        .and_then(|value| value.get("tree"))
        .and_then(|value| value.get("items"))
        .and_then(Value::as_array)
        .map(Vec::as_slice)
        .or_else(|| {
            payload
                .get("codeViewTreeRoute")
                .and_then(|value| value.get("tree"))
                .and_then(|value| value.get("items"))
                .and_then(Value::as_array)
                .map(Vec::as_slice)
        })
        .or_else(|| {
            let tree_path = payload
                .get("codeViewTreeRoute")
                .and_then(|value| value.get("path"))
                .and_then(Value::as_str)
                .unwrap_or_default();
            payload
                .get("codeViewFileTreeLayoutRoute")
                .and_then(|value| value.get("fileTree"))
                .and_then(|value| value.get(tree_path))
                .and_then(|value| value.get("items"))
                .and_then(Value::as_array)
                .map(Vec::as_slice)
        })
}

fn github_tree_entries_from_payload(payload: &Value) -> Option<Vec<GithubContentEntry>> {
    Some(
        github_tree_items_from_payload(payload)?
            .iter()
            .filter_map(|item| {
                Some(GithubContentEntry {
                    name: item.get("name").and_then(Value::as_str)?.to_string(),
                    path: item.get("path").and_then(Value::as_str)?.to_string(),
                    kind: item
                        .get("contentType")
                        .and_then(Value::as_str)
                        .unwrap_or("file")
                        .to_string(),
                })
            })
            .collect(),
    )
}

fn github_code_blob_opened_page(
    query: &str,
    hit: &SearchHit,
    payload: &Value,
) -> Option<OpenedPage> {
    let lines = payload
        .get("codeViewBlobLayoutRoute.StyledBlob")
        .and_then(|value| value.get("rawLines"))
        .and_then(Value::as_array)?
        .iter()
        .filter_map(Value::as_str)
        .map(ToString::to_string)
        .collect::<Vec<_>>();
    if lines.is_empty() {
        return None;
    }

    let path = payload
        .get("codeViewLayoutRoute")
        .and_then(|value| value.get("path"))
        .and_then(Value::as_str)
        .map(ToString::to_string)
        .unwrap_or_else(|| hit.title.clone());
    let title = format!("GitHub file: {path}");
    let page_text = render_github_code_lines(&lines);
    let excerpts = best_github_code_snippets_for_query(query, &lines);
    let summary = excerpts
        .first()
        .cloned()
        .unwrap_or_else(|| fallback_summary(hit, &page_text));

    Some(OpenedPage {
        title,
        summary,
        is_pdf: false,
        pdf_total_pages: None,
        page_sections: Vec::new(),
        excerpts,
        page_text,
    })
}

fn github_tree_opened_page(query: &str, hit: &SearchHit, payload: &Value) -> Option<OpenedPage> {
    let items = github_tree_items_from_payload(payload)?
        .iter()
        .filter_map(|item| {
            let name = item.get("name").and_then(Value::as_str)?;
            let path = item
                .get("path")
                .and_then(Value::as_str)
                .unwrap_or(name)
                .to_string();
            let kind = item
                .get("contentType")
                .and_then(Value::as_str)
                .unwrap_or("file");
            Some(format!("{kind}: {path}"))
        })
        .collect::<Vec<_>>();
    if items.is_empty() {
        return None;
    }

    let repo_path = payload
        .get("codeViewRepoRoute")
        .and_then(|value| value.get("path"))
        .and_then(Value::as_str)
        .or_else(|| {
            payload
                .get("codeViewTreeRoute")
                .and_then(|value| value.get("path"))
                .and_then(Value::as_str)
        })
        .unwrap_or("/");
    let title = if repo_path == "/" {
        format!("GitHub tree: {}", hit.title)
    } else {
        format!("GitHub tree: {}", repo_path.trim_matches('/'))
    };
    let page_text = items.join("\n");
    let excerpts = best_github_tree_entries_for_query(query, &items);
    let summary = excerpts
        .first()
        .cloned()
        .unwrap_or_else(|| fallback_summary(hit, &page_text));

    Some(OpenedPage {
        title,
        summary,
        is_pdf: false,
        pdf_total_pages: None,
        page_sections: Vec::new(),
        excerpts,
        page_text,
    })
}

fn github_markdown_opened_page(
    query: &str,
    hit: &SearchHit,
    payload: &Value,
) -> Option<OpenedPage> {
    let rich_text = payload
        .get("codeViewBlobRoute")
        .and_then(|value| value.get("richText"))
        .and_then(Value::as_str)
        .or_else(|| {
            payload
                .get("codeViewRepoRoute")
                .and_then(|value| value.get("overview"))
                .and_then(|value| value.get("overviewFiles"))
                .and_then(Value::as_array)
                .and_then(|files| {
                    files.iter().find_map(|file| {
                        file.get("richText")
                            .and_then(Value::as_str)
                            .filter(|text| !text.trim().is_empty())
                    })
                })
        })?;

    let article = Html::parse_document(rich_text);
    let title = select_text(&article, "h1, title").unwrap_or_else(|| hit.title.clone());
    let mut paragraphs = select_github_markdown_blocks(&article);
    if paragraphs.is_empty() {
        paragraphs = split_plaintext_paragraphs(rich_text);
    }
    if paragraphs.is_empty() {
        return None;
    }

    let excerpts = best_paragraphs_for_query(query, &paragraphs, 3);
    let page_text = paragraphs.join("\n\n");
    let summary = if excerpts.is_empty() {
        fallback_summary(hit, &page_text)
    } else {
        excerpts.join(" ")
    };

    Some(OpenedPage {
        title,
        summary,
        is_pdf: false,
        pdf_total_pages: None,
        page_sections: Vec::new(),
        excerpts,
        page_text,
    })
}

fn extract_github_discussion_opened_page(
    query: &str,
    hit: &SearchHit,
    body: &str,
) -> Option<OpenedPage> {
    let lowered = hit.url.to_ascii_lowercase();
    if !(lowered.contains("/issues/")
        || lowered.contains("/pull/")
        || lowered.contains("/releases/")
        || lowered.contains("/discussions/"))
    {
        return None;
    }

    let doc = Html::parse_document(body);
    let title = select_text(
        &doc,
        "bdi.js-issue-title, .gh-header-title .js-issue-title, h1",
    )
    .unwrap_or_else(|| hit.title.clone());
    let mut paragraphs = select_scoped_html_blocks(
        &doc,
        ".markdown-body, .comment-body, .js-comment-body, [data-testid='issue-body']",
        "h1, h2, h3, p, li, pre, blockquote",
    );
    if paragraphs.is_empty() {
        paragraphs = select_relevant_html_blocks(&doc);
    }
    if paragraphs.is_empty() {
        return None;
    }

    let excerpts = best_paragraphs_for_query(query, &paragraphs, 3);
    let page_text = paragraphs.join("\n\n");
    let summary = if excerpts.is_empty() {
        fallback_summary(hit, &page_text)
    } else {
        excerpts.join(" ")
    };

    Some(OpenedPage {
        title,
        summary,
        is_pdf: false,
        pdf_total_pages: None,
        page_sections: Vec::new(),
        excerpts,
        page_text,
    })
}

fn select_github_markdown_blocks(document: &Html) -> Vec<String> {
    static GITHUB_MARKDOWN_SELECTOR: OnceLock<Selector> = OnceLock::new();
    let selector = GITHUB_MARKDOWN_SELECTOR.get_or_init(|| {
        Selector::parse(
            "article h1, article h2, article h3, article p, article li, article pre, article code, article blockquote, h1, h2, h3, p, li, pre, code",
        )
        .expect("valid github markdown selector")
    });

    let blocks = document
        .select(selector)
        .filter(|node| !node_has_blocked_ancestor(*node))
        .map(|node| normalize_ws(&node.text().collect::<Vec<_>>().join(" ")))
        .filter(|text| !text.is_empty())
        .collect::<Vec<_>>();
    let cleaned = blocks
        .into_iter()
        .map(|text| normalize_ws(&text))
        .filter(|text| !text.is_empty())
        .filter(|text| !is_low_value_github_markdown_block(text))
        .collect::<Vec<_>>();
    dedupe_texts(cleaned)
}

fn is_low_value_github_markdown_block(text: &str) -> bool {
    let lowered = text.to_ascii_lowercase();
    if [
        "privacy policy",
        "terms of service",
        "cookie settings",
        "sign in",
        "log in",
        "javascript is required",
    ]
    .iter()
    .any(|marker| lowered.contains(marker))
    {
        return true;
    }

    let word_count = text.split_whitespace().count();
    let separator_count = text.matches('|').count() + text.matches('>').count();
    text.len() < 8 || word_count == 0 || separator_count >= 3
}

fn render_github_code_lines(lines: &[String]) -> String {
    lines
        .iter()
        .enumerate()
        .map(|(index, line)| format!("{:>4}: {}", index + 1, line))
        .collect::<Vec<_>>()
        .join("\n")
}

fn best_github_code_snippets_for_query(query: &str, lines: &[String]) -> Vec<String> {
    let lowered_query = query.to_ascii_lowercase();
    let command_oriented = is_command_oriented_github_query(query);
    let mut terms = query_terms(query);
    if command_oriented {
        for hint in [
            "init", "install", "setup", "run", "usage", "command", "help",
        ] {
            if !terms.iter().any(|term| term == hint) {
                terms.push(hint.to_string());
            }
        }
    }
    let mut scored = Vec::new();

    for (index, line) in lines.iter().enumerate() {
        let lowered = line.to_ascii_lowercase();
        let mut score = 0usize;
        if !lowered_query.is_empty() && lowered.contains(&lowered_query) {
            score += 300;
        }
        for term in &terms {
            if lowered.contains(term.as_str()) {
                score += 100;
            }
        }
        if command_oriented {
            for (hint, weight) in [
                ("init", 350usize),
                ("install", 280),
                ("setup", 220),
                ("usage", 180),
                ("help", 120),
                ("command", 40),
            ] {
                if lowered.contains(hint) {
                    score += weight;
                }
            }
        }
        if score == 0 {
            continue;
        }

        let start = index.saturating_sub(1);
        let end = (index + 2).min(lines.len());
        let snippet = lines[start..end]
            .iter()
            .enumerate()
            .map(|(offset, value)| format!("{:>4}: {}", start + offset + 1, value))
            .collect::<Vec<_>>()
            .join("\n");
        scored.push((score, index, snippet));
    }

    scored.sort_by(|left, right| right.0.cmp(&left.0).then_with(|| left.1.cmp(&right.1)));
    let mut snippets = Vec::new();
    for (_, _, snippet) in scored {
        if snippets.iter().any(|seen: &String| seen == &snippet) {
            continue;
        }
        snippets.push(snippet);
        if snippets.len() >= 3 {
            break;
        }
    }

    if snippets.is_empty() {
        let snippet = lines
            .iter()
            .enumerate()
            .take(6)
            .map(|(index, value)| format!("{:>4}: {}", index + 1, value))
            .collect::<Vec<_>>()
            .join("\n");
        if !snippet.trim().is_empty() {
            snippets.push(snippet);
        }
    }

    snippets
}

fn best_github_tree_entries_for_query(query: &str, entries: &[String]) -> Vec<String> {
    if entries.is_empty() {
        return Vec::new();
    }
    let best = best_paragraphs_for_query(query, entries, 5);
    if best.is_empty() {
        entries.iter().take(8).cloned().collect()
    } else {
        best
    }
}

fn extract_text_opened_page(query: &str, hit: &SearchHit, body: &str) -> OpenedPage {
    let paragraphs = clean_candidate_paragraphs(split_plaintext_paragraphs(body));
    let excerpts = best_paragraphs_for_query(query, &paragraphs, 3);
    let summary = if excerpts.is_empty() {
        hit.snippet.clone()
    } else {
        excerpts.join(" ")
    };
    OpenedPage {
        title: hit.title.clone(),
        summary,
        is_pdf: false,
        pdf_total_pages: None,
        page_sections: Vec::new(),
        excerpts,
        page_text: paragraphs.join("\n\n"),
    }
}

fn extract_pdf_opened_page(
    config: &SearchConfig,
    query: &str,
    hit: &SearchHit,
    fetched: &FetchedPageContent,
) -> Result<OpenedPage> {
    let extracted = extract_pdf_sections_guided(config, query, &fetched.body)
        .with_context(|| format!("failed to extract PDF text from {}", hit.url))?;
    let cleaned = render_pdf_page_text(&extracted.sections);
    let excerpts = best_pdf_paragraphs_for_query(query, &extracted.sections, 3, &cleaned);
    let summary = if excerpts.is_empty() {
        fallback_summary(hit, &cleaned)
    } else {
        excerpts.join(" ")
    };
    let title = pdf_title_from_sections(&extracted.sections).unwrap_or_else(|| hit.title.clone());
    Ok(OpenedPage {
        title,
        summary,
        is_pdf: true,
        pdf_total_pages: Some(extracted.total_pages),
        page_sections: extracted.sections,
        excerpts,
        page_text: cleaned,
    })
}

fn extract_pdf_sections_guided(
    config: &SearchConfig,
    query: &str,
    body: &[u8],
) -> Result<PdfExtraction> {
    let total_pages =
        page_count_for_pdf_bytes(body, None).context("failed to inspect PDF bytes")?;
    let page_numbers = (1..=total_pages)
        .map(|page| u32::try_from(page).unwrap_or(u32::MAX))
        .collect::<Vec<_>>();
    let max_pdf_pages = config.max_pdf_pages.max(1);
    let char_budget = config.max_page_chars.saturating_mul(4).clamp(8_000, 64_000);
    let candidate_pages = guided_pdf_page_order(query, &page_numbers, max_pdf_pages);
    let explicit_hints = extract_pdf_page_hints(query, Some(total_pages));
    let terms = query_terms(query);
    let initial_pages = candidate_pages
        .iter()
        .take(initial_pdf_window(max_pdf_pages))
        .copied()
        .collect::<Vec<_>>();
    let mut pages = Vec::new();
    let mut approx_chars = 0usize;
    let mut saw_query_hit = false;
    let mut loaded_after_hit = 0usize;

    let parsed = parse_pdf_bytes_internal(
        body,
        LiteParseConfigOverrides {
            ocr_enabled: Some(false),
            output_format: Some(OutputFormat::Text),
            max_pages: Some(max_pdf_pages),
            target_pages: Some(Some(join_pdf_target_pages(&candidate_pages))),
            ..Default::default()
        },
    )
    .context("failed to parse selected PDF pages")?;

    for page in parsed.pages {
        let page_num = u32::try_from(page.page_num).unwrap_or(u32::MAX);
        let cleaned = clean_pdf_text_for_llm(&page.text);
        if cleaned.is_empty() {
            continue;
        }
        approx_chars = approx_chars.saturating_add(cleaned.len());
        let page_hit = page_matches_query(&cleaned, &terms, query);
        saw_query_hit |= page_hit;
        if saw_query_hit {
            loaded_after_hit += 1;
        }
        pages.push(EvidenceSection {
            page_number: Some(page_num),
            text: cleaned,
        });
        if approx_chars >= char_budget {
            break;
        }
        if explicit_hints.is_empty() {
            if terms.is_empty() && pages.len() >= initial_pages.len().max(1) {
                break;
            }
            if saw_query_hit && loaded_after_hit >= 2 {
                break;
            }
        } else if saw_query_hit
            && pages.iter().any(|section| {
                section
                    .page_number
                    .is_some_and(|page| explicit_hints.contains(&page))
            })
        {
            break;
        }
    }

    Ok(PdfExtraction {
        total_pages,
        sections: pages,
    })
}

fn join_pdf_target_pages(page_numbers: &[u32]) -> String {
    page_numbers
        .iter()
        .map(u32::to_string)
        .collect::<Vec<_>>()
        .join(",")
}

fn guided_pdf_page_order(query: &str, page_numbers: &[u32], max_pdf_pages: usize) -> Vec<u32> {
    let mut ordered = Vec::new();
    let initial = initial_pdf_window(max_pdf_pages);
    ordered.extend(page_numbers.iter().take(initial).copied());

    let hinted = extract_pdf_page_hints(query, Some(page_numbers.len()));
    for hinted_page in hinted {
        for candidate in hinted_page.saturating_sub(1)..=hinted_page.saturating_add(1) {
            if page_numbers.contains(&candidate) && !ordered.contains(&candidate) {
                ordered.push(candidate);
            }
        }
    }

    for page_num in page_numbers {
        if ordered.len() >= max_pdf_pages {
            break;
        }
        if !ordered.contains(page_num) {
            ordered.push(*page_num);
        }
    }

    ordered.truncate(max_pdf_pages);
    ordered
}

fn initial_pdf_window(max_pdf_pages: usize) -> usize {
    max_pdf_pages.min(4).max(1)
}

fn extract_pdf_page_hints(query: &str, total_pages: Option<usize>) -> Vec<u32> {
    static PAGE_RANGE_RE: OnceLock<Regex> = OnceLock::new();
    static PAGE_SINGLE_RE: OnceLock<Regex> = OnceLock::new();

    let total_pages = total_pages.unwrap_or(usize::MAX);
    let page_range_re = PAGE_RANGE_RE.get_or_init(|| {
        Regex::new(r"(?i)\bpages?\s+(\d{1,4})\s*(?:-|–|to)\s*(\d{1,4})\b")
            .expect("valid pdf page range regex")
    });
    let page_single_re = PAGE_SINGLE_RE.get_or_init(|| {
        Regex::new(r"(?i)\b(?:page|p\.)\s*(\d{1,4})\b").expect("valid pdf page regex")
    });

    let mut pages = Vec::new();
    for caps in page_range_re.captures_iter(query) {
        let Some(start) = caps
            .get(1)
            .and_then(|value| value.as_str().parse::<u32>().ok())
        else {
            continue;
        };
        let Some(end) = caps
            .get(2)
            .and_then(|value| value.as_str().parse::<u32>().ok())
        else {
            continue;
        };
        let lower = start.min(end).max(1);
        let upper = start.max(end);
        for page in lower..=upper {
            if page as usize <= total_pages && !pages.contains(&page) {
                pages.push(page);
            }
        }
    }
    for caps in page_single_re.captures_iter(query) {
        let Some(page) = caps
            .get(1)
            .and_then(|value| value.as_str().parse::<u32>().ok())
        else {
            continue;
        };
        if page == 0 || page as usize > total_pages || pages.contains(&page) {
            continue;
        }
        pages.push(page);
    }
    pages
}

fn page_matches_query(page_text: &str, terms: &[String], query: &str) -> bool {
    if page_text.trim().is_empty() {
        return false;
    }
    let lowered = page_text.to_ascii_lowercase();
    terms.iter().any(|term| lowered.contains(term))
        || extract_pdf_page_hints(query, None)
            .into_iter()
            .any(|page| lowered.contains(&format!("page {page}")))
}

fn render_pdf_page_text(sections: &[EvidenceSection]) -> String {
    sections
        .iter()
        .map(|section| match section.page_number {
            Some(page) => format!("[Page {}]\n{}", page, section.text),
            None => section.text.clone(),
        })
        .collect::<Vec<_>>()
        .join("\n\n")
}

fn pdf_title_from_sections(sections: &[EvidenceSection]) -> Option<String> {
    sections.iter().find_map(|section| {
        let raw_paragraphs = split_pdf_paragraphs(&section.text);
        let paragraphs = fallback_candidate_paragraphs(raw_paragraphs);
        paragraphs
            .into_iter()
            .find(|paragraph| paragraph.len() >= 10)
            .map(|paragraph| trim_text(&paragraph, 120))
    })
}

fn split_plaintext_paragraphs(body: &str) -> Vec<String> {
    body.lines()
        .map(normalize_ws)
        .filter(|line| line.len() >= 40)
        .collect()
}

fn split_pdf_paragraphs(text: &str) -> Vec<String> {
    let mut paragraphs = Vec::new();
    let mut current = Vec::new();

    for line in text.lines().map(str::trim) {
        if line.is_empty() {
            if !current.is_empty() {
                paragraphs.push(current.join(" "));
                current.clear();
            }
            continue;
        }
        current.push(line.to_string());
    }

    if !current.is_empty() {
        paragraphs.push(current.join(" "));
    }

    if paragraphs.is_empty() && !text.trim().is_empty() {
        paragraphs.push(text.trim().to_string());
    }

    paragraphs
        .into_iter()
        .map(|paragraph| trim_text(&paragraph, 1200))
        .filter(|paragraph| paragraph.len() >= 30)
        .collect()
}

fn fallback_candidate_paragraphs(paragraphs: Vec<String>) -> Vec<String> {
    let cleaned = clean_candidate_paragraphs(paragraphs.clone());
    if cleaned.is_empty() {
        paragraphs
            .into_iter()
            .map(|paragraph| normalize_ws(&paragraph))
            .filter(|paragraph| !paragraph.is_empty())
            .collect()
    } else {
        cleaned
    }
}

fn fallback_summary(hit: &SearchHit, text: &str) -> String {
    let cleaned = trim_text(text, 360);
    if cleaned.is_empty() {
        hit.snippet.clone()
    } else {
        cleaned
    }
}

fn clean_pdf_text_for_llm(text: &str) -> String {
    let normalized = text.replace('\u{0000}', " ");
    let lines = normalized
        .lines()
        .map(normalize_pdf_line)
        .filter(|line| !line.is_empty())
        .collect::<Vec<_>>();

    if lines.is_empty() {
        return String::new();
    }

    let mut merged = Vec::new();
    let mut current = String::new();
    for line in lines {
        if current.is_empty() {
            current = line;
            continue;
        }
        if should_merge_pdf_lines(&current, &line) {
            merge_pdf_lines(&mut current, &line);
        } else {
            merged.push(current);
            current = line;
        }
    }
    if !current.is_empty() {
        merged.push(current);
    }

    dedupe_texts(merged).join("\n")
}

fn normalize_pdf_line(line: &str) -> String {
    let normalized = normalize_ws(line);
    if normalized.is_empty() {
        return String::new();
    }
    if is_pdf_numeric_artifact(&normalized) {
        return String::new();
    }
    normalized
}

fn is_pdf_numeric_artifact(line: &str) -> bool {
    let stripped = line
        .trim_matches(|ch: char| matches!(ch, '|' | '[' | ']' | '(' | ')' | '{' | '}'))
        .trim();
    !stripped.is_empty()
        && stripped.chars().all(|ch| {
            ch.is_ascii_digit() || matches!(ch, '.' | ',' | '%' | '+' | '-' | ':' | '/' | ' ')
        })
}

fn should_merge_pdf_lines(current: &str, next: &str) -> bool {
    if current.ends_with('-') {
        return true;
    }
    let current_len = current.len();
    let next_len = next.len();
    current_len < 140 && next_len < 140 && !ends_sentence(current) && starts_like_continuation(next)
}

fn merge_pdf_lines(current: &mut String, next: &str) {
    if current.ends_with('-') {
        current.pop();
        current.push_str(next);
        return;
    }
    current.push(' ');
    current.push_str(next);
}

fn ends_sentence(text: &str) -> bool {
    text.ends_with('.')
        || text.ends_with('!')
        || text.ends_with('?')
        || text.ends_with(':')
        || text.ends_with(';')
}

fn starts_like_continuation(text: &str) -> bool {
    text.chars()
        .next()
        .is_some_and(|ch| ch.is_ascii_lowercase() || ch.is_ascii_digit() || ch == '(')
}

fn build_find_in_page_results(
    query: &str,
    page_text: &str,
    page_sections: &[EvidenceSection],
    excerpts: &[String],
) -> Vec<FindInPageResult> {
    let fallback_text = excerpts.join("\n\n");
    query_patterns(query)
        .into_iter()
        .take(12)
        .filter_map(|pattern| {
            let matches = if !page_sections.is_empty() {
                find_matches_in_sections(page_sections, &pattern, 2)
            } else if page_text.trim().is_empty() {
                find_matches_in_text(&fallback_text, &pattern, 2)
            } else {
                find_matches_in_text(page_text, &pattern, 2)
            };
            if matches.is_empty() {
                None
            } else {
                Some(FindInPageResult { pattern, matches })
            }
        })
        .collect()
}

fn find_matches_in_sections(
    sections: &[EvidenceSection],
    pattern: &str,
    max_matches: usize,
) -> Vec<String> {
    if pattern.trim().is_empty() || max_matches == 0 {
        return Vec::new();
    }

    let mut matches = Vec::new();
    for section in sections {
        let page_matches = find_matches_in_text(&section.text, pattern, max_matches);
        for matched in page_matches {
            let labeled = match section.page_number {
                Some(page) => format!("p. {}: {}", page, matched),
                None => matched,
            };
            if !matches.contains(&labeled) {
                matches.push(labeled);
            }
            if matches.len() >= max_matches {
                return matches;
            }
        }
    }
    matches
}

fn find_matches_in_text(text: &str, pattern: &str, max_matches: usize) -> Vec<String> {
    if text.trim().is_empty() || pattern.trim().is_empty() || max_matches == 0 {
        return Vec::new();
    }

    let haystack = text.to_ascii_lowercase();
    let needle = pattern.to_ascii_lowercase();
    let mut matches = Vec::new();
    let mut search_from = 0usize;

    while search_from < haystack.len() && matches.len() < max_matches {
        let Some(found) = haystack[search_from..].find(&needle) else {
            break;
        };
        let start = search_from + found;
        let end = start + needle.len();
        let excerpt = excerpt_around_bytes(text, start, end, 96);
        if !excerpt.is_empty() && !matches.contains(&excerpt) {
            matches.push(excerpt);
        }
        search_from = end;
    }

    if !matches.is_empty() {
        return matches;
    }

    let normalized_needle = normalize_pattern(pattern);
    if normalized_needle.is_empty() {
        return matches;
    }

    let (normalized_haystack, byte_map) = normalize_text_with_byte_map(text);
    let mut normalized_from = 0usize;
    while normalized_from < normalized_haystack.len() && matches.len() < max_matches {
        let Some(found) = normalized_haystack[normalized_from..].find(&normalized_needle) else {
            break;
        };
        let start = normalized_from + found;
        let end = start + normalized_needle.len();
        let Some(&byte_start) = byte_map.get(start) else {
            break;
        };
        let byte_end = if end < byte_map.len() {
            byte_map[end]
        } else {
            text.len()
        };
        let excerpt = excerpt_around_bytes(text, byte_start, byte_end, 96);
        if !excerpt.is_empty() && !matches.contains(&excerpt) {
            matches.push(excerpt);
        }
        normalized_from = end;
    }

    matches
}

fn normalize_pattern(pattern: &str) -> String {
    let (normalized, _) = normalize_text_with_byte_map(pattern);
    normalized.trim().to_string()
}

fn normalize_text_with_byte_map(text: &str) -> (String, Vec<usize>) {
    let mut normalized = String::with_capacity(text.len());
    let mut byte_map = Vec::with_capacity(text.len());
    let mut pending_space = false;

    for (byte_idx, ch) in text.char_indices() {
        if ch.is_alphanumeric() {
            if pending_space && !normalized.is_empty() {
                normalized.push(' ');
                byte_map.push(byte_idx);
            }
            pending_space = false;
            for lowered in ch.to_lowercase() {
                normalized.push(lowered);
                byte_map.push(byte_idx);
            }
        } else if !normalized.is_empty() {
            pending_space = true;
        }
    }

    (normalized, byte_map)
}

fn excerpt_around_bytes(text: &str, start: usize, end: usize, radius: usize) -> String {
    let excerpt_start = clamp_left_boundary(text, start.saturating_sub(radius));
    let excerpt_end = clamp_right_boundary(text, end.saturating_add(radius).min(text.len()));
    trim_text(text[excerpt_start..excerpt_end].trim(), 240)
}

fn clamp_left_boundary(text: &str, mut idx: usize) -> usize {
    while idx > 0 && !text.is_char_boundary(idx) {
        idx -= 1;
    }
    idx
}

fn clamp_right_boundary(text: &str, mut idx: usize) -> usize {
    while idx < text.len() && !text.is_char_boundary(idx) {
        idx += 1;
    }
    idx.min(text.len())
}

fn extract_web_search_request(payload: &Value) -> Option<SearchToolRequest> {
    find_web_search_tool(payload.get("tools")?).map(parse_web_search_tool)
}

fn canonical_web_search_request_from_responses(
    payload: &Value,
) -> Option<CanonicalWebSearchRequest> {
    let mut request =
        extract_web_search_request(payload).map(|tool_request| CanonicalWebSearchRequest {
            query: String::new(),
            external_web_access: tool_request.external_web_access,
            allowed_domains: tool_request.allowed_domains,
            user_location: tool_request.user_location,
            search_context_size: tool_request.search_context_size,
            search_content_types: tool_request.search_content_types,
            include_sources: request_includes_sources(payload),
        })?;
    request.query = extract_latest_user_query(payload)?;
    Some(request)
}

fn canonical_request_to_tool_request(request: &CanonicalWebSearchRequest) -> SearchToolRequest {
    SearchToolRequest {
        external_web_access: request.external_web_access,
        allowed_domains: request
            .allowed_domains
            .iter()
            .map(|domain| normalize_domain(domain))
            .filter(|domain| !domain.is_empty())
            .collect(),
        user_location: request.user_location.clone(),
        search_context_size: request.search_context_size,
        search_content_types: request.search_content_types.clone(),
        include_sources: request.include_sources,
    }
}

fn find_web_search_tool(tools: &Value) -> Option<&Value> {
    tools.as_array()?.iter().find_map(|tool| {
        if tool.get("type").and_then(Value::as_str) == Some("web_search") {
            return Some(tool);
        }
        tool.get("tools").and_then(find_web_search_tool)
    })
}

fn parse_web_search_tool(tool: &Value) -> SearchToolRequest {
    let allowed_domains = tool
        .get("filters")
        .and_then(|filters| filters.get("allowed_domains"))
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .filter_map(Value::as_str)
                .map(normalize_domain)
                .filter(|domain| !domain.is_empty())
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    let user_location = tool
        .get("user_location")
        .map(parse_user_location)
        .unwrap_or_default();
    let search_content_types = tool
        .get("search_content_types")
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .filter_map(Value::as_str)
                .map(ToString::to_string)
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();

    SearchToolRequest {
        external_web_access: tool.get("external_web_access").and_then(Value::as_bool),
        allowed_domains,
        user_location,
        search_context_size: ContextSize::from_value(tool.get("search_context_size")),
        search_content_types,
        include_sources: false,
    }
}

fn request_includes_sources(payload: &Value) -> bool {
    payload
        .get("include")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .any(|item| item == "web_search_call.action.sources")
}

fn strip_web_search_tools(payload: &mut Value) -> bool {
    payload
        .get_mut("tools")
        .map(strip_web_search_tools_value)
        .unwrap_or(false)
}

fn strip_web_search_tools_value(value: &mut Value) -> bool {
    let Some(items) = value.as_array_mut() else {
        return false;
    };
    let mut changed = false;
    let mut rewritten = Vec::with_capacity(items.len());
    for mut item in std::mem::take(items) {
        if item.get("type").and_then(Value::as_str) == Some("web_search") {
            changed = true;
            continue;
        }
        if strip_nested_web_search_tools(&mut item) {
            changed = true;
        }
        if namespace_tool_is_empty(&item) {
            changed = true;
            continue;
        }
        rewritten.push(item);
    }
    *items = rewritten;
    changed
}

fn strip_nested_web_search_tools(tool: &mut Value) -> bool {
    let Some(object) = tool.as_object_mut() else {
        return false;
    };
    object
        .get_mut("tools")
        .map(strip_web_search_tools_value)
        .unwrap_or(false)
}

fn namespace_tool_is_empty(tool: &Value) -> bool {
    tool.get("type").and_then(Value::as_str) == Some("namespace")
        && tool
            .get("tools")
            .and_then(Value::as_array)
            .map(|items| items.is_empty())
            .unwrap_or(true)
}

fn parse_user_location(value: &Value) -> SearchUserLocation {
    SearchUserLocation {
        country: value
            .get("country")
            .and_then(Value::as_str)
            .map(ToString::to_string),
        region: value
            .get("region")
            .and_then(Value::as_str)
            .map(ToString::to_string),
        city: value
            .get("city")
            .and_then(Value::as_str)
            .map(ToString::to_string),
        timezone: value
            .get("timezone")
            .and_then(Value::as_str)
            .map(ToString::to_string),
    }
}

fn build_search_text(query: &str, allowed_domains: &[String]) -> String {
    if allowed_domains.is_empty() {
        return trim_query(query);
    }
    let domains = allowed_domains
        .iter()
        .map(|domain| normalize_domain(domain))
        .filter(|domain| !domain.is_empty())
        .map(|domain| format!("site:{domain}"))
        .collect::<Vec<_>>()
        .join(" OR ");
    trim_query(&format!("{query} {domains}"))
}

fn derive_region(config: &SearchConfig, location: &SearchUserLocation) -> Option<String> {
    location
        .country
        .clone()
        .or_else(|| config.default_region.clone())
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

fn filter_hits_by_domain(hits: Vec<SearchHit>, allowed_domains: &[String]) -> Vec<SearchHit> {
    if allowed_domains.is_empty() {
        return hits;
    }

    hits.into_iter()
        .filter(|hit| url_matches_allowed_domains(&hit.url, allowed_domains))
        .collect()
}

fn filter_evidence_by_domain(
    evidence: Vec<EvidenceDoc>,
    allowed_domains: &[String],
) -> Vec<EvidenceDoc> {
    if allowed_domains.is_empty() {
        return evidence;
    }

    evidence
        .into_iter()
        .filter(|doc| url_matches_allowed_domains(&doc.url, allowed_domains))
        .collect()
}

fn extract_latest_user_query(payload: &Value) -> Option<String> {
    match payload.get("input") {
        Some(Value::String(text)) => normalize_text(text),
        Some(Value::Array(items)) => items.iter().rev().find_map(extract_query_from_item),
        Some(other) => normalize_text(&other.to_string()),
        None => None,
    }
}

fn extract_query_from_item(item: &Value) -> Option<String> {
    let role = item.get("role").and_then(Value::as_str).unwrap_or_default();
    if role != "user" {
        return None;
    }

    match item.get("content") {
        Some(Value::String(text)) => normalize_text(text),
        Some(Value::Array(chunks)) => {
            let parts = chunks
                .iter()
                .filter_map(|chunk| chunk.get("text").and_then(Value::as_str))
                .map(str::trim)
                .filter(|text| !text.is_empty())
                .map(ToString::to_string)
                .collect::<Vec<_>>();
            if parts.is_empty() {
                None
            } else {
                Some(parts.join("\n"))
            }
        }
        Some(other) => normalize_text(&other.to_string()),
        None => None,
    }
}

fn inject_developer_context(payload: &mut Value, context: String) {
    let mut items = match payload.get("input") {
        Some(Value::Array(items)) => items.clone(),
        Some(Value::String(text)) => vec![json!({
            "type": "message",
            "role": "user",
            "content": [{"type":"input_text","text": text}]
        })],
        Some(other) => vec![json!({
            "type": "message",
            "role": "user",
            "content": [{"type":"input_text","text": other.to_string()}]
        })],
        None => Vec::new(),
    };

    items.push(json!({
        "type": "message",
        "role": "developer",
        "content": [{"type":"input_text","text": context}]
    }));
    payload["input"] = Value::Array(items);
}

fn render_results_context(
    original_query: &str,
    tool_request: &SearchToolRequest,
    context_size: ContextSize,
    result: &SearchResponse,
) -> String {
    let mut lines = vec![
        "CTOX executed a local web_search request for this turn.".to_string(),
        format!("Original query: {}", original_query),
        format!("Provider: {}", result.provider),
        format!(
            "External web access: {}",
            tool_request.external_web_access.unwrap_or(true)
        ),
        format!("Search context size: {}", context_size.as_str()),
        format!("Retrieved at unix_ts={}", unix_ts()),
    ];

    if !tool_request.allowed_domains.is_empty() {
        lines.push(format!(
            "Allowed domains: {}",
            tool_request.allowed_domains.join(", ")
        ));
    }
    if let Some(location) = render_location(tool_request) {
        lines.push(format!("User location: {}", location));
    }
    if !tool_request.search_content_types.is_empty() {
        lines.push(format!(
            "Requested content types: {}",
            tool_request.search_content_types.join(", ")
        ));
    }
    lines.push(
        "Use these web results as external context. Prefer the URLs below when citing sources."
            .to_string(),
    );

    if result.hits.is_empty() {
        lines.push("No search results were returned.".to_string());
    } else {
        lines.push("Search results:".to_string());
        for hit in &result.hits {
            lines.push(format!("{}. {}", hit.rank, hit.title));
            lines.push(format!("URL: {}", hit.url));
            if !hit.snippet.trim().is_empty() {
                lines.push(format!("Snippet: {}", hit.snippet));
            }
            lines.push(format!("Source: {}", hit.source));
        }
    }

    if !result.evidence.is_empty() {
        lines.push("Opened pages and extracted evidence:".to_string());
        for doc in &result.evidence {
            lines.push(format!("Open page: {}", doc.title));
            lines.push(format!("Open page URL: {}", doc.url));
            if doc.is_pdf {
                if let Some(total_pages) = doc.pdf_total_pages {
                    lines.push(format!("PDF total pages: {}", total_pages));
                }
                let loaded_pages = doc
                    .page_sections
                    .iter()
                    .filter_map(|section| section.page_number)
                    .map(|page| page.to_string())
                    .collect::<Vec<_>>();
                if !loaded_pages.is_empty() {
                    lines.push(format!("Loaded PDF pages: {}", loaded_pages.join(", ")));
                }
            }
            lines.push(format!("Open page summary: {}", doc.summary));
            for (index, excerpt) in doc.excerpts.iter().take(3).enumerate() {
                lines.push(format!("Open page extract {}: {}", index + 1, excerpt));
            }
            for find_result in doc.find_results.iter().take(3) {
                lines.push(format!("Find in page pattern: {}", find_result.pattern));
                for matched in find_result.matches.iter().take(2) {
                    lines.push(format!("Find in page match: {}", matched));
                }
            }
        }
    }

    lines.join("\n")
}

fn render_failure_context(
    original_query: &str,
    tool_request: &SearchToolRequest,
    err: &anyhow::Error,
) -> String {
    let mut lines = vec![
        "CTOX attempted a local web_search request for this turn but the search provider failed."
            .to_string(),
        format!("Original query: {}", original_query),
        format!(
            "External web access: {}",
            tool_request.external_web_access.unwrap_or(true)
        ),
        format!("Failure: {}", err),
        "Do not claim verified live web browsing beyond this point.".to_string(),
    ];
    if !tool_request.allowed_domains.is_empty() {
        lines.push(format!(
            "Allowed domains: {}",
            tool_request.allowed_domains.join(", ")
        ));
    }
    lines.join("\n")
}

fn render_location(tool_request: &SearchToolRequest) -> Option<String> {
    let mut parts = Vec::new();
    if let Some(city) = &tool_request.user_location.city {
        parts.push(city.clone());
    }
    if let Some(region) = &tool_request.user_location.region {
        parts.push(region.clone());
    }
    if let Some(country) = &tool_request.user_location.country {
        parts.push(country.clone());
    }
    if let Some(timezone) = &tool_request.user_location.timezone {
        parts.push(timezone.clone());
    }
    if parts.is_empty() {
        None
    } else {
        Some(parts.join(", "))
    }
}

fn build_query_variants(original_query: &str, executed_query: &str) -> Vec<String> {
    let mut variants = Vec::new();
    for candidate in [executed_query, original_query] {
        let normalized = trim_query(candidate);
        if !normalized.is_empty() && !variants.contains(&normalized) {
            variants.push(normalized);
        }
    }
    variants
}

fn plan_search_queries(original_query: &str, allowed_domains: &[String]) -> Vec<String> {
    let base = build_search_text(original_query, allowed_domains);
    let pdf_page_query = pdf_page_focused_query(original_query, allowed_domains);
    let keywords = keyword_focused_query(original_query, allowed_domains);
    let mut queries = Vec::new();
    for candidate in [Some(base), pdf_page_query].into_iter().flatten() {
        let normalized = trim_query(&candidate);
        if !normalized.is_empty() && !queries.contains(&normalized) {
            queries.push(normalized);
        }
    }

    for year_variant in year_variants(original_query, allowed_domains) {
        if !queries.contains(&year_variant) {
            queries.push(year_variant);
        }
    }

    if let Some(keywords) = keywords {
        let normalized = trim_query(&keywords);
        if !normalized.is_empty() && !queries.contains(&normalized) {
            queries.push(normalized);
        }
    }

    if queries.is_empty() {
        queries.push(trim_query(original_query));
    }
    queries.truncate(4);
    queries
}

fn keyword_focused_query(original_query: &str, allowed_domains: &[String]) -> Option<String> {
    let terms = significant_terms_with_numbers(original_query);
    if terms.is_empty() {
        return None;
    }
    Some(build_search_text(&terms.join(" "), allowed_domains))
}

fn pdf_page_focused_query(original_query: &str, allowed_domains: &[String]) -> Option<String> {
    if !original_query.to_ascii_lowercase().contains("pdf") {
        return None;
    }
    let mut terms = significant_terms_with_numbers(original_query);
    let hinted_pages = extract_pdf_page_hints(original_query, None);
    if let Some(page) = hinted_pages.first() {
        terms.push("page".to_string());
        terms.push(page.to_string());
    }
    if !terms.iter().any(|term| term.eq_ignore_ascii_case("pdf")) {
        terms.push("pdf".to_string());
    }
    if terms.is_empty() {
        None
    } else {
        Some(build_search_text(&terms.join(" "), allowed_domains))
    }
}

fn year_variants(original_query: &str, allowed_domains: &[String]) -> Vec<String> {
    let lowered = original_query.to_ascii_lowercase();
    if lowered.contains("2023")
        || lowered.contains("2024")
        || lowered.contains("2025")
        || lowered.contains("2026")
    {
        return Vec::new();
    }
    if !(lowered.contains("instructions")
        || lowered.contains("manual")
        || lowered.contains("report")
        || lowered.contains("form"))
    {
        return Vec::new();
    }

    let now_secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64;
    let current_year = chrono::DateTime::<chrono::Utc>::from_timestamp(now_secs, 0)
        .map(|dt| dt.year())
        .unwrap_or(2026);
    let years = if lowered.contains("form") || lowered.contains("instructions") {
        vec![current_year - 1, current_year - 2]
    } else {
        vec![current_year, current_year - 1]
    };
    let keywords = significant_terms_with_numbers(original_query).join(" ");
    let mut variants = Vec::new();
    for year in years {
        let query = trim_query(&build_search_text(
            &format!("{keywords} {year}"),
            allowed_domains,
        ));
        if !query.is_empty() {
            variants.push(query);
        }
    }
    variants
}

fn significant_terms_with_numbers(query: &str) -> Vec<String> {
    const STOP_WORDS: &[&str] = &[
        "about", "after", "also", "best", "cite", "from", "have", "into", "just", "need", "page",
        "return", "search", "sentence", "short", "sources", "tell", "that", "their", "there",
        "they", "this", "use", "using", "what", "when", "where", "which", "with", "would",
    ];

    let mut terms = Vec::new();
    for term in query
        .split(|ch: char| !(ch.is_alphanumeric() || ch == '-'))
        .map(str::trim)
        .filter(|term| !term.is_empty())
    {
        let lowered = term.to_ascii_lowercase();
        let keep =
            term.chars().all(|ch| ch.is_ascii_digit()) || term.len() >= 3 || lowered == "pdf";
        if !keep || STOP_WORDS.contains(&lowered.as_str()) || terms.contains(&lowered) {
            continue;
        }
        terms.push(lowered);
    }
    terms
}

fn build_web_search_calls(
    base_call_id: &str,
    result: &SearchResponse,
    include_sources: bool,
) -> Vec<WebSearchCall> {
    let sources = include_sources.then(|| build_action_sources(result));
    let mut calls = vec![WebSearchCall {
        id: base_call_id.to_string(),
        status: "completed",
        action: Some(WebSearchAction::Search {
            query: result.executed_queries.first().cloned().unwrap_or_default(),
            queries: result.executed_queries.clone(),
            sources,
        }),
    }];

    for (index, doc) in result.evidence.iter().enumerate() {
        calls.push(WebSearchCall {
            id: format!("{base_call_id}_open_{}", index + 1),
            status: "completed",
            action: Some(WebSearchAction::OpenPage {
                url: doc.url.clone(),
            }),
        });

        for (match_index, find_result) in doc.find_results.iter().enumerate().take(3) {
            calls.push(WebSearchCall {
                id: format!("{base_call_id}_find_{}_{}", index + 1, match_index + 1),
                status: "completed",
                action: Some(WebSearchAction::FindInPage {
                    url: doc.url.clone(),
                    pattern: find_result.pattern.clone(),
                }),
            });
        }
    }

    calls.push(WebSearchCall {
        id: format!("{base_call_id}_done"),
        status: "completed",
        action: None,
    });

    calls
}

fn build_action_sources(result: &SearchResponse) -> Vec<WebSearchSource> {
    let mut sources = Vec::new();
    for hit in &result.hits {
        push_action_source(&mut sources, &hit.url, &hit.title);
    }
    for doc in &result.evidence {
        push_action_source(&mut sources, &doc.url, &doc.title);
    }
    sources
}

fn push_action_source(sources: &mut Vec<WebSearchSource>, url: &str, title: &str) {
    if url.trim().is_empty() || sources.iter().any(|source| source.url == url) {
        return;
    }
    let _ = title;
    sources.push(WebSearchSource {
        kind: "url".to_string(),
        url: url.to_string(),
    });
}

fn query_terms(query: &str) -> Vec<String> {
    const STOP_WORDS: &[&str] = &[
        "about", "after", "also", "best", "code", "does", "find", "from", "have", "how", "into",
        "just", "more", "page", "search", "that", "their", "there", "they", "this", "tool", "what",
        "when", "where", "which", "with", "would",
    ];

    let mut terms = Vec::new();
    for term in query
        .split(|ch: char| !ch.is_alphanumeric())
        .map(str::trim)
        .filter(|term| term.len() >= 3 || term.chars().all(|ch| ch.is_ascii_digit()))
    {
        let lowered = term.to_ascii_lowercase();
        if STOP_WORDS.contains(&lowered.as_str()) || terms.contains(&lowered) {
            continue;
        }
        terms.push(lowered);
    }
    terms
}

fn query_patterns(query: &str) -> Vec<String> {
    let terms = query_terms(query);
    let mut patterns: Vec<String> = Vec::new();

    for quoted in extract_quoted_phrases(query) {
        push_unique_owned(&mut patterns, normalize_pattern(&quoted));
    }

    for focus in extract_focus_patterns(query) {
        push_unique_owned(&mut patterns, normalize_pattern(&focus));
    }

    for window_size in (2..=4).rev() {
        if terms.len() < window_size {
            continue;
        }
        for window in terms.windows(window_size) {
            let phrase = window.join(" ");
            push_unique_owned(&mut patterns, phrase.clone());
            if window_size >= 2 {
                push_unique_owned(
                    &mut patterns,
                    phrase.replace(' ', "-").trim_matches('-').to_string(),
                );
            }
        }
    }

    if terms
        .iter()
        .any(|term| matches!(term.as_str(), "start" | "begin" | "setup" | "install"))
    {
        for expansion in [
            "getting started",
            "quickstart",
            "quick start",
            "init",
            "installation",
        ] {
            push_unique_owned(&mut patterns, expansion.to_string());
        }
    }

    for term in terms {
        push_unique_owned(&mut patterns, term);
    }

    patterns.retain(|pattern| !pattern.trim().is_empty());
    patterns
}

fn extract_quoted_phrases(query: &str) -> Vec<String> {
    let mut phrases = Vec::new();
    let mut current = String::new();
    let mut in_quotes = false;

    for ch in query.chars() {
        match ch {
            '"' | '\'' => {
                if in_quotes {
                    let trimmed = current.trim();
                    if !trimmed.is_empty() {
                        phrases.push(trimmed.to_string());
                    }
                    current.clear();
                }
                in_quotes = !in_quotes;
            }
            _ if in_quotes => current.push(ch),
            _ => {}
        }
    }

    phrases
}

fn extract_focus_patterns(query: &str) -> Vec<String> {
    query
        .split_whitespace()
        .map(|part| part.trim_matches(|ch: char| !(ch.is_alphanumeric() || ch == '_' || ch == '-')))
        .filter(|part| !part.is_empty())
        .filter(|part| {
            part.contains('_')
                || part.contains('-')
                || part.chars().any(|ch| ch.is_ascii_digit())
                || (part.len() >= 3 && part.chars().any(|ch| ch.is_ascii_uppercase()))
        })
        .map(str::to_string)
        .collect()
}

fn push_unique_owned(values: &mut Vec<String>, candidate: String) {
    let trimmed = candidate.trim();
    if trimmed.is_empty() || values.iter().any(|value| value == trimmed) {
        return;
    }
    values.push(trimmed.to_string());
}

fn add_url_citations(items: &mut [Value], citations: &[SearchCitation]) {
    if citations.is_empty() {
        return;
    }

    let Some(message) = items
        .iter_mut()
        .find(|item| item.get("type").and_then(Value::as_str) == Some("message"))
    else {
        return;
    };
    let Some(content) = message.get_mut("content").and_then(Value::as_array_mut) else {
        return;
    };
    let Some(output_text) = content
        .iter_mut()
        .find(|part| part.get("type").and_then(Value::as_str) == Some("output_text"))
    else {
        return;
    };
    let Some(text) = output_text
        .get("text")
        .and_then(Value::as_str)
        .map(str::to_string)
    else {
        return;
    };

    let mut updated_text = text;
    if !updated_text.ends_with('\n') {
        updated_text.push('\n');
    }
    updated_text.push('\n');
    updated_text.push_str("Sources:\n");

    let base_len = updated_text.len();
    let mut annotations = output_text
        .get("annotations")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();

    let mut cursor = base_len;
    for (idx, citation) in citations.iter().enumerate() {
        let line = format!("[{}] {} - {}\n", idx + 1, citation.title, citation.url);
        let url_start = cursor + line.find(&citation.url).unwrap_or(0);
        let url_end = url_start + citation.url.len();
        annotations.push(json!({
            "type": "url_citation",
            "title": citation.title,
            "url": citation.url,
            "start_index": url_start,
            "end_index": url_end,
        }));
        cursor += line.len();
        updated_text.push_str(&line);
    }

    output_text["text"] = Value::String(updated_text.trim_end().to_string());
    output_text["annotations"] = Value::Array(annotations);
}

fn load_cached_search(
    root: &Path,
    config: &SearchConfig,
    cache_key: &str,
) -> Result<Option<SearchCacheEntry>> {
    let path = cache_path(root);
    if !path.exists() {
        return Ok(None);
    }
    let raw = fs::read_to_string(&path)
        .with_context(|| format!("failed to read web-search cache {}", path.display()))?;
    let file: SearchCacheFile =
        serde_json::from_str(&raw).context("failed to parse web-search cache")?;
    Ok(file
        .entries
        .get(cache_key)
        .cloned()
        .filter(|entry| unix_ts().saturating_sub(entry.created_at_epoch) <= config.cache_ttl_secs))
}

fn write_cached_search(root: &Path, cache_key: &str, response: &SearchResponse) -> Result<()> {
    let path = cache_path(root);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).with_context(|| {
            format!("failed to create web-search cache dir {}", parent.display())
        })?;
    }

    let mut file = if path.exists() {
        let raw = fs::read_to_string(&path)
            .with_context(|| format!("failed to read web-search cache {}", path.display()))?;
        serde_json::from_str::<SearchCacheFile>(&raw).unwrap_or_default()
    } else {
        SearchCacheFile::default()
    };

    file.entries.insert(
        cache_key.to_string(),
        SearchCacheEntry {
            created_at_epoch: unix_ts(),
            provider: response.provider.clone(),
            hits: response.hits.clone(),
            evidence: response.evidence.clone(),
        },
    );
    let encoded =
        serde_json::to_string_pretty(&file).context("failed to encode web-search cache")?;
    fs::write(&path, encoded)
        .with_context(|| format!("failed to write web-search cache {}", path.display()))
}

fn build_cache_key(query: &SearchQuery, tool_request: &SearchToolRequest) -> String {
    serde_json::to_string(&json!({
        "query": query.text,
        "language": query.language,
        "region": query.region,
        "safe_search": query.safe_search,
        "allowed_domains": tool_request.allowed_domains,
    }))
    .unwrap_or_else(|_| query.text.clone())
}

fn cache_path(root: &Path) -> PathBuf {
    root.join("runtime/web_search_cache.json")
}

fn load_page_cache(root: &Path) -> Result<PageCacheFile> {
    let path = page_cache_path(root);
    if !path.exists() {
        return Ok(PageCacheFile::default());
    }
    let raw = fs::read_to_string(&path)
        .with_context(|| format!("failed to read web-search page cache {}", path.display()))?;
    serde_json::from_str(&raw).context("failed to parse web-search page cache")
}

fn write_page_cache(root: &Path, file: &PageCacheFile) -> Result<()> {
    let path = page_cache_path(root);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).with_context(|| {
            format!(
                "failed to create web-search page cache dir {}",
                parent.display()
            )
        })?;
    }
    let encoded =
        serde_json::to_string_pretty(file).context("failed to encode web-search page cache")?;
    fs::write(&path, encoded)
        .with_context(|| format!("failed to write web-search page cache {}", path.display()))
}

fn prune_expired_page_cache(file: &mut PageCacheFile, ttl_secs: u64) {
    let now = unix_ts();
    file.entries
        .retain(|_, entry| now.saturating_sub(entry.created_at_epoch) <= ttl_secs);
}

fn normalize_url_cache_key(raw: &str) -> String {
    Url::parse(raw)
        .map(|mut url| {
            url.set_fragment(None);
            url.to_string()
        })
        .unwrap_or_else(|_| raw.trim().to_string())
}

fn page_cache_path(root: &Path) -> PathBuf {
    root.join("runtime/web_search_page_cache.json")
}

fn build_agent(config: &SearchConfig) -> Result<ureq::Agent> {
    Ok(ureq::AgentBuilder::new()
        .user_agent(&config.user_agent)
        .timeout(Duration::from_millis(config.timeout_ms))
        .build())
}

fn percent_decode_lossy(input: &str) -> String {
    let replaced = input.replace('+', " ");
    let bytes = replaced.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut idx = 0;
    while idx < bytes.len() {
        if bytes[idx] == b'%' && idx + 2 < bytes.len() {
            if let Ok(value) = u8::from_str_radix(&replaced[idx + 1..idx + 3], 16) {
                out.push(value);
                idx += 3;
                continue;
            }
        }
        out.push(bytes[idx]);
        idx += 1;
    }
    String::from_utf8_lossy(&out).to_string()
}

fn percent_encode_query_value(input: &str) -> String {
    url::form_urlencoded::byte_serialize(input.as_bytes()).collect()
}

fn extract_first_xml_tag_text(xml: &str, tag: &str) -> Option<String> {
    extract_first_xml_tag_text_after(xml, tag, "")
}

fn extract_first_xml_tag_text_after(xml: &str, tag: &str, after: &str) -> Option<String> {
    let haystack = if after.is_empty() {
        xml
    } else {
        let start = xml.find(after)?;
        &xml[start..]
    };
    let open = format!("<{tag}>");
    let close = format!("</{tag}>");
    let start = haystack.find(&open)? + open.len();
    let end = haystack[start..].find(&close)? + start;
    let raw = haystack[start..end].trim();
    let decoded = decode_xml_entities(raw);
    if decoded.is_empty() {
        None
    } else {
        Some(decoded)
    }
}

fn decode_xml_entities(input: &str) -> String {
    input
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&amp;", "&")
        .replace("&quot;", "\"")
        .replace("&apos;", "'")
        .replace("&#10;", "\n")
}

fn best_paragraphs_for_query(query: &str, paragraphs: &[String], limit: usize) -> Vec<String> {
    if limit == 0 || paragraphs.is_empty() {
        return Vec::new();
    }

    let terms = query_terms(query);
    let mut scored = paragraphs
        .iter()
        .enumerate()
        .map(|(index, paragraph)| {
            (
                index,
                score_paragraph_for_query(query, &terms, paragraph),
                paragraph,
            )
        })
        .collect::<Vec<_>>();
    scored.sort_by(|left, right| right.1.cmp(&left.1).then_with(|| left.0.cmp(&right.0)));

    let mut selected = scored
        .into_iter()
        .filter(|(_, score, _)| *score > 0)
        .take(limit)
        .map(|(_, _, paragraph)| paragraph.clone())
        .collect::<Vec<_>>();

    if selected.is_empty() {
        let cleaned_fallback = clean_candidate_paragraphs(paragraphs.to_vec());
        if cleaned_fallback.is_empty() {
            selected = paragraphs
                .iter()
                .map(|paragraph| normalize_ws(paragraph))
                .filter(|paragraph| !paragraph.is_empty())
                .take(limit)
                .collect();
        } else {
            selected = cleaned_fallback.into_iter().take(limit).collect();
        }
    }

    selected
}

fn best_pdf_paragraphs_for_query(
    query: &str,
    sections: &[EvidenceSection],
    limit: usize,
    fallback_text: &str,
) -> Vec<String> {
    if limit == 0 {
        return Vec::new();
    }

    let terms = query_terms(query);
    let mut candidates = Vec::new();
    for section in sections {
        let raw_paragraphs = split_pdf_paragraphs(&section.text);
        let raw_paragraphs = if raw_paragraphs.is_empty() && !section.text.trim().is_empty() {
            vec![trim_text(&section.text, 1200)]
        } else {
            raw_paragraphs
        };
        let paragraphs = fallback_candidate_paragraphs(raw_paragraphs);
        for paragraph in paragraphs {
            candidates.push((section.page_number, paragraph));
        }
    }

    let mut scored = candidates
        .into_iter()
        .enumerate()
        .map(|(index, (page_number, paragraph))| {
            (
                index,
                score_paragraph_for_query(query, &terms, &paragraph),
                page_number,
                paragraph,
            )
        })
        .collect::<Vec<_>>();
    scored.sort_by(|left, right| right.1.cmp(&left.1).then_with(|| left.0.cmp(&right.0)));

    let mut selected = scored
        .iter()
        .filter(|(_, score, _, _)| *score > 0)
        .take(limit)
        .map(|(_, _, page_number, paragraph)| format_pdf_excerpt(*page_number, paragraph))
        .collect::<Vec<_>>();

    if selected.is_empty() {
        selected = sections
            .iter()
            .filter(|section| !section.text.trim().is_empty())
            .take(limit)
            .map(|section| format_pdf_excerpt(section.page_number, &trim_text(&section.text, 240)))
            .collect();
    }

    if selected.is_empty() && !fallback_text.trim().is_empty() {
        selected.push(trim_text(fallback_text, 240));
    }

    dedupe_texts(selected)
}

fn format_pdf_excerpt(page_number: Option<u32>, text: &str) -> String {
    let excerpt = trim_text(text, 240);
    match page_number {
        Some(page) if !excerpt.is_empty() => format!("p. {}: {}", page, excerpt),
        _ => excerpt,
    }
}

fn score_paragraph_for_query(query: &str, terms: &[String], paragraph: &str) -> usize {
    if paragraph.trim().is_empty() || is_low_value_paragraph(paragraph) {
        return 0;
    }

    let lowered = paragraph.to_ascii_lowercase();
    let query_lowered = query.to_ascii_lowercase();
    let term_hits = terms
        .iter()
        .filter(|term| lowered.contains(term.as_str()))
        .count();
    let mut score = term_hits * 100;
    if query_lowered.len() >= 10 && lowered.contains(query_lowered.as_str()) {
        score += 150;
    }
    if (80..=420).contains(&paragraph.len()) {
        score += 20;
    }
    score
}

fn select_text(document: &Html, selector: &str) -> Option<String> {
    let selector = Selector::parse(selector).ok()?;
    let text = document
        .select(&selector)
        .next()
        .map(|node| normalize_ws(&node.text().collect::<Vec<_>>().join(" ")))?;
    if text.is_empty() {
        None
    } else {
        Some(text)
    }
}

fn select_attr(document: &Html, selector: &str, attr: &str) -> Option<String> {
    let selector = Selector::parse(selector).ok()?;
    let value = document
        .select(&selector)
        .next()
        .and_then(|node| node.value().attr(attr))
        .map(normalize_ws)?;
    if value.is_empty() {
        None
    } else {
        Some(value)
    }
}

fn select_relevant_html_blocks(document: &Html) -> Vec<String> {
    let focused = select_scoped_html_blocks(
        document,
        "article, main, [role='main'], .content, .main-content, .article, .post-content, .entry-content, .prose",
        "h1, h2, h3, p, li, pre, blockquote, dt, dd",
    );
    if !focused.is_empty() {
        return focused;
    }
    let blocks = select_scoped_html_blocks(
        document,
        "body, html",
        "article p, article li, main p, main li, section p, section li, p, li, pre, blockquote",
    );
    if blocks.is_empty() {
        Vec::new()
    } else {
        blocks
    }
}

fn select_docs_html_blocks(document: &Html) -> Vec<String> {
    let focused = select_scoped_html_blocks_relaxed(
        document,
        ".theme-doc-markdown, .content__default, .md-content, .rst-content, .gitbook-markdown, .docs-content, .documentation, .vp-doc, article, main, [role='main']",
        "h1, h2, h3, h4, p, li, pre, blockquote, dt, dd",
    );
    if !focused.is_empty() {
        return focused;
    }
    select_scoped_html_blocks_relaxed(
        document,
        "body, html",
        "h1, h2, h3, p, li, pre, blockquote, dt, dd",
    )
}

fn select_knowledge_html_blocks(document: &Html) -> Vec<String> {
    let focused = select_scoped_html_blocks_relaxed(
        document,
        ".mw-parser-output, #mw-content-text, main#content, #content, article, main, [role='main'], .abstract, .abstract-content, .abstract-full, .article__body, .main-content",
        "h1, h2, h3, p, li, dt, dd, blockquote, pre",
    )
    .into_iter()
    .filter(|text| !is_low_value_knowledge_block(text))
    .collect::<Vec<_>>();
    if !focused.is_empty() {
        return focused;
    }
    select_scoped_html_blocks_relaxed(
        document,
        "body, html",
        "h1, h2, h3, p, li, dt, dd, blockquote, pre",
    )
    .into_iter()
    .filter(|text| !is_low_value_knowledge_block(text))
    .collect()
}

fn select_news_html_blocks(document: &Html) -> Vec<String> {
    let focused = select_scoped_html_blocks_relaxed(
        document,
        "article, main, [role='main'], .article-body, .story-body, .article__content, .entry-content, .post-content, .article-content, .caas-body, .story-content",
        "h1, h2, h3, p, li, blockquote, pre",
    )
    .into_iter()
    .filter(|text| !is_low_value_news_block(text))
    .collect::<Vec<_>>();
    if !focused.is_empty() {
        return focused;
    }
    select_scoped_html_blocks_relaxed(document, "body, html", "h1, h2, h3, p, li, blockquote, pre")
        .into_iter()
        .filter(|text| !is_low_value_news_block(text))
        .collect()
}

fn select_scoped_html_blocks(
    document: &Html,
    root_selector: &str,
    block_selector: &str,
) -> Vec<String> {
    let Some(root_selector) = Selector::parse(root_selector).ok() else {
        return Vec::new();
    };
    let Some(block_selector) = Selector::parse(block_selector).ok() else {
        return Vec::new();
    };

    let mut best_blocks = Vec::new();
    let mut best_score = 0usize;
    for root in document.select(&root_selector) {
        let blocks = root
            .select(&block_selector)
            .filter(|node| !node_has_blocked_ancestor(*node))
            .map(|node| normalize_ws(&node.text().collect::<Vec<_>>().join(" ")))
            .collect::<Vec<_>>();
        let cleaned = clean_candidate_paragraphs(blocks);
        let score = score_html_block_set(&cleaned);
        if score > best_score {
            best_score = score;
            best_blocks = cleaned;
        }
    }

    best_blocks
}

fn select_scoped_html_blocks_relaxed(
    document: &Html,
    root_selector: &str,
    block_selector: &str,
) -> Vec<String> {
    let Some(root_selector) = Selector::parse(root_selector).ok() else {
        return Vec::new();
    };
    let Some(block_selector) = Selector::parse(block_selector).ok() else {
        return Vec::new();
    };

    let mut best_blocks = Vec::new();
    let mut best_score = 0usize;
    for root in document.select(&root_selector) {
        let blocks = root
            .select(&block_selector)
            .filter(|node| !node_has_blocked_ancestor(*node))
            .map(|node| normalize_ws(&node.text().collect::<Vec<_>>().join(" ")))
            .filter(|text| !text.is_empty())
            .filter(|text| !is_low_value_docs_block(text))
            .collect::<Vec<_>>();
        let cleaned = dedupe_texts(blocks);
        let score = score_html_block_set(&cleaned);
        if score > best_score {
            best_score = score;
            best_blocks = cleaned;
        }
    }

    best_blocks
}

fn score_html_block_set(blocks: &[String]) -> usize {
    let text_len = blocks.iter().map(String::len).sum::<usize>();
    let medium_blocks = blocks
        .iter()
        .filter(|block| (30..=600).contains(&block.len()))
        .count();
    text_len + medium_blocks * 120 + blocks.len() * 40
}

fn is_low_value_docs_block(text: &str) -> bool {
    let lowered = text.to_ascii_lowercase();
    if [
        "privacy policy",
        "terms of service",
        "cookie settings",
        "sign in",
        "log in",
        "on this page",
        "edit this page",
        "table of contents",
    ]
    .iter()
    .any(|marker| lowered.contains(marker))
    {
        return true;
    }

    let word_count = text.split_whitespace().count();
    let separator_count = text.matches('|').count() + text.matches('>').count();
    text.len() < 6 || word_count == 0 || separator_count >= 4
}

fn is_low_value_knowledge_block(text: &str) -> bool {
    let lowered = text.to_ascii_lowercase();
    if [
        "contents",
        "table of contents",
        "external links",
        "references",
        "further reading",
        "navigation menu",
        "authority control",
        "coordinates:",
    ]
    .iter()
    .any(|marker| lowered == *marker || lowered.starts_with(&format!("{marker} ")))
    {
        return true;
    }
    is_low_value_docs_block(text)
}

fn is_low_value_news_block(text: &str) -> bool {
    let lowered = text.to_ascii_lowercase();
    if [
        "advertisement",
        "read more",
        "most read",
        "related articles",
        "live updates",
        "watch live",
        "share this article",
        "follow us",
    ]
    .iter()
    .any(|marker| lowered.contains(marker))
    {
        return true;
    }
    is_low_value_docs_block(text)
}

fn node_has_blocked_ancestor(node: ElementRef<'_>) -> bool {
    node.ancestors()
        .filter_map(ElementRef::wrap)
        .any(|ancestor| element_is_blocked_container(&ancestor))
}

fn element_is_blocked_container(element: &ElementRef<'_>) -> bool {
    matches!(
        element.value().name(),
        "nav"
            | "header"
            | "footer"
            | "aside"
            | "form"
            | "button"
            | "script"
            | "style"
            | "noscript"
            | "svg"
    ) || element_matches_marker(element, "class")
        || element_matches_marker(element, "id")
}

fn element_matches_marker(element: &ElementRef<'_>, attr: &str) -> bool {
    let Some(value) = element.value().attr(attr) else {
        return false;
    };
    let lowered = value.to_ascii_lowercase();
    [
        "menu",
        "sidebar",
        "navbar",
        "topnav",
        "sidenav",
        "footer",
        "header",
        "cookie",
        "consent",
        "infobox",
        "mw-table-of-contents",
        "table-of-contents",
        "breadcrumb",
        "subscribe",
        "newsletter",
        "signup",
        "sign-in",
        "signin",
        "login",
        "related",
        "promo",
        "advert",
        "social",
    ]
    .iter()
    .any(|marker| lowered.contains(marker))
}

fn clean_candidate_paragraphs(paragraphs: Vec<String>) -> Vec<String> {
    let cleaned = paragraphs
        .into_iter()
        .map(|paragraph| normalize_ws(&paragraph))
        .filter(|paragraph| !is_low_value_paragraph(paragraph))
        .collect::<Vec<_>>();
    dedupe_texts(cleaned)
}

fn dedupe_texts(texts: Vec<String>) -> Vec<String> {
    let mut deduped = Vec::new();
    for text in texts {
        if deduped.iter().any(|seen: &String| seen == &text) {
            continue;
        }
        deduped.push(text);
    }
    deduped
}

fn is_low_value_paragraph(text: &str) -> bool {
    if text.len() < 40 {
        return true;
    }

    let lowered = text.to_ascii_lowercase();
    if [
        "privacy policy",
        "terms of service",
        "cookie settings",
        "all rights reserved",
        "sign in",
        "log in",
        "subscribe",
        "newsletter",
        "javascript is required",
    ]
    .iter()
    .any(|marker| lowered.contains(marker))
    {
        return true;
    }

    let word_count = text.split_whitespace().count();
    let separator_count = text.matches('|').count() + text.matches('>').count();
    word_count < 8 || separator_count >= 3
}

fn normalize_text(text: &str) -> Option<String> {
    let normalized = normalize_ws(text);
    if normalized.is_empty() {
        None
    } else {
        Some(normalized)
    }
}

fn normalize_ws(input: &str) -> String {
    input.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn trim_query(input: &str) -> String {
    trim_text(input, 240)
}

#[cfg(test)]
fn duplicate_fd(fd: i32, label: &str) -> Result<i32> {
    let duplicated = unsafe { libc::dup(fd) };
    if duplicated < 0 {
        return Err(anyhow!(
            "failed to duplicate {label} during quiet PDF extraction"
        ));
    }
    Ok(duplicated)
}

fn trim_text(input: &str, max_len: usize) -> String {
    let normalized = normalize_ws(input);
    let mut chars = normalized.chars();
    let trimmed: String = chars.by_ref().take(max_len).collect();
    if chars.next().is_some() {
        format!("{trimmed}...")
    } else {
        trimmed
    }
}

fn display_url(raw: &str) -> String {
    Url::parse(raw)
        .ok()
        .and_then(|url| url.host_str().map(str::to_owned))
        .unwrap_or_else(|| raw.to_string())
}

fn url_matches_allowed_domains(url: &str, allowed_domains: &[String]) -> bool {
    let host = Url::parse(url)
        .ok()
        .and_then(|url| url.host_str().map(str::to_ascii_lowercase));
    host.is_some_and(|host| {
        allowed_domains
            .iter()
            .map(|domain| normalize_domain(domain))
            .any(|domain| host == domain || host.ends_with(&format!(".{domain}")))
    })
}

fn normalize_domain(domain: &str) -> String {
    domain
        .trim()
        .trim_start_matches("https://")
        .trim_start_matches("http://")
        .trim_start_matches("*.")
        .trim_end_matches('/')
        .to_ascii_lowercase()
}

fn read_bool(root: &Path, key: &str, default: bool) -> bool {
    runtime_config::get(root, key)
        .map(|value| {
            matches!(
                value.trim().to_ascii_lowercase().as_str(),
                "1" | "true" | "yes" | "on"
            )
        })
        .unwrap_or(default)
}

fn read_u64(root: &Path, key: &str, default: u64) -> u64 {
    runtime_config::get(root, key)
        .and_then(|value| value.parse::<u64>().ok())
        .unwrap_or(default)
}

fn read_usize(root: &Path, key: &str, default: usize) -> usize {
    runtime_config::get(root, key)
        .and_then(|value| value.parse::<usize>().ok())
        .unwrap_or(default)
}

fn unix_ts() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

#[cfg(test)]
mod tests {
    use super::*;
    #[cfg(unix)]
    use std::os::fd::FromRawFd;
    use std::{env, fs, time::Instant};

    #[derive(Clone)]
    struct OpenAiBenchmarkCase {
        id: &'static str,
        prompt: &'static str,
        allowed_domains: &'static [&'static str],
        expected_markers: &'static [&'static str],
        ctox_url_override: Option<&'static str>,
    }

    #[derive(Debug, Clone)]
    struct OpenAiBenchmarkResult {
        output_text: String,
        citations: Vec<String>,
        open_page_urls: Vec<String>,
        source_urls: Vec<String>,
        queries: Vec<String>,
        action_types: Vec<String>,
    }

    fn openai_benchmark_cases() -> Vec<OpenAiBenchmarkCase> {
        vec![
            OpenAiBenchmarkCase {
                id: "github_root_start",
                prompt: "How do I start rustlings? Answer briefly.",
                allowed_domains: &["github.com"],
                expected_markers: &["rustlings init"],
                ctox_url_override: Some("https://github.com/rust-lang/rustlings"),
            },
            OpenAiBenchmarkCase {
                id: "github_blob_print",
                prompt: "In rustlings intro1.rs, what does main print?",
                allowed_domains: &["github.com"],
                expected_markers: &["println!", "Hello from Rustlings"],
                ctox_url_override: Some(
                    "https://github.com/rust-lang/rustlings/blob/main/exercises/00_intro/intro1.rs",
                ),
            },
            OpenAiBenchmarkCase {
                id: "github_tree_intro_location",
                prompt: "In rustlings, where is the intro1.rs exercise?",
                allowed_domains: &["github.com"],
                expected_markers: &["exercises/00_intro/intro1.rs", "00_intro/intro1.rs"],
                ctox_url_override: Some("https://github.com/rust-lang/rustlings"),
            },
            OpenAiBenchmarkCase {
                id: "docs_docusaurus_create",
                prompt: "How do you create a new Docusaurus site?",
                allowed_domains: &["docusaurus.io"],
                expected_markers: &["npm create docusaurus@latest", "create docusaurus"],
                ctox_url_override: None,
            },
            OpenAiBenchmarkCase {
                id: "knowledge_rust_design",
                prompt: "What is Rust designed for?",
                allowed_domains: &["wikipedia.org"],
                expected_markers: &["performance and safety", "safe concurrency"],
                ctox_url_override: Some(
                    "https://en.wikipedia.org/wiki/Rust_(programming_language)",
                ),
            },
            OpenAiBenchmarkCase {
                id: "news_prohuman_declaration",
                prompt: "In the TechCrunch article 'A roadmap for AI, if anyone will listen', what declaration did the article say was signed?",
                allowed_domains: &["techcrunch.com"],
                expected_markers: &["Pro-Human AI Declaration"],
                ctox_url_override: Some(
                    "https://techcrunch.com/2026/03/07/a-roadmap-for-ai-if-anyone-will-listen/",
                ),
            },
        ]
    }

    fn openai_api_key() -> Result<String> {
        env::var("OPENAI_API_KEY").context("OPENAI_API_KEY is required for OpenAI benchmark")
    }

    fn run_openai_web_search_case(case: &OpenAiBenchmarkCase) -> Result<OpenAiBenchmarkResult> {
        let api_key = openai_api_key()?;
        let payload = json!({
            "model": "gpt-5",
            "input": case.prompt,
            "tools": [{
                "type": "web_search",
                "filters": { "allowed_domains": case.allowed_domains },
            }],
            "include": ["web_search_call.action.sources"],
        });
        let encoded =
            serde_json::to_string(&payload).context("failed to encode OpenAI benchmark payload")?;
        let mut last_error: Option<anyhow::Error> = None;
        for attempt in 0..3 {
            let response = ureq::AgentBuilder::new()
                .timeout(Duration::from_secs(150))
                .build()
                .post("https://api.openai.com/v1/responses")
                .set("authorization", &format!("Bearer {}", api_key))
                .set("content-type", "application/json")
                .send_string(&encoded);
            match response {
                Ok(response) => {
                    let value: Value = serde_json::from_reader(response.into_reader())
                        .context("failed to decode OpenAI benchmark response")?;
                    return parse_openai_benchmark_result(&value);
                }
                Err(err) => {
                    last_error = Some(
                        anyhow!(err).context("failed to call OpenAI responses API for benchmark"),
                    );
                    if attempt < 2 {
                        std::thread::sleep(Duration::from_secs((attempt + 1) as u64 * 2));
                    }
                }
            }
        }
        Err(last_error
            .unwrap_or_else(|| anyhow!("failed to call OpenAI responses API for benchmark")))
    }

    fn parse_openai_benchmark_result(payload: &Value) -> Result<OpenAiBenchmarkResult> {
        let output = payload
            .get("output")
            .and_then(Value::as_array)
            .context("OpenAI benchmark response missing output array")?;

        let mut output_text = String::new();
        let mut citations = Vec::new();
        let mut open_page_urls = Vec::new();
        let mut source_urls = Vec::new();
        let mut queries = Vec::new();
        let mut action_types = Vec::new();

        for item in output {
            match item.get("type").and_then(Value::as_str) {
                Some("message") => {
                    if let Some(content) = item.get("content").and_then(Value::as_array) {
                        for part in content {
                            if part.get("type").and_then(Value::as_str) == Some("output_text") {
                                if let Some(text) = part.get("text").and_then(Value::as_str) {
                                    output_text.push_str(text);
                                }
                                if let Some(annotations) =
                                    part.get("annotations").and_then(Value::as_array)
                                {
                                    for annotation in annotations {
                                        if annotation.get("type").and_then(Value::as_str)
                                            == Some("url_citation")
                                        {
                                            if let Some(url) =
                                                annotation.get("url").and_then(Value::as_str)
                                            {
                                                push_unique_string(&mut citations, url);
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
                Some("web_search_call") => {
                    if let Some(action) = item.get("action") {
                        if action.is_null() {
                            continue;
                        }
                        if let Some(action_type) = action.get("type").and_then(Value::as_str) {
                            push_unique_string(&mut action_types, action_type);
                            match action_type {
                                "search" => {
                                    if let Some(query) = action.get("query").and_then(Value::as_str)
                                    {
                                        push_unique_string(&mut queries, query);
                                    }
                                    if let Some(all_queries) =
                                        action.get("queries").and_then(Value::as_array)
                                    {
                                        for query in all_queries.iter().filter_map(Value::as_str) {
                                            push_unique_string(&mut queries, query);
                                        }
                                    }
                                    if let Some(sources) =
                                        action.get("sources").and_then(Value::as_array)
                                    {
                                        for source in sources {
                                            if let Some(url) =
                                                source.get("url").and_then(Value::as_str)
                                            {
                                                push_unique_string(&mut source_urls, url);
                                            }
                                        }
                                    }
                                }
                                "open_page" => {
                                    if let Some(url) = action.get("url").and_then(Value::as_str) {
                                        push_unique_string(&mut open_page_urls, url);
                                    }
                                }
                                _ => {}
                            }
                        }
                    }
                }
                _ => {}
            }
        }

        Ok(OpenAiBenchmarkResult {
            output_text,
            citations,
            open_page_urls,
            source_urls,
            queries,
            action_types,
        })
    }

    fn benchmark_candidate_urls(
        case: &OpenAiBenchmarkCase,
        openai: &OpenAiBenchmarkResult,
    ) -> Vec<String> {
        if let Some(url) = case.ctox_url_override {
            return vec![url.to_string()];
        }

        let mut urls = Vec::new();
        for url in &openai.citations {
            push_unique_string(&mut urls, url);
        }
        for url in &openai.open_page_urls {
            push_unique_string(&mut urls, url);
        }
        for url in &openai.source_urls {
            push_unique_string(&mut urls, url);
        }
        urls
    }

    fn run_ctox_benchmark_case(
        config: &SearchConfig,
        case: &OpenAiBenchmarkCase,
        url: &str,
    ) -> Result<EvidenceDoc> {
        let hit = SearchHit {
            title: display_url(url),
            url: url.to_string(),
            snippet: String::new(),
            source: "benchmark".to_string(),
            rank: 1,
        };
        fetch_evidence_doc(config, case.prompt, &hit)
    }

    fn text_matches_expected_markers(text: &str, markers: &[&str]) -> bool {
        let lowered = text.to_ascii_lowercase();
        markers
            .iter()
            .any(|marker| lowered.contains(&marker.to_ascii_lowercase()))
    }

    fn push_unique_string(values: &mut Vec<String>, candidate: &str) {
        if candidate.trim().is_empty() || values.iter().any(|value| value == candidate) {
            return;
        }
        values.push(candidate.to_string());
    }

    #[test]
    fn extracts_web_search_request_fields() {
        let payload = json!({
            "tools": [{
                "type": "web_search",
                "external_web_access": false,
                "search_context_size": "high",
                "filters": { "allowed_domains": ["example.com"] },
                "user_location": { "type": "approximate", "country": "US", "city": "New York" }
            }]
        });
        let request = extract_web_search_request(&payload).expect("web search tool");
        assert_eq!(request.external_web_access, Some(false));
        assert_eq!(request.allowed_domains, vec!["example.com"]);
        assert_eq!(request.search_context_size, Some(ContextSize::High));
        assert_eq!(request.user_location.country.as_deref(), Some("US"));
        assert_eq!(request.user_location.city.as_deref(), Some("New York"));
    }

    #[test]
    fn detects_include_sources_request() {
        let payload = json!({
            "include": ["web_search_call.action.sources", "something.else"]
        });
        assert!(request_includes_sources(&payload));
    }

    #[test]
    fn canonical_request_extraction_captures_query_and_include_sources() {
        let payload = json!({
            "tools": [{
                "type": "web_search",
                "search_context_size": "medium",
                "filters": { "allowed_domains": ["docs.example.com"] }
            }],
            "include": ["web_search_call.action.sources"],
            "input": [
                {"role":"user","content":[{"type":"input_text","text":"latest query"}]}
            ]
        });
        let request =
            canonical_web_search_request_from_responses(&payload).expect("canonical web search");
        assert_eq!(request.query, "latest query");
        assert_eq!(request.allowed_domains, vec!["docs.example.com"]);
        assert_eq!(request.search_context_size, Some(ContextSize::Medium));
        assert!(request.include_sources);
    }

    #[test]
    fn strips_web_search_tools_after_ctox_owns_search() {
        let mut payload = json!({
            "tools": [
                {"type":"web_search","search_context_size":"medium"},
                {
                    "type":"namespace",
                    "tools":[
                        {"type":"web_search","search_context_size":"low"},
                        {"type":"function","name":"exec_command","parameters":{"type":"object"}}
                    ]
                },
                {"type":"function","name":"write_stdin","parameters":{"type":"object"}}
            ]
        });
        assert!(strip_web_search_tools(&mut payload));
        assert_eq!(
            payload["tools"],
            json!([
                {
                    "type":"namespace",
                    "tools":[
                        {"type":"function","name":"exec_command","parameters":{"type":"object"}}
                    ]
                },
                {"type":"function","name":"write_stdin","parameters":{"type":"object"}}
            ])
        );
    }

    #[test]
    fn openai_web_search_mode_defaults_to_ctox_primary() {
        let root = unique_test_root("web_search_mode_default");
        assert_eq!(
            OpenAiWebSearchCompatMode::from_root(&root),
            OpenAiWebSearchCompatMode::CtoxPrimary
        );
    }

    #[test]
    fn openai_web_search_mode_reads_passthrough_override() {
        let root = unique_test_root("web_search_mode_passthrough");
        set_runtime_config(&root, "CTOX_WEB_SEARCH_OPENAI_MODE", "passthrough");
        assert_eq!(
            OpenAiWebSearchCompatMode::from_root(&root),
            OpenAiWebSearchCompatMode::Passthrough
        );
    }

    #[test]
    fn openai_web_search_mode_accepts_tui_labels() {
        let root = unique_test_root("web_search_mode_tui_labels");
        set_runtime_config(&root, "CTOX_WEB_SEARCH_OPENAI_MODE", "openai");
        assert_eq!(
            OpenAiWebSearchCompatMode::from_root(&root),
            OpenAiWebSearchCompatMode::Passthrough
        );
        set_runtime_config(&root, "CTOX_WEB_SEARCH_OPENAI_MODE", "local_stack");
        assert_eq!(
            OpenAiWebSearchCompatMode::from_root(&root),
            OpenAiWebSearchCompatMode::CtoxPrimary
        );
    }

    #[test]
    fn augment_request_strips_native_tool_and_injects_context() {
        let root = unique_test_root("augment_request_ctox_primary");
        set_runtime_config(&root, "CTOX_WEB_SEARCH_PROVIDER", "mock");

        let mut payload = json!({
            "tools": [
                {"type":"web_search","search_context_size":"medium"},
                {"type":"function","name":"exec_command","parameters":{"type":"object"}}
            ],
            "input": [
                {"role":"user","content":[{"type":"input_text","text":"find CTOX remote web info"}]}
            ]
        });
        let augmentation = augment_responses_request(&root, &mut payload).unwrap();
        assert!(augmentation.is_some());
        assert_eq!(
            payload["tools"],
            json!([
                {"type":"function","name":"exec_command","parameters":{"type":"object"}}
            ])
        );
        let input = payload["input"].as_array().expect("input array");
        assert_eq!(input.last().unwrap()["role"], "developer");
        assert!(input.last().unwrap()["content"][0]["text"]
            .as_str()
            .unwrap()
            .contains("CTOX executed a local web_search request"));
    }

    #[test]
    fn ctox_web_search_tool_returns_namespaced_payload() {
        let root = unique_test_root("ctox_web_search_tool");
        set_runtime_config(&root, "CTOX_WEB_SEARCH_PROVIDER", "mock");

        let payload = run_ctox_web_search_tool(
            &root,
            &CanonicalWebSearchRequest {
                query: "ctox web search runtime".to_string(),
                external_web_access: Some(true),
                allowed_domains: Vec::new(),
                user_location: SearchUserLocation::default(),
                search_context_size: Some(ContextSize::Medium),
                search_content_types: Vec::new(),
                include_sources: true,
            },
        )
        .unwrap();

        assert_eq!(payload["tool"], "ctox_web_search");
        assert_eq!(payload["ok"], json!(true));
        assert_eq!(
            payload["results"][0]["url"],
            "https://example.com/mock-result"
        );
    }

    #[test]
    fn ctox_web_read_tool_returns_find_results() {
        let root = unique_test_root("ctox_web_read_tool");
        set_runtime_config(&root, "CTOX_WEB_SEARCH_PROVIDER", "mock");

        let payload = run_ctox_web_read_tool(
            &root,
            &DirectWebReadRequest {
                url: "https://example.com/mock-result".to_string(),
                query: Some("ctox web search evidence".to_string()),
                find: vec!["CTOX_REMOTE_WEB_OK".to_string()],
            },
        )
        .unwrap();

        assert_eq!(payload["tool"], "ctox_web_read");
        assert_eq!(payload["ok"], json!(true));
        assert_eq!(payload["url"], "https://example.com/mock-result");
        assert_eq!(payload["find_results"][0]["pattern"], "ctox remote web ok");
    }

    #[test]
    fn extracts_latest_user_query_from_structured_input() {
        let payload = json!({
            "input": [
                {"role":"user","content":[{"type":"input_text","text":"first"}]},
                {"role":"developer","content":[{"type":"input_text","text":"note"}]},
                {"role":"user","content":[{"type":"input_text","text":"latest query"}]}
            ]
        });
        assert_eq!(
            extract_latest_user_query(&payload).as_deref(),
            Some("latest query")
        );
    }

    #[test]
    fn augments_output_with_web_search_call_and_citations() {
        let payload = json!({
            "id": "resp_1",
            "output": [{
                "type": "message",
                "role": "assistant",
                "content": [{"type":"output_text","text":"ok","annotations":[]}]
            }]
        });
        let augmentation = WebSearchAugmentation {
            calls: vec![
                WebSearchCall {
                    id: "ws_1".to_string(),
                    status: "completed",
                    action: Some(WebSearchAction::Search {
                        query: "weather berlin".to_string(),
                        queries: vec!["weather berlin".to_string()],
                        sources: Some(vec![WebSearchSource {
                            kind: "url".to_string(),
                            url: "https://example.com/weather".to_string(),
                        }]),
                    }),
                },
                WebSearchCall {
                    id: "ws_1_open_1".to_string(),
                    status: "completed",
                    action: Some(WebSearchAction::OpenPage {
                        url: "https://example.com/weather".to_string(),
                    }),
                },
                WebSearchCall {
                    id: "ws_1_find_1".to_string(),
                    status: "completed",
                    action: Some(WebSearchAction::FindInPage {
                        url: "https://example.com/weather".to_string(),
                        pattern: "berlin".to_string(),
                    }),
                },
                WebSearchCall {
                    id: "ws_1_done".to_string(),
                    status: "completed",
                    action: None,
                },
            ],
            citations: vec![SearchCitation {
                title: "Weather".to_string(),
                url: "https://example.com/weather".to_string(),
            }],
        };
        let rewritten = augment_responses_output(
            serde_json::to_vec(&payload).unwrap().as_slice(),
            &augmentation,
        )
        .unwrap();
        let value: Value = serde_json::from_slice(&rewritten).unwrap();
        assert_eq!(value["output"][0]["type"], "web_search_call");
        assert_eq!(value["output"][0]["action"]["query"], "weather berlin");
        assert_eq!(
            value["output"][0]["action"]["sources"][0]["url"],
            "https://example.com/weather"
        );
        assert_eq!(value["output"][1]["action"]["type"], "open_page");
        assert_eq!(value["output"][2]["action"]["type"], "find_in_page");
        assert!(value["output"][3]["action"].is_null());
        assert_eq!(
            value["output"][4]["content"][0]["annotations"][0]["type"],
            "url_citation"
        );
    }

    #[test]
    fn filters_hits_by_allowed_domain() {
        let hits = vec![
            SearchHit {
                title: "A".to_string(),
                url: "https://docs.example.com/a".to_string(),
                snippet: String::new(),
                source: "mock".to_string(),
                rank: 1,
            },
            SearchHit {
                title: "B".to_string(),
                url: "https://other.test/b".to_string(),
                snippet: String::new(),
                source: "mock".to_string(),
                rank: 2,
            },
        ];
        let filtered = filter_hits_by_domain(hits, &["example.com".to_string()]);
        assert_eq!(filtered.len(), 1);
        assert_eq!(filtered[0].url, "https://docs.example.com/a");
    }

    #[test]
    fn mock_open_page_builds_real_excerpts_and_find_results() {
        let config = test_config(ProviderKind::Mock);
        let hit = SearchHit {
            title: "Mock result".to_string(),
            url: "https://example.com/mock-result".to_string(),
            snippet: "Mock snippet".to_string(),
            source: "mock".to_string(),
            rank: 1,
        };
        let evidence = fetch_evidence_doc(&config, "find CTOX_REMOTE_WEB_OK in page", &hit)
            .expect("mock evidence");
        assert!(!evidence.excerpts.is_empty());
        assert!(evidence.page_text.contains("CTOX_REMOTE_WEB_OK"));
        assert!(evidence
            .find_results
            .iter()
            .flat_map(|result| result.matches.iter())
            .any(|matched| matched.contains("CTOX_REMOTE_WEB_OK")));
    }

    #[test]
    fn build_web_search_calls_uses_real_find_results() {
        let result = SearchResponse {
            provider: "mock".to_string(),
            hits: Vec::new(),
            evidence: vec![EvidenceDoc {
                url: "https://example.com/mock-result".to_string(),
                title: "Mock".to_string(),
                summary: "Summary".to_string(),
                is_pdf: false,
                pdf_total_pages: None,
                page_sections: Vec::new(),
                excerpts: vec!["Excerpt".to_string()],
                page_text: "CTOX_REMOTE_WEB_OK".to_string(),
                find_results: vec![FindInPageResult {
                    pattern: "ctox_remote_web_ok".to_string(),
                    matches: vec!["CTOX_REMOTE_WEB_OK".to_string()],
                }],
            }],
            executed_queries: vec!["find CTOX_REMOTE_WEB_OK".to_string()],
        };
        let calls = build_web_search_calls("ws_1", &result, true);
        assert_eq!(calls.len(), 4);
        assert_eq!(
            calls[0].output_item()["action"]["query"],
            "find CTOX_REMOTE_WEB_OK"
        );
        assert_eq!(
            calls[0].output_item()["action"]["queries"][0],
            "find CTOX_REMOTE_WEB_OK"
        );
        assert_eq!(
            calls[0].output_item()["action"]["sources"][0]["type"],
            "url"
        );
        assert_eq!(
            calls[0].output_item()["action"]["sources"][0]["url"],
            "https://example.com/mock-result"
        );
        assert!(matches!(
            calls[1].action,
            Some(WebSearchAction::OpenPage { .. })
        ));
        assert!(matches!(
            calls[2].action,
            Some(WebSearchAction::FindInPage { .. })
        ));
        assert!(calls[3].output_item()["action"].is_null());
    }

    #[test]
    fn find_in_page_matches_normalized_hyphenated_phrase() {
        let matches = build_find_in_page_results(
            "What is the Pro-Human AI Declaration?",
            "The Pro-Human AI Declaration was announced in the article.",
            &[],
            &[],
        );
        assert!(
            matches
                .iter()
                .flat_map(|result| result.matches.iter())
                .any(|matched| matched.contains("Pro-Human AI Declaration")),
            "expected normalized phrase match, got {:?}",
            matches
        );
    }

    #[test]
    fn find_in_page_expands_start_queries_to_init_and_getting_started() {
        let matches = build_find_in_page_results(
            "How do I start rustlings?",
            "See the Getting Started guide, then run `rustlings init` in your shell.",
            &[],
            &[],
        );
        assert!(
            matches
                .iter()
                .flat_map(|result| result.matches.iter())
                .any(|matched| matched.contains("rustlings init")
                    || matched.contains("Getting Started")),
            "expected start-query expansion match, got {:?}",
            matches
        );
    }

    #[test]
    fn find_in_page_matches_hyphenated_docs_commands() {
        let matches = build_find_in_page_results(
            "How do I create a Docusaurus site?",
            "Use the create-docusaurus command to scaffold a new site quickly.",
            &[],
            &[],
        );
        assert!(
            matches
                .iter()
                .flat_map(|result| result.matches.iter())
                .any(|matched| matched.contains("create-docusaurus")),
            "expected hyphenated docs command match, got {:?}",
            matches
        );
    }

    #[test]
    fn planned_search_queries_include_keyword_pdf_and_year_rewrites() {
        let queries = plan_search_queries(
            "Use web search and tell me the filing requirements from page 8 of the IRS instructions for Form 1040 PDF.",
            &["irs.gov".to_string()],
        );
        assert!(!queries.is_empty());
        assert!(queries[0].contains("site:irs.gov"));
        assert!(queries
            .iter()
            .any(|query| query.to_ascii_lowercase().contains("page 8")));
        assert!(queries
            .iter()
            .any(|query| query.to_ascii_lowercase().contains("pdf")));

        let now_secs = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs() as i64;
        let current_year = chrono::DateTime::<chrono::Utc>::from_timestamp(now_secs, 0)
            .map(|dt| dt.year())
            .unwrap_or(2026);
        assert!(queries
            .iter()
            .any(|query| query.contains(&(current_year - 1).to_string())));
        assert!(queries
            .iter()
            .any(|query| query.contains(&(current_year - 2).to_string())));
    }

    #[test]
    fn bing_rss_parser_extracts_search_hits() {
        let payload = r#"<?xml version="1.0" encoding="utf-8" ?>
<rss version="2.0">
  <channel>
    <item>
      <title>Docusaurus Introduction</title>
      <link>https://docusaurus.io/docs</link>
      <description>Create a new Docusaurus site with create-docusaurus@latest.</description>
    </item>
    <item>
      <title>Rustlings Repo</title>
      <link>https://github.com/rust-lang/rustlings</link>
      <description>Small exercises to get you used to reading and writing Rust code.</description>
    </item>
  </channel>
</rss>"#;
        let hits = parse_bing_rss_results(payload, 0, 5).expect("bing rss results");
        assert_eq!(hits.len(), 2);
        assert_eq!(hits[0].url, "https://docusaurus.io/docs");
        assert!(hits[0].snippet.contains("create-docusaurus@latest"));
        assert_eq!(hits[1].rank, 2);
    }

    #[test]
    fn duckduckgo_html_parser_extracts_search_hits() {
        let payload = r#"
<html>
  <body>
    <div class="result">
      <a class="result__a" href="//duckduckgo.com/l/?uddg=https%3A%2F%2Fwww.artisan.co%2F">Artisan</a>
      <a class="result__snippet">AI employees for go-to-market teams.</a>
    </div>
    <div class="result">
      <a class="result__a" href="https://relevanceai.com/">Relevance AI</a>
      <div class="result__snippet">Build and run an AI workforce.</div>
    </div>
  </body>
</html>"#;
        let hits = parse_duckduckgo_html_results(payload, 0, 5).expect("duckduckgo html results");
        assert_eq!(hits.len(), 2);
        assert_eq!(hits[0].url, "https://www.artisan.co/");
        assert!(hits[0].snippet.contains("AI employees"));
        assert_eq!(hits[1].url, "https://relevanceai.com/");
        assert_eq!(hits[1].rank, 2);
    }

    #[test]
    fn duckduckgo_accept_language_derives_from_query_language() {
        assert_eq!(
            super::duckduckgo_accept_language(Some("de-DE")),
            "de-DE,de;q=0.9,en;q=0.8"
        );
        assert_eq!(super::duckduckgo_accept_language(Some("fr")), "fr,en;q=0.8");
        assert_eq!(super::duckduckgo_accept_language(None), "en-US,en;q=0.9");
        assert_eq!(
            super::duckduckgo_accept_language(Some("  ")),
            "en-US,en;q=0.9"
        );
    }

    #[test]
    fn duckduckgo_anomaly_modal_is_detected() {
        let body = r#"<html><body><div class="anomaly-modal__title">Unfortunately, bots use DuckDuckGo too.</div></body></html>"#;
        assert!(super::detect_duckduckgo_anomaly(body));
        let clean = r#"<html><body><div class="result"><a class="result__a" href="/x">x</a></div></body></html>"#;
        assert!(!super::detect_duckduckgo_anomaly(clean));
    }

    #[test]
    fn parse_chrome_major_extracts_version_from_default_ua() {
        let ua = "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/136.0.0.0 Safari/537.36";
        assert_eq!(super::parse_chrome_major_from_ua(ua), Some(136));
        assert_eq!(super::parse_chrome_major_from_ua("ctox-test"), None);
    }

    // The stdout-capture path uses process-wide dup2, which races with any
    // other test thread that also writes to stdout. That makes this test
    // flaky under `cargo test` default parallelism. Run it explicitly with
    // `cargo test -- --test-threads=1 ctox_web_read_tool_keeps_stdout_clean_for_pdf_reads --ignored`.
    #[cfg(unix)]
    #[ignore = "requires --test-threads=1 because stdout capture races other test threads"]
    #[test]
    fn ctox_web_read_tool_keeps_stdout_clean_for_pdf_reads() {
        let root = unique_test_root("ctox_web_read_pdf_stdout");
        set_runtime_config(&root, "CTOX_WEB_SEARCH_PROVIDER", "mock");

        let (captured_stdout, payload) = capture_stdout(|| {
            run_ctox_web_read_tool(
                &root,
                &DirectWebReadRequest {
                    url: "https://example.com/mock-result.pdf".to_string(),
                    query: Some("Attention Is All You Need".to_string()),
                    find: vec!["Attention Is All You Need".to_string()],
                },
            )
            .expect("pdf read payload")
        });

        assert!(
            captured_stdout.trim().is_empty(),
            "expected PDF read to keep stdout clean, got {:?}",
            captured_stdout
        );
        assert_eq!(payload["tool"], "ctox_web_read");
        assert_eq!(payload["is_pdf"], json!(true));
        assert!(payload["page_text_excerpt"]
            .as_str()
            .unwrap_or_default()
            .contains("Attention Is All You Need"));
    }

    #[test]
    fn detects_pdf_by_content_type_and_signature() {
        let hit = SearchHit {
            title: "Doc".to_string(),
            url: "https://example.com/doc".to_string(),
            snippet: String::new(),
            source: "mock".to_string(),
            rank: 1,
        };
        let fetched = FetchedPageContent {
            body: b"%PDF-1.4 test".to_vec(),
            content_type: Some("application/pdf".to_string()),
            final_url: "https://example.com/doc".to_string(),
        };
        assert!(is_pdf_content(&hit, &fetched));
    }

    #[test]
    fn mock_pdf_open_page_extracts_llm_readable_text() {
        let config = test_config(ProviderKind::Mock);
        let hit = SearchHit {
            title: "Mock PDF".to_string(),
            url: "https://example.com/mock-result.pdf".to_string(),
            snippet: "Mock PDF snippet".to_string(),
            source: "mock".to_string(),
            rank: 1,
        };
        let evidence = fetch_evidence_doc(&config, "find CTOX_REMOTE_WEB_OK in pdf", &hit)
            .expect("mock pdf evidence");
        assert!(evidence.page_text.contains("CTOX_REMOTE_WEB_OK"));
        assert!(!evidence.excerpts.is_empty());
        assert!(evidence
            .find_results
            .iter()
            .flat_map(|result| result.matches.iter())
            .any(|matched| matched.contains("CTOX_REMOTE_WEB_OK")));
    }

    #[test]
    fn bounded_pdf_extraction_respects_page_limit() {
        let mut config = test_config(ProviderKind::Mock);
        config.max_pdf_pages = 1;
        config.max_page_chars = 4_000;

        let hit = SearchHit {
            title: "Paged PDF".to_string(),
            url: "https://example.com/paged.pdf".to_string(),
            snippet: "Paged snippet".to_string(),
            source: "mock".to_string(),
            rank: 1,
        };
        let fetched = FetchedPageContent {
            body: mock_pdf_bytes_with_pages(&[
                "First page keeps the CTOX_PRIMARY_TOKEN visible.",
                "Second page should never be reached and mentions CTOX_SECONDARY_TOKEN.",
            ]),
            content_type: Some("application/pdf".to_string()),
            final_url: hit.url.clone(),
        };

        let opened = extract_pdf_opened_page(&config, "ctox primary token", &hit, &fetched)
            .expect("bounded pdf extraction");
        assert!(opened.page_text.contains("CTOX_PRIMARY_TOKEN"));
        assert!(!opened.page_text.contains("CTOX_SECONDARY_TOKEN"));
    }

    #[test]
    fn pdf_page_hint_loads_targeted_page_and_labels_matches() {
        let mut config = test_config(ProviderKind::Mock);
        config.max_pdf_pages = 5;
        config.max_page_chars = 8_000;

        let hit = SearchHit {
            title: "Paged PDF".to_string(),
            url: "https://example.com/paged.pdf".to_string(),
            snippet: "Paged snippet".to_string(),
            source: "mock".to_string(),
            rank: 1,
        };
        let fetched = FetchedPageContent {
            body: mock_pdf_bytes_with_pages(&[
                "Page one intro only.",
                "Page two intro only.",
                "Page three intro only.",
                "Page four intro only.",
                "Page five contains CTOX_TARGET_PAGE_TOKEN for the focused query.",
                "Page six fallback only.",
            ]),
            content_type: Some("application/pdf".to_string()),
            final_url: hit.url.clone(),
        };

        let opened = extract_pdf_opened_page(
            &config,
            "check page 5 for CTOX_TARGET_PAGE_TOKEN",
            &hit,
            &fetched,
        )
        .expect("page-hinted pdf extraction");

        assert_eq!(opened.pdf_total_pages, Some(6));
        assert!(opened
            .page_sections
            .iter()
            .any(|section| section.page_number == Some(5)));
        assert!(opened.page_text.contains("[Page 5]"));
        assert!(opened
            .excerpts
            .iter()
            .any(|excerpt| excerpt.starts_with("p. 5:")));

        let doc = build_query_evidence_doc(
            &config,
            "check page 5 for CTOX_TARGET_PAGE_TOKEN",
            &hit,
            hit.url.clone(),
            opened,
        );
        assert!(doc
            .find_results
            .iter()
            .flat_map(|result| result.matches.iter())
            .any(|matched| matched.starts_with("p. 5:")));
    }

    #[test]
    fn html_extraction_filters_navigation_and_cookie_boilerplate() {
        let hit = SearchHit {
            title: "Test page".to_string(),
            url: "https://example.com/page".to_string(),
            snippet: "Snippet".to_string(),
            source: "mock".to_string(),
            rank: 1,
        };
        let html = r#"<!doctype html>
<html>
  <body>
    <header><p>Home | Pricing | Support</p></header>
    <div class="cookie-banner"><p>Privacy Policy Terms of Service Cookie Settings</p></div>
    <main>
      <article>
        <p>The CTOX web search runtime opens source pages and extracts the most relevant evidence for the active query.</p>
        <p>This paragraph explains how open_page and find_in_page stay aligned with the same extracted page text.</p>
      </article>
    </main>
    <footer><p>All rights reserved.</p></footer>
  </body>
</html>"#;
        let opened = extract_opened_page("ctox web search evidence", &hit, html);
        assert!(opened
            .page_text
            .contains("The CTOX web search runtime opens source pages"));
        assert!(!opened.page_text.contains("Privacy Policy Terms of Service"));
        assert!(!opened.page_text.contains("Home | Pricing | Support"));
    }

    #[test]
    fn generic_html_prefers_main_content_over_sidebar_noise() {
        let hit = SearchHit {
            title: "Docs page".to_string(),
            url: "https://example.com/docs/page".to_string(),
            snippet: "Snippet".to_string(),
            source: "mock".to_string(),
            rank: 1,
        };
        let html = r#"<!doctype html>
<html>
  <body>
    <aside class="sidebar">
      <p>Intro Getting Started Installation API Reference Changelog Support Pricing Company Blog Careers</p>
      <p>More links More links More links More links More links More links More links More links</p>
    </aside>
    <main>
      <article>
        <h1>Installation</h1>
        <p>The CTOX runtime can mount a reviewed web_search adapter pipeline so models can read pages with less boilerplate and better passage selection.</p>
        <p>Use the install command with the provided token and restart the local service after the environment file has been written.</p>
      </article>
    </main>
  </body>
</html>"#;
        let opened = extract_opened_page("install runtime command", &hit, html);
        assert!(opened
            .page_text
            .contains("The CTOX runtime can mount a reviewed web_search adapter pipeline"));
        assert!(!opened.page_text.contains("Pricing Company Blog Careers"));
    }

    #[test]
    fn docs_adapter_extracts_docusaurus_markdown_and_code_blocks() {
        let hit = SearchHit {
            title: "Docusaurus docs".to_string(),
            url: "https://docusaurus.io/docs".to_string(),
            snippet: "Docs".to_string(),
            source: "mock".to_string(),
            rank: 1,
        };
        let html = r#"<!doctype html>
<html>
  <body>
    <main>
      <article class="theme-doc-markdown markdown">
        <h1>Getting Started</h1>
        <p>Docusaurus helps you ship structured docs sites with versioning and search.</p>
        <pre><code>npm create docusaurus@latest my-site classic</code></pre>
      </article>
    </main>
  </body>
</html>"#;
        let opened = extract_opened_page("docusaurus npm create", &hit, html);
        assert_eq!(detect_page_adapter(&hit, html), PageAdapterKind::DocsSite);
        assert!(opened
            .page_text
            .contains("Docusaurus helps you ship structured docs sites"));
        assert!(opened
            .page_text
            .contains("npm create docusaurus@latest my-site classic"));
    }

    #[test]
    fn knowledge_adapter_extracts_wikipedia_article_body() {
        let hit = SearchHit {
            title: "Rust (programming language)".to_string(),
            url: "https://en.wikipedia.org/wiki/Rust_(programming_language)".to_string(),
            snippet: "Wikipedia".to_string(),
            source: "mock".to_string(),
            rank: 1,
        };
        let html = r#"<!doctype html>
<html>
  <body>
    <main id="content">
      <table class="infobox"><tr><td>Paradigm multi-paradigm</td></tr></table>
      <div class="mw-parser-output">
        <h1>Rust (programming language)</h1>
        <p>Rust is a multi-paradigm, general-purpose programming language designed for performance and safety, especially safe concurrency.</p>
        <p>Rust achieves memory safety without garbage collection, and the language was originally designed at Mozilla Research.</p>
        <h2>References</h2>
        <p>External links and citations.</p>
      </div>
    </main>
  </body>
</html>"#;
        let opened = extract_opened_page("memory safety rust", &hit, html);
        assert_eq!(
            detect_page_adapter(&hit, html),
            PageAdapterKind::KnowledgeSite
        );
        assert_eq!(opened.title, "Rust (programming language)");
        assert!(opened
            .page_text
            .contains("general-purpose programming language"));
        assert!(!opened.page_text.contains("Paradigm multi-paradigm"));
        assert!(!opened.page_text.contains("External links and citations"));
    }

    #[test]
    fn knowledge_adapter_extracts_arxiv_abstract() {
        let hit = SearchHit {
            title: "Attention Is All You Need".to_string(),
            url: "https://arxiv.org/abs/1706.03762".to_string(),
            snippet: "ArXiv".to_string(),
            source: "mock".to_string(),
            rank: 1,
        };
        let html = r#"<!doctype html>
<html>
  <body>
    <main id="content">
      <h1>Attention Is All You Need</h1>
      <blockquote class="abstract mathjax">
        <span class="descriptor">Abstract:</span>
        The dominant sequence transduction models are based on complex recurrent or convolutional neural networks that include an encoder and a decoder.
      </blockquote>
      <div class="extra-services">Download PDF Other formats</div>
    </main>
  </body>
</html>"#;
        let opened = extract_opened_page("sequence transduction encoder decoder", &hit, html);
        assert_eq!(
            detect_page_adapter(&hit, html),
            PageAdapterKind::KnowledgeSite
        );
        assert!(opened
            .page_text
            .contains("dominant sequence transduction models"));
        assert!(!opened.page_text.contains("Download PDF Other formats"));
    }

    #[test]
    fn news_adapter_extracts_article_body_and_drops_related_blocks() {
        let hit = SearchHit {
            title: "Markets rally on rate hopes".to_string(),
            url: "https://www.reuters.com/world/us/markets-rally-rate-hopes/".to_string(),
            snippet: "Reuters".to_string(),
            source: "mock".to_string(),
            rank: 1,
        };
        let html = r#"<!doctype html>
<html>
  <head>
    <meta property="og:type" content="article" />
    <meta property="og:title" content="Markets rally on rate hopes" />
  </head>
  <body>
    <aside class="related-links"><p>Related articles Most read Watch live</p></aside>
    <main>
      <article class="article-body">
        <h1>Markets rally on rate hopes</h1>
        <p>Stocks rose on Thursday after fresh inflation data strengthened expectations that central banks may start easing later this year.</p>
        <p>Analysts said investors were focusing on bond yields, labor-market signals and company guidance during the session.</p>
      </article>
    </main>
  </body>
</html>"#;
        let opened = extract_opened_page("inflation data easing bond yields", &hit, html);
        assert_eq!(detect_page_adapter(&hit, html), PageAdapterKind::NewsSite);
        assert_eq!(opened.title, "Markets rally on rate hopes");
        assert!(opened
            .page_text
            .contains("Stocks rose on Thursday after fresh inflation data"));
        assert!(!opened
            .page_text
            .contains("Related articles Most read Watch live"));
    }

    #[test]
    fn github_tree_embedded_data_extracts_repo_items() {
        let hit = SearchHit {
            title: "rustlings".to_string(),
            url: "https://github.com/rust-lang/rustlings/tree/main/exercises".to_string(),
            snippet: "Tree".to_string(),
            source: "mock".to_string(),
            rank: 1,
        };
        let html = r#"<!doctype html>
<html>
  <body>
    <script type="application/json" data-target="react-app.embeddedData">{
      "payload": {
        "codeViewRepoRoute": {
          "path": "exercises",
          "tree": {
            "items": [
              {"name":"00_intro","path":"exercises/00_intro","contentType":"directory"},
              {"name":"README.md","path":"exercises/README.md","contentType":"file"}
            ]
          }
        }
      }
    }</script>
  </body>
</html>"#;
        let opened = extract_opened_page("readme intro", &hit, html);
        assert_eq!(opened.title, "GitHub tree: exercises");
        assert!(opened.page_text.contains("directory: exercises/00_intro"));
        assert!(opened.page_text.contains("file: exercises/README.md"));
    }

    #[test]
    fn github_tree_embedded_data_extracts_items_from_code_view_tree_route() {
        let hit = SearchHit {
            title: "rustlings".to_string(),
            url: "https://github.com/rust-lang/rustlings/tree/main/exercises/00_intro".to_string(),
            snippet: "Tree".to_string(),
            source: "mock".to_string(),
            rank: 1,
        };
        let html = r#"<!doctype html>
<html>
  <body>
    <script type="application/json" data-target="react-app.embeddedData">{
      "payload": {
        "codeViewTreeRoute": {
          "path": "exercises/00_intro",
          "tree": {
            "items": [
              {"name":"intro1.rs","path":"exercises/00_intro/intro1.rs","contentType":"file"},
              {"name":"intro2.rs","path":"exercises/00_intro/intro2.rs","contentType":"file"}
            ]
          }
        },
        "codeViewFileTreeLayoutRoute": {
          "fileTree": {
            "exercises/00_intro": {
              "items": [
                {"name":"intro1.rs","path":"exercises/00_intro/intro1.rs","contentType":"file"},
                {"name":"intro2.rs","path":"exercises/00_intro/intro2.rs","contentType":"file"}
              ]
            }
          }
        }
      }
    }</script>
  </body>
</html>"#;
        let opened = extract_opened_page("where is intro1.rs", &hit, html);
        assert_eq!(opened.title, "GitHub tree: exercises/00_intro");
        assert!(opened
            .page_text
            .contains("file: exercises/00_intro/intro1.rs"));
        assert!(opened
            .excerpts
            .iter()
            .any(|excerpt| excerpt.contains("intro1.rs")));
    }

    #[test]
    fn github_markdown_embedded_data_extracts_readme_text() {
        let hit = SearchHit {
            title: "Rustlings".to_string(),
            url: "https://github.com/rust-lang/rustlings".to_string(),
            snippet: "Rustlings repo".to_string(),
            source: "mock".to_string(),
            rank: 1,
        };
        let html = r#"<!doctype html>
<html>
  <body>
    <script type="application/json" data-target="react-app.embeddedData">{
      "payload": {
        "codeViewRepoRoute": {
          "overview": {
            "overviewFiles": [{
              "path": "README.md",
              "richText": "<article class=\"markdown-body\"><h1>Rustlings</h1><p>Small exercises to get you used to reading and writing Rust code.</p><p>Run <code>cargo install rustlings</code> to get started.</p></article>"
            }]
          }
        }
      }
    }</script>
  </body>
</html>"#;
        let opened = extract_opened_page("rustlings rust code", &hit, html);
        assert_eq!(opened.title, "Rustlings");
        assert!(opened
            .page_text
            .contains("Small exercises to get you used to reading and writing Rust code."));
        assert!(opened.page_text.contains("cargo install rustlings"));
    }

    #[test]
    fn github_code_embedded_data_extracts_raw_lines() {
        let hit = SearchHit {
            title: "intro1.rs".to_string(),
            url: "https://github.com/rust-lang/rustlings/blob/main/exercises/00_intro/intro1.rs"
                .to_string(),
            snippet: "Rustlings intro file".to_string(),
            source: "mock".to_string(),
            rank: 1,
        };
        let html = r#"<!doctype html>
<html>
  <body>
    <script type="application/json" data-target="react-app.embeddedData">{
      "payload": {
        "codeViewLayoutRoute": {
          "path": "exercises/00_intro/intro1.rs"
        },
        "codeViewBlobLayoutRoute.StyledBlob": {
          "rawLines": [
            "// Welcome to Rustlings",
            "fn main() {",
            "    println!(\"Hello from Rustlings\");",
            "}"
          ]
        }
      }
    }</script>
  </body>
</html>"#;
        let opened = extract_opened_page("println rustlings", &hit, html);
        assert_eq!(opened.title, "GitHub file: exercises/00_intro/intro1.rs");
        assert!(opened.page_text.contains("2: fn main() {"));
        assert!(opened
            .page_text
            .contains("3:     println!(\"Hello from Rustlings\");"));
        assert!(opened
            .excerpts
            .iter()
            .any(|excerpt| excerpt.contains("println!")));
    }

    #[test]
    fn parses_github_repo_tree_and_blob_urls() {
        let repo =
            parse_github_url_parts("https://github.com/rust-lang/rustlings").expect("repo url");
        assert_eq!(repo.owner, "rust-lang");
        assert_eq!(repo.repo, "rustlings");
        assert_eq!(repo.kind, GithubUrlKind::RepoRoot);

        let tree = parse_github_url_parts(
            "https://github.com/rust-lang/rustlings/tree/main/exercises/00_intro",
        )
        .expect("tree url");
        assert_eq!(tree.kind, GithubUrlKind::Tree);
        assert_eq!(tree.ref_name.as_deref(), Some("main"));
        assert_eq!(tree.path.as_deref(), Some("exercises/00_intro"));

        let blob =
            parse_github_url_parts("https://github.com/rust-lang/rustlings/blob/main/src/main.rs")
                .expect("blob url");
        assert_eq!(blob.kind, GithubUrlKind::Blob);
        assert_eq!(blob.path.as_deref(), Some("src/main.rs"));
    }

    #[test]
    fn github_api_repo_payload_surfaces_query_relevant_files() {
        let hit = SearchHit {
            title: "rust-lang/rustlings".to_string(),
            url: "https://github.com/rust-lang/rustlings".to_string(),
            snippet: "Rustlings repo".to_string(),
            source: "mock".to_string(),
            rank: 1,
        };
        let payload = GithubApiPayload {
            kind: "repo_root".to_string(),
            title: "GitHub repo: rust-lang/rustlings".to_string(),
            repo: "rust-lang/rustlings".to_string(),
            path: None,
            description: "Small exercises to get you used to reading and writing Rust code."
                .to_string(),
            readme: "Rustlings helps you practice Rust. Run cargo install rustlings to install the binary."
                .to_string(),
            entries: vec![
                "file: README.md".to_string(),
                "file: Cargo.toml".to_string(),
                "directory: src".to_string(),
                "file: install.sh".to_string(),
            ],
            supplemental_files: vec![
                GithubApiFile {
                    path: "src/main.rs".to_string(),
                    text: "fn main() {\n    println!(\"run rustlings init to set up the exercises\");\n}".to_string(),
                },
                GithubApiFile {
                    path: "install.sh".to_string(),
                    text: "cargo install rustlings\nrustlings init\n".to_string(),
                },
            ],
        };

        let opened = github_api_payload_opened_page("how do i start rustlings", &hit, &payload)
            .expect("github api repo page");
        assert!(opened.page_text.contains("File: src/main.rs"));
        assert!(opened.page_text.contains("rustlings init"));
        assert!(opened
            .excerpts
            .iter()
            .any(|excerpt| excerpt.contains("rustlings init")));
    }

    #[test]
    fn github_api_blob_payload_extracts_numbered_code_lines() {
        let hit = SearchHit {
            title: "main.rs".to_string(),
            url: "https://github.com/rust-lang/rustlings/blob/main/src/main.rs".to_string(),
            snippet: "Rustlings main".to_string(),
            source: "mock".to_string(),
            rank: 1,
        };
        let payload = GithubApiPayload {
            kind: "blob".to_string(),
            title: "GitHub file: src/main.rs".to_string(),
            repo: "rust-lang/rustlings".to_string(),
            path: Some("src/main.rs".to_string()),
            description: String::new(),
            readme: String::new(),
            entries: Vec::new(),
            supplemental_files: vec![GithubApiFile {
                path: "src/main.rs".to_string(),
                text: "fn main() {\n    println!(\"rustlings init\");\n}".to_string(),
            }],
        };

        let opened = github_api_payload_opened_page("init command", &hit, &payload)
            .expect("github api blob page");
        assert!(opened.page_text.contains("1: fn main() {"));
        assert!(opened
            .excerpts
            .iter()
            .any(|excerpt| excerpt.contains("rustlings init")));
    }

    #[test]
    fn github_api_repo_payload_keeps_short_directory_summary_lines() {
        let hit = SearchHit {
            title: "rust-lang/rustlings".to_string(),
            url: "https://github.com/rust-lang/rustlings".to_string(),
            snippet: "Rustlings repo".to_string(),
            source: "mock".to_string(),
            rank: 1,
        };
        let payload = GithubApiPayload {
            kind: "repo_root".to_string(),
            title: "GitHub repo: rust-lang/rustlings".to_string(),
            repo: "rust-lang/rustlings".to_string(),
            path: None,
            description: String::new(),
            readme: String::new(),
            entries: vec!["directory: exercises".to_string()],
            supplemental_files: vec![
                GithubApiFile {
                    path: "exercises/".to_string(),
                    text: "directory: exercises/00_intro\nfile: exercises/README.md".to_string(),
                },
                GithubApiFile {
                    path: "exercises/00_intro/".to_string(),
                    text: "file: exercises/00_intro/intro1.rs\nfile: exercises/00_intro/intro2.rs"
                        .to_string(),
                },
            ],
        };

        let opened =
            github_api_payload_opened_page("where is the intro1.rs exercise", &hit, &payload)
                .expect("github api repo page");
        assert!(opened
            .page_text
            .contains("file: exercises/00_intro/intro1.rs"));
        assert!(!opened.summary.trim().is_empty());
    }

    #[test]
    fn github_directory_selection_prefers_query_relevant_paths() {
        let root_entries = vec![
            GithubContentEntry {
                name: "src".to_string(),
                path: "src".to_string(),
                kind: "dir".to_string(),
            },
            GithubContentEntry {
                name: "exercises".to_string(),
                path: "exercises".to_string(),
                kind: "dir".to_string(),
            },
        ];
        let selected =
            select_github_directory_paths("where is the intro1.rs exercise", &root_entries, None);
        assert_eq!(selected.first().map(String::as_str), Some("exercises"));

        let child_entries = vec![
            GithubContentEntry {
                name: "00_intro".to_string(),
                path: "exercises/00_intro".to_string(),
                kind: "dir".to_string(),
            },
            GithubContentEntry {
                name: "99_misc".to_string(),
                path: "exercises/99_misc".to_string(),
                kind: "dir".to_string(),
            },
        ];
        let nested = select_github_directory_paths(
            "where is the intro1.rs exercise",
            &child_entries,
            Some("exercises"),
        );
        assert_eq!(
            nested.first().map(String::as_str),
            Some("exercises/00_intro")
        );
    }

    #[test]
    fn pdf_cleanup_merges_hyphenated_lines_and_drops_numeric_artifacts() {
        let cleaned = clean_pdf_text_for_llm(
            "Executive sum-\nmary for CTOX\n\n12\n13\n14\n\nThe runtime reads PDFs cleanly.",
        );
        assert!(cleaned.contains("Executive summary for CTOX"));
        assert!(!cleaned.contains("\n12\n"));
        assert!(cleaned.contains("The runtime reads PDFs cleanly."));
    }

    #[test]
    fn best_paragraphs_falls_back_to_short_pdf_text_when_needed() {
        let paragraphs = vec!["Dummy PDF file".to_string()];
        let excerpts = best_paragraphs_for_query("dummy pdf", &paragraphs, 3);
        assert_eq!(excerpts, vec!["Dummy PDF file".to_string()]);
    }

    #[test]
    fn session_reuses_persisted_page_cache_without_network() {
        let root = unique_test_root("web_search_page_cache");
        fs::create_dir_all(root.join("runtime")).expect("runtime dir");

        let hit = SearchHit {
            title: "Cached page".to_string(),
            url: "http://127.0.0.1:9/mock-result".to_string(),
            snippet: "Snippet".to_string(),
            source: "mock".to_string(),
            rank: 1,
        };

        let warm_config = test_config(ProviderKind::Mock);
        let mut warm_session = WebSearchSession::new(&root, &warm_config).expect("warm session");
        let warmed = warm_session
            .fetch_evidence_doc("find CTOX_REMOTE_WEB_OK", &hit)
            .expect("warm cache");
        warm_session
            .persist_page_cache()
            .expect("persist page cache");

        let cold_hit = SearchHit {
            source: "google".to_string(),
            ..hit.clone()
        };
        let cold_config = test_config(ProviderKind::Brave);
        let mut cold_session = WebSearchSession::new(&root, &cold_config).expect("cold session");
        let cached = cold_session
            .fetch_evidence_doc("find CTOX_REMOTE_WEB_OK", &cold_hit)
            .expect("load cached page");

        assert_eq!(cached.url, warmed.url);
        assert!(cached.page_text.contains("CTOX_REMOTE_WEB_OK"));
    }

    #[test]
    fn execute_search_uses_cached_search_when_external_web_access_is_disabled() {
        let root = unique_test_root("web_search_cached_search");
        fs::create_dir_all(root.join("runtime")).expect("runtime dir");

        let config = test_config(ProviderKind::Mock);
        let query = SearchQuery {
            text: "ctox cached search".to_string(),
            count: 5,
            offset: 0,
            language: None,
            region: None,
            safe_search: 1,
        };
        let live_request = SearchToolRequest {
            external_web_access: Some(true),
            ..SearchToolRequest::default()
        };
        let cached_request = SearchToolRequest {
            external_web_access: Some(false),
            ..SearchToolRequest::default()
        };

        let warm = execute_search(&root, &config, &live_request, "ctox cached search", &query)
            .expect("warm search");
        let cached = execute_search(
            &root,
            &config,
            &cached_request,
            "ctox cached search",
            &query,
        )
        .expect("cached search");

        assert_eq!(cached.provider, "mock-cached");
        assert_eq!(cached.hits.len(), warm.hits.len());
        assert_eq!(cached.evidence.len(), warm.evidence.len());
    }

    #[test]
    fn execute_search_plans_queries_from_original_prompt_not_augmented_query() {
        let root = unique_test_root("web_search_original_query_plan");
        fs::create_dir_all(root.join("runtime")).expect("runtime dir");

        let config = test_config(ProviderKind::Mock);
        let request = SearchToolRequest {
            external_web_access: Some(true),
            allowed_domains: vec!["github.com".to_string()],
            ..SearchToolRequest::default()
        };
        let query = SearchQuery {
            text: build_search_text(
                "In rustlings, where is the intro1.rs exercise?",
                &request.allowed_domains,
            ),
            count: 5,
            offset: 0,
            language: None,
            region: None,
            safe_search: 1,
        };

        let result = execute_search(
            &root,
            &config,
            &request,
            "In rustlings, where is the intro1.rs exercise?",
            &query,
        )
        .expect("search result");

        assert!(result
            .executed_queries
            .iter()
            .all(|query| !query.contains("site:github.com site:github.com")));
        assert!(result
            .executed_queries
            .iter()
            .any(|query| query.contains("site:github.com")));
    }

    #[test]
    #[ignore = "live network validation for RFC search recall"]
    fn real_world_rfc_editor_search_returns_hits() {
        let root = unique_test_root("real_world_rfc_editor_search");
        fs::create_dir_all(root.join("runtime")).expect("runtime dir");

        let mut config = test_config(ProviderKind::Brave);
        config.timeout_ms = 20_000;
        config.max_page_bytes = 2_000_000;

        let request = SearchToolRequest {
            external_web_access: Some(true),
            allowed_domains: vec!["rfc-editor.org".to_string()],
            search_context_size: Some(ContextSize::Medium),
            include_sources: true,
            ..SearchToolRequest::default()
        };
        let query_text = "RFC 9110 HTTP Semantics";
        let query = SearchQuery {
            text: build_search_text(query_text, &request.allowed_domains),
            count: 5,
            offset: 0,
            language: config.default_language.clone(),
            region: None,
            safe_search: 1,
        };
        let result = execute_search(&root, &config, &request, query_text, &query)
            .expect("rfc-editor search result");

        println!(
            "RFC SEARCH HITS: {:?}",
            result.hits.iter().map(|hit| &hit.url).collect::<Vec<_>>()
        );
        assert!(
            !result.hits.is_empty(),
            "expected at least one hit for RFC 9110 on rfc-editor.org"
        );
        assert!(
            result
                .hits
                .iter()
                .any(|hit| hit.url.to_ascii_lowercase().contains("rfc9110")),
            "expected RFC 9110 URL in hits, got {:?}",
            result.hits
        );
    }

    #[test]
    #[ignore = "live network validation for real-world PDFs"]
    fn real_world_pdf_samples_extract_meaningful_text() {
        let config = test_config(ProviderKind::Brave);
        let cases = [
            (
                "https://www.w3.org/WAI/ER/tests/xhtml/testfiles/resources/pdf/dummy.pdf",
                "Dummy PDF",
            ),
            ("https://www.orimi.com/pdf-test.pdf", "PDF Test File"),
        ];

        for (url, expected) in cases {
            let hit = SearchHit {
                title: format!("Live PDF {expected}"),
                url: url.to_string(),
                snippet: String::new(),
                source: "google".to_string(),
                rank: 1,
            };
            let evidence =
                fetch_evidence_doc(&config, expected, &hit).expect("real-world pdf evidence");
            println!("URL: {}", url);
            println!("TITLE: {}", evidence.title);
            println!("SUMMARY: {}", evidence.summary);
            println!("EXCERPTS: {:?}", evidence.excerpts);
            println!("PAGE_TEXT: {}", trim_text(&evidence.page_text, 800));
            assert!(
                evidence
                    .page_text
                    .to_ascii_lowercase()
                    .contains(&expected.to_ascii_lowercase()),
                "expected extracted page text to contain {:?}, got {:?}",
                expected,
                trim_text(&evidence.page_text, 300)
            );
            assert!(
                !evidence.excerpts.is_empty(),
                "expected non-empty excerpts for {}",
                url
            );
        }
    }

    #[test]
    #[ignore = "live network validation for GitHub repo and blob extraction"]
    fn real_world_github_pages_extract_meaningful_text() {
        let mut config = test_config(ProviderKind::Brave);
        config.max_page_bytes = 2_000_000;
        config.timeout_ms = 20_000;
        let cases = [
            (
                "rustlings exercises rust code",
                SearchHit {
                    title: "rust-lang/rustlings".to_string(),
                    url: "https://github.com/rust-lang/rustlings".to_string(),
                    snippet: "Rustlings repository".to_string(),
                    source: "google".to_string(),
                    rank: 1,
                },
                "writing Rust code",
            ),
            (
                "println intro1 main rustlings",
                SearchHit {
                    title: "intro1.rs".to_string(),
                    url: "https://github.com/rust-lang/rustlings/blob/main/exercises/00_intro/intro1.rs"
                        .to_string(),
                    snippet: "Rustlings intro file".to_string(),
                    source: "google".to_string(),
                    rank: 1,
                },
                "println!",
            ),
        ];

        for (query, hit, expected) in cases {
            let evidence = fetch_evidence_doc(&config, query, &hit).expect("github evidence");
            println!("URL: {}", hit.url);
            println!("TITLE: {}", evidence.title);
            println!("SUMMARY: {}", evidence.summary);
            println!("EXCERPTS: {:?}", evidence.excerpts);
            println!("PAGE_TEXT: {}", trim_text(&evidence.page_text, 800));
            assert!(evidence.page_text.contains(expected), "missing {expected}");
            assert!(
                !evidence.excerpts.is_empty(),
                "expected non-empty excerpts for {}",
                hit.url
            );
            assert!(
                evidence
                    .find_results
                    .iter()
                    .flat_map(|result| result.matches.iter())
                    .next()
                    .is_some(),
                "expected find_in_page matches for {}",
                hit.url
            );
        }
    }

    #[test]
    #[ignore = "live network validation for GitHub repo-root command extraction"]
    fn real_world_github_repo_root_surfaces_start_command() {
        let mut config = test_config(ProviderKind::Brave);
        config.max_page_bytes = 2_000_000;
        config.timeout_ms = 20_000;

        let hit = SearchHit {
            title: "rust-lang/rustlings".to_string(),
            url: "https://github.com/rust-lang/rustlings".to_string(),
            snippet: "Rustlings repository".to_string(),
            source: "google".to_string(),
            rank: 1,
        };
        let evidence =
            fetch_evidence_doc(&config, "how do i start rustlings", &hit).expect("github evidence");
        println!("URL: {}", hit.url);
        println!("TITLE: {}", evidence.title);
        println!("SUMMARY: {}", trim_text(&evidence.summary, 400));
        println!("EXCERPTS: {:?}", evidence.excerpts);
        println!("PAGE_TEXT: {}", trim_text(&evidence.page_text, 1200));
        assert!(
            evidence
                .page_text
                .to_ascii_lowercase()
                .contains("rustlings init")
                || evidence
                    .excerpts
                    .iter()
                    .any(|excerpt| excerpt.to_ascii_lowercase().contains("rustlings init")),
            "expected repo root evidence to surface rustlings init, got {:?}",
            trim_text(&evidence.page_text, 500)
        );
    }

    #[test]
    #[ignore = "live network validation for GitHub repo-root path discovery"]
    fn real_world_github_repo_root_surfaces_intro_exercise_location() {
        let mut config = test_config(ProviderKind::Brave);
        config.max_page_bytes = 2_000_000;
        config.timeout_ms = 20_000;

        let hit = SearchHit {
            title: "rust-lang/rustlings".to_string(),
            url: "https://github.com/rust-lang/rustlings".to_string(),
            snippet: "Rustlings repository".to_string(),
            source: "google".to_string(),
            rank: 1,
        };
        let evidence = fetch_evidence_doc(
            &config,
            "In rustlings, where is the intro1.rs exercise?",
            &hit,
        )
        .expect("github evidence");
        println!("URL: {}", hit.url);
        println!("TITLE: {}", evidence.title);
        println!("SUMMARY: {}", trim_text(&evidence.summary, 400));
        println!("EXCERPTS: {:?}", evidence.excerpts);
        println!("PAGE_TEXT: {}", trim_text(&evidence.page_text, 1500));
        assert!(
            evidence
                .page_text
                .to_ascii_lowercase()
                .contains("exercises/00_intro/intro1.rs")
                || evidence.excerpts.iter().any(|excerpt| excerpt
                    .to_ascii_lowercase()
                    .contains("exercises/00_intro/intro1.rs"))
                || evidence
                    .find_results
                    .iter()
                    .flat_map(|result| result.matches.iter())
                    .any(|entry| entry
                        .to_ascii_lowercase()
                        .contains("exercises/00_intro/intro1.rs")),
            "expected repo root evidence to surface intro1.rs path, got {:?}",
            trim_text(&evidence.page_text, 600)
        );
    }

    #[test]
    #[ignore = "live OpenAI-vs-CTOX compatibility benchmark"]
    fn openai_web_search_compatibility_benchmark() {
        let mut config = test_config(ProviderKind::Brave);
        config.max_page_bytes = 2_000_000;
        config.timeout_ms = 30_000;

        let mut report_rows = Vec::new();
        let mut benchmark_failures = Vec::new();

        for case in openai_benchmark_cases() {
            let openai = match run_openai_web_search_case(&case) {
                Ok(result) => result,
                Err(err) => {
                    benchmark_failures.push(format!("{}:openai_error", case.id));
                    report_rows.push(json!({
                        "id": case.id,
                        "prompt": case.prompt,
                        "allowed_domains": case.allowed_domains,
                        "expected_markers": case.expected_markers,
                        "error": format!("{err:#}"),
                    }));
                    continue;
                }
            };
            let openai_tool_used = !openai.action_types.is_empty()
                || !openai.citations.is_empty()
                || !openai.open_page_urls.is_empty()
                || !openai.source_urls.is_empty();
            if !openai_tool_used {
                report_rows.push(json!({
                    "id": case.id,
                    "prompt": case.prompt,
                    "allowed_domains": case.allowed_domains,
                    "expected_markers": case.expected_markers,
                    "openai": {
                        "actions": openai.action_types,
                        "queries": openai.queries,
                        "citations": openai.citations,
                        "open_page_urls": openai.open_page_urls,
                        "sources": openai.source_urls,
                        "output_text": openai.output_text,
                    },
                    "skipped": "openai_no_tool_use",
                }));
                continue;
            }
            let candidate_urls = benchmark_candidate_urls(&case, &openai);
            assert!(
                !candidate_urls.is_empty(),
                "benchmark case {} produced no usable URL",
                case.id
            );

            let mut selected_url = None;
            let mut selected_doc = None;
            let mut selected_hits = (false, false, false);
            let mut best_score = 0usize;
            let mut last_ctox_error = None;

            for url in &candidate_urls {
                match run_ctox_benchmark_case(&config, &case, url) {
                    Ok(doc) => {
                        let text_hit =
                            text_matches_expected_markers(&doc.page_text, case.expected_markers);
                        let excerpt_hit = doc.excerpts.iter().any(|excerpt| {
                            text_matches_expected_markers(excerpt, case.expected_markers)
                        });
                        let find_hit = doc
                            .find_results
                            .iter()
                            .flat_map(|result| result.matches.iter())
                            .any(|matched| {
                                text_matches_expected_markers(matched, case.expected_markers)
                            });
                        let score = usize::from(text_hit)
                            + usize::from(excerpt_hit)
                            + usize::from(find_hit);
                        if selected_doc.is_none() || score > best_score {
                            best_score = score;
                            selected_url = Some(url.clone());
                            selected_hits = (text_hit, excerpt_hit, find_hit);
                            selected_doc = Some(doc);
                        }
                        if score > 0 {
                            break;
                        }
                    }
                    Err(err) => {
                        last_ctox_error = Some(format!("{err:#}"));
                    }
                }
            }

            let primary_url = selected_url.unwrap_or_else(|| {
                panic!(
                    "CTOX benchmark case {} failed for all URLs: {:?} last_error={:?}",
                    case.id, candidate_urls, last_ctox_error
                )
            });
            let ctox = selected_doc.expect("selected ctox doc");
            let (ctox_text_hit, ctox_excerpt_hit, ctox_find_hit) = selected_hits;
            let openai_output_hit =
                text_matches_expected_markers(&openai.output_text, case.expected_markers);
            let ok = ctox_text_hit || ctox_excerpt_hit || ctox_find_hit;
            if !ok {
                benchmark_failures.push(case.id.to_string());
            }

            println!("\nCASE {}", case.id);
            println!("PROMPT: {}", case.prompt);
            println!("OPENAI_ACTIONS: {:?}", openai.action_types);
            println!("OPENAI_QUERIES: {:?}", openai.queries);
            println!("OPENAI_CITATIONS: {:?}", openai.citations);
            println!("CANDIDATE_URLS: {:?}", candidate_urls);
            println!("PRIMARY_URL: {}", primary_url);
            println!("OPENAI_OUTPUT: {}", trim_text(&openai.output_text, 500));
            println!("CTOX_TITLE: {}", ctox.title);
            println!("CTOX_SUMMARY: {}", trim_text(&ctox.summary, 400));
            println!("CTOX_EXCERPTS: {:?}", ctox.excerpts);
            println!("CTOX_PAGE_TEXT: {}", trim_text(&ctox.page_text, 800));
            println!(
                "RESULT: openai_output_hit={} ctox_text_hit={} ctox_excerpt_hit={} ctox_find_hit={}",
                openai_output_hit, ctox_text_hit, ctox_excerpt_hit, ctox_find_hit
            );

            report_rows.push(json!({
                "id": case.id,
                "prompt": case.prompt,
                "allowed_domains": case.allowed_domains,
                "expected_markers": case.expected_markers,
                "candidate_urls": candidate_urls,
                "primary_url": primary_url,
                "openai": {
                    "actions": openai.action_types,
                    "queries": openai.queries,
                    "citations": openai.citations,
                    "open_page_urls": openai.open_page_urls,
                    "sources": openai.source_urls,
                    "output_text": openai.output_text,
                    "output_hit": openai_output_hit,
                },
                "ctox": {
                    "title": ctox.title,
                    "summary": ctox.summary,
                    "excerpts": ctox.excerpts,
                    "page_text": ctox.page_text,
                    "find_results": ctox.find_results,
                    "text_hit": ctox_text_hit,
                    "excerpt_hit": ctox_excerpt_hit,
                    "find_hit": ctox_find_hit,
                },
                "ok": ok,
            }));
        }

        let report = json!({
            "generated_at_epoch": unix_ts(),
            "cases": report_rows,
            "failures": benchmark_failures,
        });
        let report_path = std::env::temp_dir().join("ctox_openai_web_search_benchmark.json");
        fs::write(
            &report_path,
            serde_json::to_string_pretty(&report).expect("benchmark report encoding"),
        )
        .expect("benchmark report write");
        println!("BENCHMARK_REPORT: {}", report_path.display());

        assert!(
            benchmark_failures.is_empty(),
            "OpenAI compatibility benchmark failures: {:?}",
            benchmark_failures
        );
    }

    #[test]
    #[ignore = "live network validation for docs-site extraction"]
    fn real_world_docs_sites_extract_meaningful_text() {
        let mut config = test_config(ProviderKind::Brave);
        config.max_page_bytes = 2_000_000;
        config.timeout_ms = 20_000;

        let cases = [
            (
                "docusaurus docs getting started",
                SearchHit {
                    title: "Docusaurus docs".to_string(),
                    url: "https://docusaurus.io/docs".to_string(),
                    snippet: "Docusaurus docs".to_string(),
                    source: "google".to_string(),
                    rank: 1,
                },
                "Docusaurus",
            ),
            (
                "read the docs sphinx getting started",
                SearchHit {
                    title: "Read the Docs".to_string(),
                    url: "https://docs.readthedocs.io/en/stable/intro/getting-started-with-sphinx.html"
                        .to_string(),
                    snippet: "Read the Docs".to_string(),
                    source: "google".to_string(),
                    rank: 1,
                },
                "Sphinx",
            ),
        ];

        for (query, hit, expected) in cases {
            let evidence = fetch_evidence_doc(&config, query, &hit).expect("docs evidence");
            println!("URL: {}", hit.url);
            println!("TITLE: {}", evidence.title);
            println!("SUMMARY: {}", evidence.summary);
            println!("EXCERPTS: {:?}", evidence.excerpts);
            println!("PAGE_TEXT: {}", trim_text(&evidence.page_text, 800));
            assert!(evidence.page_text.contains(expected), "missing {expected}");
            assert!(
                !evidence.excerpts.is_empty(),
                "expected non-empty excerpts for {}",
                hit.url
            );
        }
    }

    #[test]
    #[ignore = "live network validation for news-site extraction"]
    fn real_world_news_sites_extract_meaningful_text() {
        let mut config = test_config(ProviderKind::Brave);
        config.max_page_bytes = 2_000_000;
        config.timeout_ms = 20_000;

        let cases = [(
            "pro-human ai declaration pentagon anthropic",
            SearchHit {
                title: "A roadmap for AI, if anyone will listen".to_string(),
                url: "https://techcrunch.com/2026/03/07/a-roadmap-for-ai-if-anyone-will-listen/"
                    .to_string(),
                snippet: "TechCrunch AI article".to_string(),
                source: "google".to_string(),
                rank: 1,
            },
            "Pro-Human AI Declaration",
        )];

        for (query, hit, expected) in cases {
            let evidence = fetch_evidence_doc(&config, query, &hit).expect("news evidence");
            println!("URL: {}", hit.url);
            println!("TITLE: {}", evidence.title);
            println!("SUMMARY: {}", evidence.summary);
            println!("EXCERPTS: {:?}", evidence.excerpts);
            println!("PAGE_TEXT: {}", trim_text(&evidence.page_text, 800));
            assert!(evidence.page_text.contains(expected), "missing {expected}");
            assert!(
                !evidence.excerpts.is_empty(),
                "expected non-empty excerpts for {}",
                hit.url
            );
            assert!(
                evidence
                    .find_results
                    .iter()
                    .flat_map(|result| result.matches.iter())
                    .next()
                    .is_some(),
                "expected find_in_page matches for {}",
                hit.url
            );
        }
    }

    #[test]
    #[ignore = "live network validation for knowledge-site extraction"]
    fn real_world_knowledge_sites_extract_meaningful_text() {
        let mut config = test_config(ProviderKind::Brave);
        config.max_page_bytes = 2_000_000;
        config.timeout_ms = 20_000;

        let cases = [
            (
                "rust programming language memory safety",
                SearchHit {
                    title: "Rust (programming language)".to_string(),
                    url: "https://en.wikipedia.org/wiki/Rust_(programming_language)".to_string(),
                    snippet: "Wikipedia".to_string(),
                    source: "google".to_string(),
                    rank: 1,
                },
                "memory safety",
            ),
            (
                "attention is all you need transformer abstract",
                SearchHit {
                    title: "Attention Is All You Need".to_string(),
                    url: "https://arxiv.org/abs/1706.03762".to_string(),
                    snippet: "arXiv".to_string(),
                    source: "google".to_string(),
                    rank: 1,
                },
                "sequence transduction",
            ),
        ];

        for (query, hit, expected) in cases {
            let evidence = fetch_evidence_doc(&config, query, &hit).expect("knowledge evidence");
            println!("URL: {}", hit.url);
            println!("TITLE: {}", evidence.title);
            println!("SUMMARY: {}", evidence.summary);
            println!("EXCERPTS: {:?}", evidence.excerpts);
            println!("PAGE_TEXT: {}", trim_text(&evidence.page_text, 800));
            assert!(
                evidence
                    .page_text
                    .to_ascii_lowercase()
                    .contains(&expected.to_ascii_lowercase()),
                "missing {:?} in {}",
                expected,
                hit.url
            );
            assert!(
                !evidence.excerpts.is_empty(),
                "expected non-empty excerpts for {}",
                hit.url
            );
        }
    }

    #[test]
    #[ignore = "live network validation for semi-complex PDFs"]
    fn real_world_semicomplex_pdfs_extract_meaningful_text_quickly() {
        let mut config = test_config(ProviderKind::Brave);
        config.timeout_ms = 20_000;
        config.max_page_bytes = 6_000_000;
        config.max_page_chars = 20_000;

        let cases = [
            (
                "https://arxiv.org/pdf/1706.03762.pdf",
                "Attention Is All You Need",
            ),
            (
                "https://www.irs.gov/pub/irs-pdf/i1040gi.pdf",
                "Instructions for Form 1040",
            ),
            (
                "https://www.govinfo.gov/content/pkg/PLAW-117publ263/pdf/PLAW-117publ263.pdf",
                "James M. Inhofe National Defense Authorization Act for Fiscal Year 2023",
            ),
        ];

        for (url, expected) in cases {
            let hit = SearchHit {
                title: format!("Live PDF {expected}"),
                url: url.to_string(),
                snippet: String::new(),
                source: "google".to_string(),
                rank: 1,
            };
            let started = Instant::now();
            let evidence =
                fetch_evidence_doc(&config, expected, &hit).expect("semi-complex pdf evidence");
            let elapsed = started.elapsed();
            println!("URL: {}", url);
            println!("ELAPSED_MS: {}", elapsed.as_millis());
            println!("TITLE: {}", evidence.title);
            println!("SUMMARY: {}", trim_text(&evidence.summary, 400));
            println!("EXCERPTS: {:?}", evidence.excerpts);
            println!("PAGE_TEXT: {}", trim_text(&evidence.page_text, 1200));
            assert!(
                evidence
                    .page_text
                    .to_ascii_lowercase()
                    .contains(&expected.to_ascii_lowercase()),
                "expected extracted page text to contain {:?}, got {:?}",
                expected,
                trim_text(&evidence.page_text, 500)
            );
            assert!(
                !evidence.excerpts.is_empty(),
                "expected non-empty excerpts for {}",
                url
            );
        }
    }

    #[test]
    #[ignore = "live network validation for page-hinted PDF loading"]
    fn real_world_pdf_page_hint_loads_requested_page() {
        let mut config = test_config(ProviderKind::Brave);
        config.timeout_ms = 20_000;
        config.max_page_bytes = 6_000_000;
        config.max_page_chars = 20_000;
        config.max_pdf_pages = 8;

        let hit = SearchHit {
            title: "IRS 1040 instructions".to_string(),
            url: "https://www.irs.gov/pub/irs-pdf/i1040gi.pdf".to_string(),
            snippet: String::new(),
            source: "google".to_string(),
            rank: 1,
        };
        let evidence = fetch_evidence_doc(&config, "page 8 filing requirements", &hit)
            .expect("real-world page-hinted pdf evidence");

        println!(
            "LOADED_PAGES: {:?}",
            evidence
                .page_sections
                .iter()
                .filter_map(|section| section.page_number)
                .collect::<Vec<_>>()
        );
        println!("SUMMARY: {}", evidence.summary);
        println!("EXCERPTS: {:?}", evidence.excerpts);

        assert!(
            evidence
                .page_sections
                .iter()
                .any(|section| section.page_number == Some(8)),
            "expected page-hinted PDF extraction to include page 8"
        );
    }

    fn test_config(provider: ProviderKind) -> SearchConfig {
        SearchConfig {
            enabled: true,
            provider,
            searxng_base_url: None,
            timeout_ms: 1000,
            default_top_k: 5,
            max_top_k: 8,
            user_agent: "ctox-test".to_string(),
            default_language: None,
            default_region: None,
            default_safe_search: true,
            cache_ttl_secs: 60,
            page_cache_ttl_secs: 60,
            max_page_bytes: 128_000,
            max_page_chars: 8_000,
            max_pdf_pages: 12,
        }
    }

    fn unique_test_root(prefix: &str) -> PathBuf {
        let root = std::env::temp_dir().join(format!("{}_{}", prefix, unix_ts()));
        let _ = fs::remove_dir_all(&root);
        root
    }

    fn set_runtime_config(root: &Path, key: &str, value: &str) {
        let runtime_config = crate::runtime_config::runtime_config_path(root);
        fs::create_dir_all(runtime_config.parent().unwrap()).unwrap();
        let conn = rusqlite::Connection::open(runtime_config).unwrap();
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS runtime_env_kv (
                env_key TEXT PRIMARY KEY,
                env_value TEXT NOT NULL
            );",
        )
        .unwrap();
        conn.execute(
            "INSERT INTO runtime_env_kv(env_key, env_value)
             VALUES(?1, ?2)
             ON CONFLICT(env_key) DO UPDATE SET env_value = excluded.env_value",
            (key, value),
        )
        .unwrap();
    }

    #[cfg(unix)]
    fn capture_stdout<T>(operation: impl FnOnce() -> T) -> (String, T) {
        static CAPTURE_MUTEX: OnceLock<Mutex<()>> = OnceLock::new();
        let _guard = CAPTURE_MUTEX
            .get_or_init(|| Mutex::new(()))
            .lock()
            .expect("stdout capture mutex poisoned");

        let mut pipe_fds = [0; 2];
        assert_eq!(
            unsafe { libc::pipe(pipe_fds.as_mut_ptr()) },
            0,
            "failed to create stdout capture pipe"
        );
        let read_fd = pipe_fds[0];
        let write_fd = pipe_fds[1];
        let stdout_dup = duplicate_fd(libc::STDOUT_FILENO, "stdout capture")
            .expect("failed to duplicate stdout for capture");
        assert!(
            unsafe { libc::dup2(write_fd, libc::STDOUT_FILENO) } >= 0,
            "failed to redirect stdout into capture pipe"
        );
        unsafe {
            libc::close(write_fd);
        }

        let result = operation();

        std::io::stdout().flush().expect("flush stdout");
        unsafe {
            libc::fflush(std::ptr::null_mut());
        }
        assert!(
            unsafe { libc::dup2(stdout_dup, libc::STDOUT_FILENO) } >= 0,
            "failed to restore stdout after capture"
        );
        unsafe {
            libc::close(stdout_dup);
        }

        let mut captured = String::new();
        let mut reader = unsafe { std::fs::File::from_raw_fd(read_fd) };
        std::io::Read::read_to_string(&mut reader, &mut captured).expect("read captured stdout");
        (captured, result)
    }
}
