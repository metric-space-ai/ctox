//! Scholarly / shadow-archive search.
//!
//! This module is the typed surface for "find a book / paper / standard /
//! magazine record by topic, title, author, ISBN, or DOI". It returns
//! metadata-only records (title, authors, year, language, file format/size,
//! ISBN, DOI, MD5, detail URL, thumbnail). For records that carry a DOI, the
//! module can additionally resolve a **legal open-access PDF URL** via
//! Unpaywall — that path is what `ctox web deep-research` uses to pull paper
//! full text during synthesis. The module deliberately does not fetch full
//! text from unauthorized mirrors.
//!
//! The first metadata backend is Anna's Archive. The module is shaped so
//! additional scholarly backends (OpenAlex, Crossref, arXiv) can be added
//! later behind the same `ScholarlySearchProvider` enum without changing the
//! return shape.
//!
//! Backend protocol port note: Anna's Archive's `/search` URL parameter set
//! (`lang`, `content`, `ext`, `sort`, `q`, `page`) is functional protocol
//! dictated by Anna's Archive itself, not by any third-party adapter. The
//! HTML parsing here is implemented from the live HTML and is independent of
//! any AGPL-licensed reference adapter.

use anyhow::{bail, Context, Result};
use scraper::{ElementRef, Html, Selector};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::BTreeSet;
use std::path::Path;
use std::time::Duration;
use url::Url;

use crate::runtime_config;

/// Default Anna's Archive base URL. Operators can point at a mirror via the
/// `CTOX_ANNAS_ARCHIVE_BASE_URL` runtime-config key.
pub const ANNAS_ARCHIVE_DEFAULT_BASE_URL: &str = "https://annas-archive.org";

/// Default Unpaywall API base URL (used when `CTOX_UNPAYWALL_BASE_URL` is
/// not set).
pub const UNPAYWALL_DEFAULT_BASE_URL: &str = "https://api.unpaywall.org/v2";

/// Anna's Archive `content` filter values.
pub const ANNAS_ARCHIVE_CONTENT_TYPES: &[&str] = &[
    "book_fiction",
    "book_nonfiction",
    "book_unknown",
    "book_comic",
    "magazine",
    "standards_document",
];

/// Anna's Archive `sort` filter values. Empty / absent means relevance.
pub const ANNAS_ARCHIVE_SORT_VALUES: &[&str] = &[
    "newest",
    "oldest",
    "largest",
    "smallest",
    "newest_added",
    "oldest_added",
    "random",
];

const SOURCE_POLICY_NOTICE: &str = "Metadata-only scholarly discovery. For DOI-bearing records, CTOX may resolve a legal open-access PDF via Unpaywall. CTOX does not download or reproduce copyrighted full text from unauthorized mirrors.";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScholarlySearchProvider {
    Auto,
    AnnasArchive,
}

impl ScholarlySearchProvider {
    pub fn from_label(raw: &str) -> Self {
        match raw.trim().to_ascii_lowercase().as_str() {
            "auto" | "" => Self::Auto,
            "annas_archive" | "annas-archive" | "anna_archive" | "anna-archive" | "annas" => {
                Self::AnnasArchive
            }
            _ => Self::Auto,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Auto => "auto",
            Self::AnnasArchive => "annas_archive",
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ScholarlySearchRequest {
    pub query: String,
    pub provider: Option<ScholarlySearchProvider>,
    pub content_types: Vec<String>,
    pub languages: Vec<String>,
    pub extensions: Vec<String>,
    pub sort: Option<String>,
    pub max_results: Option<usize>,
    pub page: Option<usize>,
    /// When true, results that carry a DOI are augmented with a legal
    /// open-access PDF URL via the Unpaywall resolver.
    pub with_oa_pdf: bool,
    /// When true, drop results without an extractable DOI (paper-only mode).
    pub only_doi: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ScholarlyResult {
    pub provider: String,
    pub source_id: String,
    pub detail_url: String,
    pub title: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub authors: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub publisher: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub year: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub language: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub file_format: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub file_size_label: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub isbn: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub doi: Option<String>,
    /// Legal open-access PDF URL resolved via Unpaywall when
    /// `ScholarlySearchRequest::with_oa_pdf` is set and the record has a DOI.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub open_access_pdf: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub open_access_license: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub thumbnail_url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub snippet: Option<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub tags: Vec<String>,
    pub rank: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScholarlySearchResponse {
    pub provider: String,
    pub query: String,
    pub results: Vec<ScholarlyResult>,
    pub executed_url: String,
    pub source_policy: String,
}

pub fn run_ctox_scholarly_search_tool(
    root: &Path,
    request: &ScholarlySearchRequest,
) -> Result<Value> {
    let response = execute_scholarly_search(root, request)?;
    Ok(json!({
        "ok": true,
        "tool": "ctox_scholarly_search",
        "query": response.query,
        "provider": response.provider,
        "executed_url": response.executed_url,
        "source_policy": response.source_policy,
        "results": response.results,
    }))
}

pub fn execute_scholarly_search(
    root: &Path,
    request: &ScholarlySearchRequest,
) -> Result<ScholarlySearchResponse> {
    let query = request.query.trim();
    if query.is_empty() {
        bail!("ctox scholarly search requires a non-empty query");
    }
    if !is_enabled(root) {
        bail!("CTOX scholarly search is disabled (CTOX_SCHOLARLY_SEARCH_ENABLED=false)");
    }
    let provider = resolve_provider(root, request.provider);
    match provider {
        ScholarlySearchProvider::Auto => unreachable!("auto provider must be resolved"),
        ScholarlySearchProvider::AnnasArchive => annas_archive_search(root, request),
    }
}

fn resolve_provider(
    root: &Path,
    requested: Option<ScholarlySearchProvider>,
) -> ScholarlySearchProvider {
    let provider = requested.unwrap_or_else(|| {
        runtime_config::get(root, "CTOX_SCHOLARLY_SEARCH_PROVIDER")
            .map(|raw| ScholarlySearchProvider::from_label(&raw))
            .unwrap_or(ScholarlySearchProvider::Auto)
    });
    match provider {
        ScholarlySearchProvider::Auto => ScholarlySearchProvider::AnnasArchive,
        other => other,
    }
}

fn is_enabled(root: &Path) -> bool {
    runtime_config::get(root, "CTOX_SCHOLARLY_SEARCH_ENABLED")
        .map(|value| {
            matches!(
                value.trim().to_ascii_lowercase().as_str(),
                "1" | "true" | "yes" | "on"
            )
        })
        .unwrap_or(true)
}

fn annas_archive_search(
    root: &Path,
    request: &ScholarlySearchRequest,
) -> Result<ScholarlySearchResponse> {
    let base_url = runtime_config::get(root, "CTOX_ANNAS_ARCHIVE_BASE_URL")
        .unwrap_or_else(|| ANNAS_ARCHIVE_DEFAULT_BASE_URL.to_string());
    let timeout_ms = runtime_config::get(root, "CTOX_SCHOLARLY_TIMEOUT_MS")
        .and_then(|value| value.parse::<u64>().ok())
        .unwrap_or(8000);
    let user_agent = runtime_config::get(root, "CTOX_SCHOLARLY_USER_AGENT")
        .unwrap_or_else(|| {
            "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/136.0.0.0 Safari/537.36".to_string()
        });
    let default_top_k = runtime_config::get(root, "CTOX_SCHOLARLY_TOP_K")
        .and_then(|value| value.parse::<usize>().ok())
        .unwrap_or(10);
    let top_k = request.max_results.unwrap_or(default_top_k).clamp(1, 50);
    let page = request.page.unwrap_or(1).max(1);

    let url = build_annas_archive_search_url(root, request, &base_url, page)?;

    let agent = ureq::AgentBuilder::new()
        .user_agent(&user_agent)
        .timeout(Duration::from_millis(timeout_ms))
        .build();

    let response = agent
        .get(url.as_str())
        .set(
            "accept",
            "text/html,application/xhtml+xml,application/xml;q=0.9,*/*;q=0.8",
        )
        .set("accept-language", "en-US,en;q=0.9")
        .call()
        .with_context(|| format!("failed to query Anna's Archive at {}", base_url))?;

    let body = response
        .into_string()
        .context("failed to read Anna's Archive response body")?;

    let mut results = parse_annas_archive_results(&body, base_url.trim_end_matches('/'), top_k);

    if request.only_doi {
        results.retain(|hit| hit.doi.is_some());
        for (idx, hit) in results.iter_mut().enumerate() {
            hit.rank = idx + 1;
        }
    }

    if request.with_oa_pdf {
        augment_results_with_open_access_pdfs(root, &mut results);
    }

    Ok(ScholarlySearchResponse {
        provider: ScholarlySearchProvider::AnnasArchive.as_str().to_string(),
        query: request.query.trim().to_string(),
        results,
        executed_url: url.to_string(),
        source_policy: SOURCE_POLICY_NOTICE.to_string(),
    })
}

fn build_annas_archive_search_url(
    root: &Path,
    request: &ScholarlySearchRequest,
    base_url: &str,
    page: usize,
) -> Result<Url> {
    let mut url = Url::parse(&format!("{}/search", base_url.trim_end_matches('/')))
        .with_context(|| format!("invalid Anna's Archive base URL: {}", base_url))?;
    {
        let mut qp = url.query_pairs_mut();
        let configured_default_lang =
            runtime_config::get(root, "CTOX_ANNAS_ARCHIVE_DEFAULT_LANGUAGE");
        if request.languages.is_empty() {
            if let Some(lang) = configured_default_lang.as_deref() {
                let trimmed = lang.trim();
                if !trimmed.is_empty() {
                    qp.append_pair("lang", trimmed);
                }
            }
        } else {
            for lang in &request.languages {
                let trimmed = lang.trim();
                if !trimmed.is_empty() {
                    qp.append_pair("lang", trimmed);
                }
            }
        }
        for content in &request.content_types {
            let trimmed = content.trim();
            if !trimmed.is_empty() {
                qp.append_pair("content", trimmed);
            }
        }
        for ext in &request.extensions {
            let trimmed = ext.trim();
            if !trimmed.is_empty() {
                qp.append_pair("ext", trimmed);
            }
        }
        if let Some(sort) = request.sort.as_deref() {
            let trimmed = sort.trim();
            if !trimmed.is_empty() {
                qp.append_pair("sort", trimmed);
            }
        }
        qp.append_pair("q", request.query.trim());
        if page > 1 {
            qp.append_pair("page", &page.to_string());
        }
    }
    Ok(url)
}

fn parse_annas_archive_results(body: &str, base_url: &str, top_k: usize) -> Vec<ScholarlyResult> {
    let document = Html::parse_document(body);
    let md5_anchor = match Selector::parse(r#"a[href*="/md5/"]"#) {
        Ok(selector) => selector,
        Err(_) => return Vec::new(),
    };

    let mut results: Vec<ScholarlyResult> = Vec::new();
    let mut seen: BTreeSet<String> = BTreeSet::new();
    for anchor in document.select(&md5_anchor) {
        if results.len() >= top_k {
            break;
        }
        let href = anchor.value().attr("href").unwrap_or("").trim();
        let Some(md5_hash) = extract_md5_hash(href) else {
            continue;
        };
        if !seen.insert(md5_hash.clone()) {
            continue;
        }

        let detail_url = if href.starts_with("http") {
            href.to_string()
        } else if href.starts_with('/') {
            format!("{}{}", base_url, href)
        } else {
            format!("{}/{}", base_url, href.trim_start_matches('/'))
        };

        let row = ascend_to_record_row(anchor);
        let row_text = collect_row_text(row);

        let title = pick_title(anchor, row);
        let thumbnail = first_image_src(row, base_url);
        let metadata_line = pick_metadata_line(row);
        let snippet = pick_snippet(row);
        let (language, file_format, file_size_label) = parse_metadata_line(&metadata_line);
        let year = extract_year(&metadata_line).or_else(|| extract_year(&row_text));
        let isbn = extract_isbn(&row_text);
        let doi = extract_doi(&row_text);
        let tags = parse_tags(&metadata_line);
        let authors = pick_text_after_title(row, &title);

        results.push(ScholarlyResult {
            provider: ScholarlySearchProvider::AnnasArchive.as_str().to_string(),
            source_id: md5_hash,
            detail_url,
            title,
            authors,
            publisher: None,
            year,
            language,
            file_format,
            file_size_label,
            isbn,
            doi,
            open_access_pdf: None,
            open_access_license: None,
            thumbnail_url: thumbnail,
            snippet,
            tags,
            rank: results.len() + 1,
        });
    }
    results
}

fn extract_md5_hash(href: &str) -> Option<String> {
    let path = href.split('?').next()?;
    let pos = path.find("/md5/")?;
    let candidate = &path[pos + "/md5/".len()..];
    let hash: String = candidate
        .chars()
        .take_while(|c| c.is_ascii_hexdigit())
        .collect();
    if hash.len() == 32 {
        Some(hash.to_ascii_lowercase())
    } else {
        None
    }
}

fn ascend_to_record_row<'a>(anchor: ElementRef<'a>) -> ElementRef<'a> {
    let mut current = anchor;
    for _ in 0..6 {
        let Some(parent_node) = current.parent() else {
            break;
        };
        let Some(parent) = ElementRef::wrap(parent_node) else {
            break;
        };
        let class = parent.value().attr("class").unwrap_or("");
        if class.contains("js-aarecord-list-outer") {
            return current;
        }
        current = parent;
    }
    current
}

fn collect_row_text(row: ElementRef<'_>) -> String {
    let raw = row.text().collect::<Vec<_>>().join(" ");
    normalize_ws(&raw)
}

fn pick_title<'a>(anchor: ElementRef<'a>, row: ElementRef<'a>) -> String {
    if let Ok(title_selector) = Selector::parse(".js-vim-focus") {
        if let Some(node) = row.select(&title_selector).next() {
            let text = normalize_ws(&node.text().collect::<Vec<_>>().join(" "));
            if !text.is_empty() {
                return text;
            }
        }
    }
    let text = normalize_ws(&anchor.text().collect::<Vec<_>>().join(" "));
    if !text.is_empty() {
        return text;
    }
    let row_text = collect_row_text(row);
    row_text.chars().take(120).collect()
}

fn first_image_src(row: ElementRef<'_>, base_url: &str) -> Option<String> {
    let img_selector = Selector::parse("img").ok()?;
    let img = row.select(&img_selector).next()?;
    let src = img
        .value()
        .attr("src")
        .or_else(|| img.value().attr("data-src"))?
        .trim();
    if src.is_empty() {
        return None;
    }
    if src.starts_with("http") {
        Some(src.to_string())
    } else if src.starts_with("//") {
        Some(format!("https:{src}"))
    } else if src.starts_with('/') {
        Some(format!("{}{}", base_url, src))
    } else {
        Some(src.to_string())
    }
}

fn pick_metadata_line(row: ElementRef<'_>) -> String {
    if let Ok(selector) = Selector::parse(".font-semibold") {
        let mut best: Option<String> = None;
        for node in row.select(&selector) {
            let text = normalize_ws(&node.text().collect::<Vec<_>>().join(" "));
            if text.is_empty() {
                continue;
            }
            if text.contains(',') || text.contains('·') {
                return text;
            }
            if best.is_none() {
                best = Some(text);
            }
        }
        if let Some(found) = best {
            return found;
        }
    }
    String::new()
}

fn pick_snippet(row: ElementRef<'_>) -> Option<String> {
    let selector = Selector::parse(".line-clamp").ok()?;
    for node in row.select(&selector) {
        let text = normalize_ws(&node.text().collect::<Vec<_>>().join(" "));
        if text.len() > 20 {
            return Some(trim_text(&text, 400));
        }
    }
    None
}

fn pick_text_after_title<'a>(row: ElementRef<'a>, title: &str) -> Option<String> {
    let row_text = collect_row_text(row);
    let trimmed_title = title.trim();
    if trimmed_title.is_empty() {
        return None;
    }
    let idx = row_text.find(trimmed_title)?;
    let after = row_text[idx + trimmed_title.len()..].trim();
    if after.is_empty() {
        return None;
    }
    let candidate: String = after.chars().take(160).collect();
    let cleaned = candidate
        .trim_start_matches([',', '·', ' ', ';'])
        .trim()
        .to_string();
    if cleaned.is_empty() {
        None
    } else {
        Some(cleaned)
    }
}

fn parse_metadata_line(line: &str) -> (Option<String>, Option<String>, Option<String>) {
    let line_trimmed = line.trim();
    if line_trimmed.is_empty() {
        return (None, None, None);
    }
    let tokens: Vec<&str> = line_trimmed
        .split(|c: char| c == ',' || c == '·')
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .collect();
    let mut language: Option<String> = None;
    let mut file_format: Option<String> = None;
    let mut file_size: Option<String> = None;
    for token in tokens {
        if language.is_none() {
            if let Some(code) = extract_language_code(token) {
                language = Some(code);
                continue;
            }
        }
        let lower = token.to_ascii_lowercase();
        if file_format.is_none() && is_file_extension(&lower) {
            file_format = Some(lower);
            continue;
        }
        if file_size.is_none() && looks_like_file_size(token) {
            file_size = Some(token.to_string());
            continue;
        }
    }
    (language, file_format, file_size)
}

fn extract_language_code(token: &str) -> Option<String> {
    if let Some(open) = token.find('[') {
        if let Some(close_offset) = token[open..].find(']') {
            let code = &token[open + 1..open + close_offset];
            let trimmed = code.trim();
            if (2..=5).contains(&trimmed.len())
                && trimmed
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || c == '-')
            {
                return Some(trimmed.to_ascii_lowercase());
            }
        }
    }
    None
}

fn is_file_extension(token: &str) -> bool {
    matches!(
        token,
        "pdf" | "epub"
            | "mobi"
            | "azw3"
            | "djvu"
            | "fb2"
            | "txt"
            | "rtf"
            | "doc"
            | "docx"
            | "lit"
            | "cbz"
            | "cbr"
            | "html"
            | "htm"
    )
}

fn looks_like_file_size(token: &str) -> bool {
    let trimmed: String = token.chars().filter(|c| !c.is_whitespace()).collect();
    if trimmed.len() < 3 {
        return false;
    }
    let lower = trimmed.to_ascii_lowercase();
    let has_unit = lower.ends_with("kb")
        || lower.ends_with("mb")
        || lower.ends_with("gb")
        || lower.ends_with("tb");
    let starts_with_digit = lower
        .chars()
        .next()
        .map(|c| c.is_ascii_digit())
        .unwrap_or(false);
    has_unit && starts_with_digit
}

fn parse_tags(line: &str) -> Vec<String> {
    line.split(|c: char| c == ',' || c == '·')
        .map(|s| normalize_ws(s.trim()))
        .filter(|s| !s.is_empty())
        .take(8)
        .collect()
}

fn extract_year(text: &str) -> Option<i32> {
    let chars: Vec<(usize, char)> = text.char_indices().collect();
    if chars.len() < 4 {
        return None;
    }
    for i in 0..=chars.len() - 4 {
        let window = &chars[i..i + 4];
        if window.iter().all(|(_, c)| c.is_ascii_digit()) {
            let prev_is_digit = i > 0 && chars[i - 1].1.is_ascii_digit();
            let next_is_digit = i + 4 < chars.len() && chars[i + 4].1.is_ascii_digit();
            if !prev_is_digit && !next_is_digit {
                let s: String = window.iter().map(|(_, c)| *c).collect();
                if let Ok(year) = s.parse::<i32>() {
                    if (1700..=2099).contains(&year) {
                        return Some(year);
                    }
                }
            }
        }
    }
    None
}

fn extract_isbn(text: &str) -> Option<String> {
    let bytes = text.as_bytes();
    let mut search_from = 0;
    while let Some(found) = text[search_from..].find("ISBN") {
        let start = search_from + found + "ISBN".len();
        let mut idx = start;
        while idx < bytes.len() && matches!(bytes[idx], b' ' | b':' | b'-' | b'\t') {
            idx += 1;
        }
        let mut digits = String::new();
        while idx < bytes.len() {
            let b = bytes[idx];
            if b.is_ascii_digit() {
                digits.push(b as char);
                idx += 1;
            } else if b == b'-' || b == b' ' {
                idx += 1;
            } else {
                break;
            }
            if digits.len() >= 13 {
                break;
            }
        }
        if digits.len() == 10 || digits.len() == 13 {
            return Some(digits);
        }
        search_from = start;
    }
    None
}

/// Extract a DOI from free text. Recognises bare `10.NNNN/<suffix>`,
/// `doi:`-prefixed forms, and `doi.org/...` URL forms. Trailing sentence
/// punctuation is trimmed.
fn extract_doi(text: &str) -> Option<String> {
    let bytes = text.as_bytes();
    let mut idx = 0;
    while idx + 4 < bytes.len() {
        if bytes[idx] == b'1' && bytes[idx + 1] == b'0' && bytes[idx + 2] == b'.' {
            let prev_ok = idx == 0 || !is_doi_inner_byte(bytes[idx - 1]);
            if prev_ok {
                let digits_start = idx + 3;
                let mut p = digits_start;
                while p < bytes.len() && bytes[p].is_ascii_digit() {
                    p += 1;
                }
                let digit_count = p - digits_start;
                if (4..=9).contains(&digit_count) && p < bytes.len() && bytes[p] == b'/' {
                    let suffix_start = p + 1;
                    let mut q = suffix_start;
                    while q < bytes.len() && is_doi_suffix_byte(bytes[q]) {
                        q += 1;
                    }
                    let mut end = q;
                    while end > suffix_start
                        && matches!(bytes[end - 1], b'.' | b',' | b';' | b':' | b')' | b']')
                    {
                        end -= 1;
                    }
                    if end > suffix_start {
                        return Some(text[idx..end].to_string());
                    }
                }
            }
        }
        idx += 1;
    }
    None
}

fn is_doi_inner_byte(b: u8) -> bool {
    b.is_ascii_alphanumeric() || matches!(b, b'.' | b'/' | b'-' | b'_')
}

fn is_doi_suffix_byte(b: u8) -> bool {
    !b.is_ascii_whitespace() && !matches!(b, b'<' | b'>' | b'"' | b'\'' | b'\\')
}

fn augment_results_with_open_access_pdfs(root: &Path, results: &mut [ScholarlyResult]) {
    let timeout_ms = runtime_config::get(root, "CTOX_SCHOLARLY_TIMEOUT_MS")
        .and_then(|value| value.parse::<u64>().ok())
        .unwrap_or(8000);
    let user_agent = runtime_config::get(root, "CTOX_SCHOLARLY_USER_AGENT")
        .unwrap_or_else(|| {
            "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/136.0.0.0 Safari/537.36".to_string()
        });
    let agent = ureq::AgentBuilder::new()
        .user_agent(&user_agent)
        .timeout(Duration::from_millis(timeout_ms))
        .build();
    let unpaywall_base = runtime_config::get(root, "CTOX_UNPAYWALL_BASE_URL")
        .unwrap_or_else(|| UNPAYWALL_DEFAULT_BASE_URL.to_string());
    let contact_email = runtime_config::get(root, "CTOX_UNPAYWALL_EMAIL")
        .unwrap_or_else(|| "ctox@example.org".to_string());
    for hit in results.iter_mut() {
        let Some(doi) = hit.doi.as_deref() else {
            continue;
        };
        if let Ok(Some(resolved)) =
            resolve_unpaywall_oa_pdf(&agent, &unpaywall_base, &contact_email, doi)
        {
            hit.open_access_pdf = Some(resolved.url);
            hit.open_access_license = resolved.license;
        }
    }
}

#[derive(Debug, Clone)]
struct ResolvedOpenAccessPdf {
    url: String,
    license: Option<String>,
}

fn resolve_unpaywall_oa_pdf(
    agent: &ureq::Agent,
    base_url: &str,
    contact_email: &str,
    doi: &str,
) -> Result<Option<ResolvedOpenAccessPdf>> {
    let normalized_doi = doi
        .trim()
        .trim_start_matches("https://doi.org/")
        .trim_start_matches("http://doi.org/")
        .trim_start_matches("doi:")
        .trim_start_matches("DOI:")
        .trim();
    if normalized_doi.is_empty() {
        return Ok(None);
    }
    let encoded_doi: String =
        url::form_urlencoded::byte_serialize(normalized_doi.as_bytes()).collect();
    let encoded_email: String =
        url::form_urlencoded::byte_serialize(contact_email.as_bytes()).collect();
    let endpoint = format!(
        "{}/{}?email={}",
        base_url.trim_end_matches('/'),
        encoded_doi,
        encoded_email
    );
    let response = match agent.get(&endpoint).set("accept", "application/json").call() {
        Ok(response) => response,
        Err(_) => return Ok(None),
    };
    let payload: Value = match serde_json::from_reader(response.into_reader()) {
        Ok(payload) => payload,
        Err(_) => return Ok(None),
    };
    let is_oa = payload.get("is_oa").and_then(Value::as_bool).unwrap_or(false);
    if !is_oa {
        return Ok(None);
    }
    let best = payload.get("best_oa_location").or_else(|| {
        payload
            .get("oa_locations")
            .and_then(Value::as_array)
            .and_then(|arr| arr.first())
    });
    let Some(best) = best else {
        return Ok(None);
    };
    let pdf_url = best
        .get("url_for_pdf")
        .and_then(Value::as_str)
        .or_else(|| best.get("url").and_then(Value::as_str))
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned);
    let Some(pdf_url) = pdf_url else {
        return Ok(None);
    };
    let license = best
        .get("license")
        .and_then(Value::as_str)
        .filter(|value| !value.trim().is_empty())
        .map(ToOwned::to_owned);
    Ok(Some(ResolvedOpenAccessPdf {
        url: pdf_url,
        license,
    }))
}

fn normalize_ws(input: &str) -> String {
    input.split_whitespace().collect::<Vec<_>>().join(" ")
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn provider_label_round_trip() {
        assert_eq!(
            ScholarlySearchProvider::from_label("annas_archive"),
            ScholarlySearchProvider::AnnasArchive
        );
        assert_eq!(
            ScholarlySearchProvider::from_label("annas-archive"),
            ScholarlySearchProvider::AnnasArchive
        );
        assert_eq!(
            ScholarlySearchProvider::from_label(""),
            ScholarlySearchProvider::Auto
        );
        assert_eq!(
            ScholarlySearchProvider::from_label("auto"),
            ScholarlySearchProvider::Auto
        );
        assert_eq!(
            ScholarlySearchProvider::AnnasArchive.as_str(),
            "annas_archive"
        );
    }

    #[test]
    fn extracts_md5_hash_from_relative_path() {
        let hash = extract_md5_hash("/md5/0123456789abcdef0123456789abcdef").unwrap();
        assert_eq!(hash, "0123456789abcdef0123456789abcdef");
    }

    #[test]
    fn extracts_md5_hash_from_absolute_url() {
        let hash =
            extract_md5_hash("https://annas-archive.org/md5/AABBCCDDEEFF00112233445566778899")
                .unwrap();
        assert_eq!(hash, "aabbccddeeff00112233445566778899");
    }

    #[test]
    fn rejects_md5_with_wrong_length() {
        assert!(extract_md5_hash("/md5/abcdef").is_none());
        assert!(extract_md5_hash("/md5/").is_none());
        assert!(extract_md5_hash("/somethingelse/abc").is_none());
    }

    #[test]
    fn extracts_year_in_range() {
        assert_eq!(extract_year("Springer, 2019, pdf"), Some(2019));
        assert_eq!(extract_year("Wiley 1987"), Some(1987));
        assert_eq!(extract_year("ISBN 978019883"), None);
        assert_eq!(extract_year("Year: 1500"), None);
    }

    #[test]
    fn extracts_language_code_from_bracket_notation() {
        assert_eq!(extract_language_code("English [en]"), Some("en".to_string()));
        assert_eq!(extract_language_code("Français [fr]"), Some("fr".to_string()));
        assert_eq!(extract_language_code("Plain text"), None);
    }

    #[test]
    fn parse_metadata_line_extracts_lang_format_size() {
        let line = "English [en], pdf, 2.4MB, Book (non-fiction)";
        let (lang, fmt, size) = parse_metadata_line(line);
        assert_eq!(lang, Some("en".to_string()));
        assert_eq!(fmt, Some("pdf".to_string()));
        assert_eq!(size, Some("2.4MB".to_string()));
    }

    #[test]
    fn parse_metadata_line_handles_middle_dot_separator() {
        let line = "German [de] · epub · 543KB";
        let (lang, fmt, size) = parse_metadata_line(line);
        assert_eq!(lang, Some("de".to_string()));
        assert_eq!(fmt, Some("epub".to_string()));
        assert_eq!(size, Some("543KB".to_string()));
    }

    #[test]
    fn parse_metadata_line_returns_none_for_empty() {
        let (lang, fmt, size) = parse_metadata_line("");
        assert_eq!(lang, None);
        assert_eq!(fmt, None);
        assert_eq!(size, None);
    }

    #[test]
    fn extract_isbn_finds_thirteen_digit_value() {
        let text = "ISBN: 978-3-16-148410-0 published 2024";
        assert_eq!(extract_isbn(text), Some("9783161484100".to_string()));
    }

    #[test]
    fn extract_isbn_finds_ten_digit_value() {
        let text = "ISBN 0306406152 reprint";
        assert_eq!(extract_isbn(text), Some("0306406152".to_string()));
    }

    #[test]
    fn extract_isbn_returns_none_when_missing_prefix() {
        let text = "no identifier here, just numbers 1234567890123";
        assert_eq!(extract_isbn(text), None);
    }

    #[test]
    fn extract_doi_finds_bare_doi() {
        assert_eq!(
            extract_doi("Reference: 10.1038/nature12373 see"),
            Some("10.1038/nature12373".to_string())
        );
    }

    #[test]
    fn extract_doi_finds_doi_org_url() {
        assert_eq!(
            extract_doi("Available at https://doi.org/10.1234/abc.2024.001."),
            Some("10.1234/abc.2024.001".to_string())
        );
    }

    #[test]
    fn extract_doi_strips_trailing_punctuation() {
        assert_eq!(
            extract_doi("(see 10.5555/foo.bar);"),
            Some("10.5555/foo.bar".to_string())
        );
    }

    #[test]
    fn extract_doi_returns_none_for_text_without_doi() {
        assert!(extract_doi("ISBN 0306406152, Wiley, 2007").is_none());
    }

    #[test]
    fn looks_like_file_size_accepts_common_sizes() {
        assert!(looks_like_file_size("2.4MB"));
        assert!(looks_like_file_size("543KB"));
        assert!(looks_like_file_size("1.2 GB"));
        assert!(!looks_like_file_size("MB"));
        assert!(!looks_like_file_size("abc"));
        assert!(!looks_like_file_size("Page 12"));
    }

    #[test]
    fn build_annas_archive_search_url_orders_params_correctly() {
        let request = ScholarlySearchRequest {
            query: "quantum entanglement".to_string(),
            content_types: vec!["book_nonfiction".to_string()],
            languages: vec!["en".to_string()],
            extensions: vec!["pdf".to_string()],
            sort: Some("newest".to_string()),
            page: Some(2),
            ..Default::default()
        };
        let root = std::env::temp_dir().join("ctox-scholarly-test-noop");
        let url = build_annas_archive_search_url(&root, &request, "https://annas-archive.org", 2)
            .expect("URL build");
        let raw = url.as_str();
        assert!(raw.starts_with("https://annas-archive.org/search?"));
        assert!(raw.contains("lang=en"));
        assert!(raw.contains("content=book_nonfiction"));
        assert!(raw.contains("ext=pdf"));
        assert!(raw.contains("sort=newest"));
        assert!(raw.contains("q=quantum+entanglement") || raw.contains("q=quantum%20entanglement"));
        assert!(raw.contains("page=2"));
    }

    #[test]
    fn build_annas_archive_search_url_omits_page_one() {
        let request = ScholarlySearchRequest {
            query: "rust".to_string(),
            ..Default::default()
        };
        let root = std::env::temp_dir().join("ctox-scholarly-test-noop");
        let url = build_annas_archive_search_url(&root, &request, "https://annas-archive.org", 1)
            .expect("URL build");
        assert!(!url.as_str().contains("page="));
    }

    #[test]
    fn parse_results_extracts_md5_title_and_metadata_from_fixture() {
        let fixture = r##"
        <html>
          <body>
            <main>
              <div class="js-aarecord-list-outer">
                <a href="/md5/0123456789abcdef0123456789abcdef" class="block">
                  <div class="flex">
                    <img src="/cover/abc.jpg" alt="cover" />
                    <div>
                      <h3 class="js-vim-focus">Quantum Entanglement: Foundations</h3>
                      <div class="font-semibold">English [en], pdf, 2.4MB, 📕 Book (non-fiction), Springer, 2019</div>
                      <div class="relative"><div class="line-clamp">A comprehensive introduction to quantum entanglement, Bell inequalities, and applications to quantum information theory.</div></div>
                      <div>ISBN: 978-3-16-148410-0, doi: 10.1234/quantum.2019.001</div>
                    </div>
                  </div>
                </a>
                <a href="/md5/aabbccddeeff00112233445566778899" class="block">
                  <div class="flex">
                    <img src="/cover/xyz.jpg" alt="cover" />
                    <div>
                      <h3 class="js-vim-focus">Entangled States in Many-Body Systems</h3>
                      <div class="font-semibold">German [de], epub, 543KB, 📕 Book (non-fiction), Wiley, 2007</div>
                    </div>
                  </div>
                </a>
              </div>
            </main>
          </body>
        </html>
        "##;
        let results = parse_annas_archive_results(fixture, "https://annas-archive.org", 10);
        assert_eq!(results.len(), 2, "expected two unique md5 results");

        let first = &results[0];
        assert_eq!(first.source_id, "0123456789abcdef0123456789abcdef");
        assert_eq!(
            first.detail_url,
            "https://annas-archive.org/md5/0123456789abcdef0123456789abcdef"
        );
        assert_eq!(first.title, "Quantum Entanglement: Foundations");
        assert_eq!(first.language, Some("en".to_string()));
        assert_eq!(first.file_format, Some("pdf".to_string()));
        assert_eq!(first.file_size_label, Some("2.4MB".to_string()));
        assert_eq!(first.year, Some(2019));
        assert_eq!(first.isbn, Some("9783161484100".to_string()));
        assert_eq!(first.doi.as_deref(), Some("10.1234/quantum.2019.001"));
        assert_eq!(
            first.thumbnail_url.as_deref(),
            Some("https://annas-archive.org/cover/abc.jpg")
        );
        assert!(first
            .snippet
            .as_deref()
            .is_some_and(|s| s.contains("Bell inequalities")));
        assert_eq!(first.rank, 1);

        let second = &results[1];
        assert_eq!(second.language, Some("de".to_string()));
        assert_eq!(second.file_format, Some("epub".to_string()));
        assert_eq!(second.year, Some(2007));
        assert!(second.doi.is_none());
        assert_eq!(second.rank, 2);
    }

    #[test]
    fn parse_results_dedupes_repeated_md5_anchors() {
        let fixture = r##"
        <div class="js-aarecord-list-outer">
          <a href="/md5/00000000000000000000000000000001"><span>First</span></a>
          <div>
            <a href="/md5/00000000000000000000000000000001">Same hash, different anchor</a>
          </div>
          <a href="/md5/00000000000000000000000000000002"><span>Second</span></a>
        </div>
        "##;
        let results = parse_annas_archive_results(fixture, "https://annas-archive.org", 10);
        assert_eq!(results.len(), 2);
        assert_eq!(results[0].source_id, "00000000000000000000000000000001");
        assert_eq!(results[1].source_id, "00000000000000000000000000000002");
    }

    #[test]
    fn parse_results_respects_top_k() {
        let mut fixture = String::from(r#"<div class="js-aarecord-list-outer">"#);
        for i in 0..6 {
            fixture.push_str(&format!(
                r#"<a href="/md5/0000000000000000000000000000000{i}"><span>Title {i}</span></a>"#
            ));
        }
        fixture.push_str("</div>");
        let results = parse_annas_archive_results(&fixture, "https://annas-archive.org", 3);
        assert_eq!(results.len(), 3);
    }

    #[test]
    fn parse_results_returns_empty_when_no_md5_links() {
        let fixture = r##"<html><body><p>No results found.</p></body></html>"##;
        let results = parse_annas_archive_results(fixture, "https://annas-archive.org", 10);
        assert!(results.is_empty());
    }

    // ----- End-to-end mock-server tests -----

    fn write_runtime_kv(db_path: &Path, key: &str, value: &str) {
        use rusqlite::Connection;
        let conn = Connection::open(db_path).expect("open mock kv db");
        conn.execute(
            "CREATE TABLE IF NOT EXISTS runtime_env_kv (env_key TEXT PRIMARY KEY, env_value TEXT NOT NULL)",
            [],
        )
        .expect("create runtime_env_kv");
        conn.execute(
            "INSERT OR REPLACE INTO runtime_env_kv (env_key, env_value) VALUES (?1, ?2)",
            rusqlite::params![key, value],
        )
        .expect("insert runtime kv");
    }

    fn unique_test_root(tag: &str) -> std::path::PathBuf {
        use std::time::SystemTime;
        use std::time::UNIX_EPOCH;
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        let root = std::env::temp_dir().join(format!(
            "ctox-scholarly-{tag}-{}-{}",
            std::process::id(),
            unique
        ));
        std::fs::create_dir_all(root.join("runtime")).expect("create runtime dir");
        root
    }

    fn read_http_request_path(stream: &mut std::net::TcpStream) -> String {
        use std::io::BufRead;
        use std::io::BufReader;
        let mut reader = BufReader::new(stream);
        let mut request_line = String::new();
        let _ = reader.read_line(&mut request_line);
        // discard remaining headers
        loop {
            let mut line = String::new();
            match reader.read_line(&mut line) {
                Ok(0) | Err(_) => break,
                Ok(_) => {}
            }
            if line == "\r\n" || line == "\n" {
                break;
            }
        }
        request_line
            .split_whitespace()
            .nth(1)
            .unwrap_or("")
            .to_string()
    }

    fn write_http_response(stream: &mut std::net::TcpStream, body: &str, content_type: &str) {
        use std::io::Write;
        let response = format!(
            "HTTP/1.1 200 OK\r\nContent-Type: {}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
            content_type,
            body.len(),
            body
        );
        let _ = stream.write_all(response.as_bytes());
    }

    #[test]
    fn end_to_end_against_local_mock_http_server() {
        use std::net::TcpListener;

        const FIXTURE_HTML: &str = r##"<!doctype html><html><body><main>
            <div class="js-aarecord-list-outer">
              <a href="/md5/0123456789abcdef0123456789abcdef" class="block">
                <div class="flex">
                  <img src="/cover/abc.jpg" alt="cover" />
                  <div>
                    <h3 class="js-vim-focus">Quantum Entanglement: Foundations</h3>
                    <div class="font-semibold">English [en], pdf, 2.4MB, 📕 Book (non-fiction), Springer, 2019</div>
                    <div class="relative"><div class="line-clamp">A comprehensive introduction to quantum entanglement, Bell inequalities, and applications to quantum information theory.</div></div>
                    <div>ISBN: 978-3-16-148410-0</div>
                  </div>
                </div>
              </a>
              <a href="/md5/aabbccddeeff00112233445566778899" class="block">
                <div class="flex">
                  <img src="/cover/xyz.jpg" alt="cover" />
                  <div>
                    <h3 class="js-vim-focus">Entangled States in Many-Body Systems</h3>
                    <div class="font-semibold">German [de], epub, 543KB, 📕 Book (non-fiction), Wiley, 2007</div>
                  </div>
                </div>
              </a>
            </div>
            </main></body></html>"##;

        let listener = TcpListener::bind("127.0.0.1:0").expect("bind mock listener");
        let port = listener.local_addr().expect("mock local addr").port();

        let body = FIXTURE_HTML.to_string();
        let server = std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().expect("accept mock connection");
            let _ = read_http_request_path(&mut stream);
            write_http_response(&mut stream, &body, "text/html; charset=utf-8");
        });

        let root = unique_test_root("mock-aa-only");
        write_runtime_kv(
            &root.join("runtime/ctox.sqlite3"),
            "CTOX_ANNAS_ARCHIVE_BASE_URL",
            &format!("http://127.0.0.1:{port}"),
        );

        let request = ScholarlySearchRequest {
            query: "quantum entanglement".to_string(),
            content_types: vec!["book_nonfiction".to_string()],
            languages: vec!["en".to_string()],
            extensions: vec!["pdf".to_string()],
            sort: Some("newest".to_string()),
            max_results: Some(10),
            ..Default::default()
        };
        let response =
            execute_scholarly_search(&root, &request).expect("execute_scholarly_search");
        let _ = server.join();

        assert_eq!(response.provider, "annas_archive");
        assert!(response
            .executed_url
            .starts_with(&format!("http://127.0.0.1:{port}/search?")));
        assert_eq!(response.results.len(), 2);
        assert_eq!(
            response.results[0].source_id,
            "0123456789abcdef0123456789abcdef"
        );
        assert_eq!(response.results[0].title, "Quantum Entanglement: Foundations");
        assert!(response.results[0].open_access_pdf.is_none());

        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn end_to_end_resolves_open_access_pdf_via_unpaywall() {
        use std::net::TcpListener;

        const AA_HTML_WITH_DOI: &str = r##"<!doctype html><html><body><main>
            <div class="js-aarecord-list-outer">
              <a href="/md5/1111111111111111111111111111aaaa" class="block">
                <div class="flex">
                  <div>
                    <h3 class="js-vim-focus">Information Theory of Communication</h3>
                    <div class="font-semibold">English [en], pdf, 1.1MB, Bell Labs, 1948</div>
                    <div>doi: 10.1002/j.1538-7305.1948.tb01338.x</div>
                  </div>
                </div>
              </a>
            </div>
            </main></body></html>"##;

        const UNPAYWALL_JSON: &str = r##"{
            "doi": "10.1002/j.1538-7305.1948.tb01338.x",
            "is_oa": true,
            "best_oa_location": {
                "url": "https://example.org/landing",
                "url_for_pdf": "https://example.org/papers/shannon-1948.pdf",
                "license": "cc-by",
                "version": "publishedVersion",
                "host_type": "publisher"
            },
            "oa_locations": []
        }"##;

        let aa_listener = TcpListener::bind("127.0.0.1:0").expect("bind aa mock");
        let aa_port = aa_listener.local_addr().expect("aa addr").port();
        let unpaywall_listener = TcpListener::bind("127.0.0.1:0").expect("bind unpaywall mock");
        let unpaywall_port = unpaywall_listener.local_addr().expect("up addr").port();

        let aa_body = AA_HTML_WITH_DOI.to_string();
        let aa_server = std::thread::spawn(move || {
            let (mut stream, _) = aa_listener.accept().expect("accept aa");
            let path = read_http_request_path(&mut stream);
            assert!(
                path.starts_with("/search"),
                "aa mock should receive /search, got {path}"
            );
            write_http_response(&mut stream, &aa_body, "text/html; charset=utf-8");
        });

        let unpaywall_body = UNPAYWALL_JSON.to_string();
        let captured_path = std::sync::Arc::new(std::sync::Mutex::new(String::new()));
        let captured_path_clone = std::sync::Arc::clone(&captured_path);
        let unpaywall_server = std::thread::spawn(move || {
            let (mut stream, _) = unpaywall_listener.accept().expect("accept unpaywall");
            let path = read_http_request_path(&mut stream);
            *captured_path_clone.lock().expect("captured path lock") = path;
            write_http_response(&mut stream, &unpaywall_body, "application/json");
        });

        let root = unique_test_root("mock-aa-with-oa");
        let db = root.join("runtime/ctox.sqlite3");
        write_runtime_kv(
            &db,
            "CTOX_ANNAS_ARCHIVE_BASE_URL",
            &format!("http://127.0.0.1:{aa_port}"),
        );
        write_runtime_kv(
            &db,
            "CTOX_UNPAYWALL_BASE_URL",
            &format!("http://127.0.0.1:{unpaywall_port}"),
        );
        write_runtime_kv(&db, "CTOX_UNPAYWALL_EMAIL", "ci@ctox.test");

        let request = ScholarlySearchRequest {
            query: "shannon information theory".to_string(),
            with_oa_pdf: true,
            ..Default::default()
        };
        let response =
            execute_scholarly_search(&root, &request).expect("execute_scholarly_search with OA");
        let _ = aa_server.join();
        let _ = unpaywall_server.join();

        assert_eq!(response.results.len(), 1);
        let hit = &response.results[0];
        assert_eq!(hit.doi.as_deref(), Some("10.1002/j.1538-7305.1948.tb01338.x"));
        assert_eq!(
            hit.open_access_pdf.as_deref(),
            Some("https://example.org/papers/shannon-1948.pdf"),
            "open-access PDF must be resolved via Unpaywall mock"
        );
        assert_eq!(hit.open_access_license.as_deref(), Some("cc-by"));

        let captured = captured_path.lock().expect("captured path read").clone();
        assert!(captured.contains("10.1002"), "Unpaywall request path should encode DOI: {captured}");
        assert!(
            captured.contains("email=ci%40ctox.test") || captured.contains("email=ci@ctox.test"),
            "Unpaywall request must include email: {captured}"
        );

        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn only_doi_filter_drops_records_without_doi() {
        use std::net::TcpListener;

        const AA_HTML_MIX: &str = r##"<!doctype html><html><body><main>
            <div class="js-aarecord-list-outer">
              <a href="/md5/aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa" class="block">
                <div class="flex"><div>
                  <h3 class="js-vim-focus">Has DOI</h3>
                  <div class="font-semibold">English [en], pdf, 1MB, 2020</div>
                  <div>doi: 10.5555/has.doi</div>
                </div></div>
              </a>
              <a href="/md5/bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb" class="block">
                <div class="flex"><div>
                  <h3 class="js-vim-focus">No DOI</h3>
                  <div class="font-semibold">English [en], pdf, 1MB, 2021</div>
                </div></div>
              </a>
            </div>
            </main></body></html>"##;

        let listener = TcpListener::bind("127.0.0.1:0").expect("bind");
        let port = listener.local_addr().expect("addr").port();
        let body = AA_HTML_MIX.to_string();
        let server = std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().expect("accept");
            let _ = read_http_request_path(&mut stream);
            write_http_response(&mut stream, &body, "text/html; charset=utf-8");
        });

        let root = unique_test_root("mock-only-doi");
        write_runtime_kv(
            &root.join("runtime/ctox.sqlite3"),
            "CTOX_ANNAS_ARCHIVE_BASE_URL",
            &format!("http://127.0.0.1:{port}"),
        );

        let request = ScholarlySearchRequest {
            query: "anything".to_string(),
            only_doi: true,
            ..Default::default()
        };
        let response = execute_scholarly_search(&root, &request).expect("execute");
        let _ = server.join();

        assert_eq!(response.results.len(), 1);
        assert_eq!(response.results[0].title, "Has DOI");
        assert_eq!(response.results[0].rank, 1);

        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    #[ignore = "live network: hits real annas-archive.org"]
    fn live_annas_archive_returns_results() {
        let root = std::env::temp_dir().join("ctox-scholarly-live");
        std::fs::create_dir_all(&root).expect("create live test root");
        let request = ScholarlySearchRequest {
            query: "shannon information theory".to_string(),
            max_results: Some(5),
            ..Default::default()
        };
        let response = match execute_scholarly_search(&root, &request) {
            Ok(response) => response,
            Err(err) => panic!("Anna's Archive live request failed: {err:#}"),
        };
        assert!(!response.results.is_empty());
        assert!(response.results.iter().all(|hit| hit.source_id.len() == 32));
        assert!(response.results.iter().all(|hit| !hit.title.is_empty()));
    }
}
