//! DOI and arXiv resolvers via Crossref, OpenAlex, and arXiv APIs.
//!
//! These resolvers are read-only: they take an identifier and return
//! a normalized [`CanonicalCitation`]. Network errors and 404s return
//! `Ok(None)` so the caller can fall back to lower-fidelity metadata.

use anyhow::Context;
use anyhow::Result;
use regex::Regex;
use serde::Deserialize;
use serde::Serialize;
use std::sync::OnceLock;
use std::time::Duration;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CanonicalCitation {
    pub citation_kind: String, // doi | arxiv | url
    pub canonical_id: String,
    pub title: Option<String>,
    pub authors: Vec<String>,
    pub venue: Option<String>,
    pub year: Option<i64>,
    pub publisher: Option<String>,
    pub landing_url: Option<String>,
    pub full_text_url: Option<String>,
    pub abstract_md: Option<String>,
    pub license: Option<String>,
    pub resolver: String,
}

const CROSSREF_BASE: &str = "https://api.crossref.org/works/";
const OPENALEX_BASE: &str = "https://api.openalex.org/works/doi:";
const ARXIV_BASE: &str = "http://export.arxiv.org/api/query?id_list=";
const HTTP_TIMEOUT: Duration = Duration::from_secs(10);
const USER_AGENT: &str = "ctox-report/1 (mailto:contact@metric-space-ai.github.io)";

fn doi_regex() -> &'static Regex {
    static R: OnceLock<Regex> = OnceLock::new();
    R.get_or_init(|| {
        Regex::new(r#"\b(10\.\d{4,9}/[-._;()/:A-Z0-9a-z]+)\b"#).expect("compile DOI regex")
    })
}

fn arxiv_regex() -> &'static Regex {
    static R: OnceLock<Regex> = OnceLock::new();
    R.get_or_init(|| {
        Regex::new(r#"\barXiv:\s*(\d{4}\.\d{4,5})(?:v\d+)?\b"#).expect("compile arXiv regex")
    })
}

pub fn extract_dois_from_text(text: &str) -> Vec<String> {
    let mut out: Vec<String> = doi_regex()
        .captures_iter(text)
        .filter_map(|c| c.get(1).map(|m| normalize_doi(m.as_str())))
        .collect();
    out.sort();
    out.dedup();
    out
}

pub fn extract_arxiv_from_text(text: &str) -> Vec<String> {
    let mut out: Vec<String> = arxiv_regex()
        .captures_iter(text)
        .filter_map(|c| c.get(1).map(|m| m.as_str().to_string()))
        .collect();
    out.sort();
    out.dedup();
    out
}

fn normalize_doi(raw: &str) -> String {
    raw.trim()
        .trim_start_matches("doi:")
        .trim_end_matches('.')
        .trim_end_matches(',')
        .to_string()
}

pub fn resolve_doi_via_crossref(doi: &str) -> Result<Option<CanonicalCitation>> {
    let doi_norm = normalize_doi(doi);
    if doi_norm.is_empty() {
        return Ok(None);
    }
    let url = format!("{CROSSREF_BASE}{doi_norm}");
    let agent = ureq::AgentBuilder::new()
        .timeout_connect(HTTP_TIMEOUT)
        .timeout_read(HTTP_TIMEOUT)
        .user_agent(USER_AGENT)
        .build();
    let resp = match agent.get(&url).call() {
        Ok(r) => r,
        Err(ureq::Error::Status(404, _)) => return Ok(None),
        Err(other) => return Err(anyhow::anyhow!("crossref request failed: {other}")),
    };
    let body = resp.into_string().context("read crossref body")?;
    let parsed: CrossrefEnvelope =
        serde_json::from_str(&body).context("parse crossref envelope")?;
    Ok(Some(parsed.message.into_canonical(doi_norm)))
}

pub fn resolve_doi_via_openalex(doi: &str) -> Result<Option<CanonicalCitation>> {
    let doi_norm = normalize_doi(doi);
    if doi_norm.is_empty() {
        return Ok(None);
    }
    let url = format!("{OPENALEX_BASE}{doi_norm}");
    let agent = ureq::AgentBuilder::new()
        .timeout_connect(HTTP_TIMEOUT)
        .timeout_read(HTTP_TIMEOUT)
        .user_agent(USER_AGENT)
        .build();
    let resp = match agent.get(&url).call() {
        Ok(r) => r,
        Err(ureq::Error::Status(404, _)) => return Ok(None),
        Err(other) => return Err(anyhow::anyhow!("openalex request failed: {other}")),
    };
    let body = resp.into_string().context("read openalex body")?;
    let parsed: OpenAlexWork = serde_json::from_str(&body).context("parse openalex work")?;
    Ok(Some(parsed.into_canonical(doi_norm)))
}

pub fn resolve_arxiv(arxiv_id: &str) -> Result<Option<CanonicalCitation>> {
    let id = arxiv_id.trim();
    if id.is_empty() {
        return Ok(None);
    }
    let url = format!("{ARXIV_BASE}{id}");
    let agent = ureq::AgentBuilder::new()
        .timeout_connect(HTTP_TIMEOUT)
        .timeout_read(HTTP_TIMEOUT)
        .user_agent(USER_AGENT)
        .build();
    let resp = match agent.get(&url).call() {
        Ok(r) => r,
        Err(ureq::Error::Status(404, _)) => return Ok(None),
        Err(other) => return Err(anyhow::anyhow!("arxiv request failed: {other}")),
    };
    let body = resp.into_string().context("read arxiv body")?;
    Ok(parse_arxiv_atom(&body, id))
}

#[derive(Debug, Deserialize)]
struct CrossrefEnvelope {
    message: CrossrefWork,
}

#[derive(Debug, Deserialize)]
struct CrossrefWork {
    #[serde(default)]
    title: Vec<String>,
    #[serde(default)]
    author: Vec<CrossrefAuthor>,
    #[serde(rename = "container-title", default)]
    container_title: Vec<String>,
    #[serde(default)]
    publisher: Option<String>,
    #[serde(default)]
    issued: Option<CrossrefIssued>,
    #[serde(default, rename = "URL")]
    url: Option<String>,
    #[serde(default)]
    abstract_text: Option<String>,
    #[serde(default)]
    license: Vec<CrossrefLicense>,
}

#[derive(Debug, Deserialize)]
struct CrossrefAuthor {
    #[serde(default)]
    given: Option<String>,
    #[serde(default)]
    family: Option<String>,
}

#[derive(Debug, Deserialize)]
struct CrossrefIssued {
    #[serde(default, rename = "date-parts")]
    date_parts: Vec<Vec<i64>>,
}

#[derive(Debug, Deserialize)]
struct CrossrefLicense {
    #[serde(default, rename = "URL")]
    url: Option<String>,
}

impl CrossrefWork {
    fn into_canonical(self, doi: String) -> CanonicalCitation {
        let title = self.title.into_iter().next();
        let authors = self
            .author
            .into_iter()
            .map(|a| match (a.family, a.given) {
                (Some(f), Some(g)) => format!("{f}, {g}"),
                (Some(f), None) => f,
                (None, Some(g)) => g,
                (None, None) => String::new(),
            })
            .filter(|s| !s.is_empty())
            .collect();
        let venue = self.container_title.into_iter().next();
        let year = self
            .issued
            .as_ref()
            .and_then(|i| i.date_parts.first())
            .and_then(|p| p.first())
            .copied();
        let license = self.license.into_iter().next().and_then(|l| l.url);
        CanonicalCitation {
            citation_kind: "doi".to_string(),
            canonical_id: doi.clone(),
            title,
            authors,
            venue,
            year,
            publisher: self.publisher,
            landing_url: self.url.or_else(|| Some(format!("https://doi.org/{doi}"))),
            full_text_url: None,
            abstract_md: self.abstract_text,
            license,
            resolver: "crossref".to_string(),
        }
    }
}

#[derive(Debug, Deserialize)]
struct OpenAlexWork {
    #[serde(default)]
    title: Option<String>,
    #[serde(default)]
    publication_year: Option<i64>,
    #[serde(default)]
    host_venue: Option<OpenAlexHostVenue>,
    #[serde(default)]
    primary_location: Option<OpenAlexLocation>,
    #[serde(default)]
    authorships: Vec<OpenAlexAuthorship>,
    #[serde(default)]
    open_access: Option<OpenAlexOpenAccess>,
    #[serde(default)]
    doi: Option<String>,
    #[serde(default)]
    abstract_inverted_index: Option<serde_json::Value>,
}

#[derive(Debug, Deserialize)]
struct OpenAlexHostVenue {
    #[serde(default)]
    display_name: Option<String>,
    #[serde(default)]
    publisher: Option<String>,
}

#[derive(Debug, Deserialize)]
struct OpenAlexLocation {
    #[serde(default)]
    landing_page_url: Option<String>,
    #[serde(default)]
    pdf_url: Option<String>,
    #[serde(default)]
    license: Option<String>,
}

#[derive(Debug, Deserialize)]
struct OpenAlexAuthorship {
    #[serde(default)]
    author: Option<OpenAlexAuthor>,
}

#[derive(Debug, Deserialize)]
struct OpenAlexAuthor {
    #[serde(default)]
    display_name: Option<String>,
}

#[derive(Debug, Deserialize)]
struct OpenAlexOpenAccess {
    #[serde(default)]
    is_oa: bool,
    #[serde(default)]
    oa_url: Option<String>,
}

impl OpenAlexWork {
    fn into_canonical(self, fallback_doi: String) -> CanonicalCitation {
        let venue = self
            .host_venue
            .as_ref()
            .and_then(|v| v.display_name.clone());
        let publisher = self.host_venue.as_ref().and_then(|v| v.publisher.clone());
        let landing = self
            .primary_location
            .as_ref()
            .and_then(|p| p.landing_page_url.clone());
        let pdf_url = self
            .primary_location
            .as_ref()
            .and_then(|p| p.pdf_url.clone());
        let license = self
            .primary_location
            .as_ref()
            .and_then(|p| p.license.clone());
        let oa_url = self
            .open_access
            .and_then(|oa| if oa.is_oa { oa.oa_url } else { None });
        let authors = self
            .authorships
            .into_iter()
            .filter_map(|a| a.author.and_then(|x| x.display_name))
            .collect();
        let abstract_md = self
            .abstract_inverted_index
            .as_ref()
            .and_then(reconstruct_inverted_abstract);
        CanonicalCitation {
            citation_kind: "doi".to_string(),
            canonical_id: self
                .doi
                .as_deref()
                .map(strip_doi_prefix)
                .unwrap_or(fallback_doi),
            title: self.title,
            authors,
            venue,
            year: self.publication_year,
            publisher,
            landing_url: landing,
            full_text_url: pdf_url.or(oa_url),
            abstract_md,
            license,
            resolver: "openalex".to_string(),
        }
    }
}

fn strip_doi_prefix(raw: &str) -> String {
    raw.trim()
        .trim_start_matches("https://doi.org/")
        .trim_start_matches("http://doi.org/")
        .trim_start_matches("doi:")
        .to_string()
}

fn reconstruct_inverted_abstract(value: &serde_json::Value) -> Option<String> {
    let map = value.as_object()?;
    let mut positioned: Vec<(usize, String)> = Vec::new();
    for (word, indices) in map {
        if let Some(arr) = indices.as_array() {
            for v in arr {
                if let Some(idx) = v.as_u64() {
                    positioned.push((idx as usize, word.clone()));
                }
            }
        }
    }
    if positioned.is_empty() {
        return None;
    }
    positioned.sort_by_key(|(idx, _)| *idx);
    Some(
        positioned
            .into_iter()
            .map(|(_, w)| w)
            .collect::<Vec<_>>()
            .join(" "),
    )
}

fn parse_arxiv_atom(body: &str, id: &str) -> Option<CanonicalCitation> {
    let title = atom_extract_first(body, "<title>", "</title>", 1)?;
    let summary = atom_extract_first(body, "<summary>", "</summary>", 0);
    let published = atom_extract_first(body, "<published>", "</published>", 0);
    let authors = atom_extract_all(body, "<name>", "</name>");
    let year = published
        .as_ref()
        .and_then(|s| s.get(..4))
        .and_then(|s| s.parse::<i64>().ok());
    Some(CanonicalCitation {
        citation_kind: "arxiv".to_string(),
        canonical_id: id.to_string(),
        title: Some(title.trim().to_string()),
        authors: authors.into_iter().map(|s| s.trim().to_string()).collect(),
        venue: Some("arXiv".to_string()),
        year,
        publisher: Some("arXiv".to_string()),
        landing_url: Some(format!("https://arxiv.org/abs/{id}")),
        full_text_url: Some(format!("https://arxiv.org/pdf/{id}")),
        abstract_md: summary.map(|s| s.trim().to_string()),
        license: None,
        resolver: "arxiv".to_string(),
    })
}

fn atom_extract_first(body: &str, open: &str, close: &str, skip: usize) -> Option<String> {
    let mut cursor = 0;
    for _ in 0..=skip {
        let start = body[cursor..].find(open)? + cursor + open.len();
        let end = body[start..].find(close)? + start;
        if skip == 0 || cursor + open.len() != start {
            cursor = end + close.len();
            if cursor == end + close.len() && start < end {
                if skip == 0 {
                    return Some(body[start..end].to_string());
                }
            }
        } else {
            cursor = end + close.len();
        }
    }
    let start = body[cursor..].find(open)? + cursor + open.len();
    let end = body[start..].find(close)? + start;
    Some(body[start..end].to_string())
}

fn atom_extract_all(body: &str, open: &str, close: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut cursor = 0;
    while let Some(rel) = body[cursor..].find(open) {
        let start = cursor + rel + open.len();
        if let Some(rel_close) = body[start..].find(close) {
            let end = start + rel_close;
            out.push(body[start..end].to_string());
            cursor = end + close.len();
        } else {
            break;
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extract_dois_finds_repeated_and_unique() {
        let txt = "see 10.1016/j.paerosci.2013.07.002 and also 10.1016/j.paerosci.2013.07.002 \
                   plus 10.3390/coatings9110727. unrelated.";
        let dois = extract_dois_from_text(txt);
        assert_eq!(
            dois,
            vec![
                "10.1016/j.paerosci.2013.07.002".to_string(),
                "10.3390/coatings9110727".to_string(),
            ]
        );
    }

    #[test]
    fn extract_arxiv_strips_versions() {
        let txt = "see arXiv:2401.12345v2 and arXiv:2310.99999.";
        let ax = extract_arxiv_from_text(txt);
        assert_eq!(ax, vec!["2310.99999".to_string(), "2401.12345".to_string()]);
    }

    #[test]
    fn normalize_doi_strips_prefix_and_punct() {
        assert_eq!(normalize_doi("doi:10.1016/x.y."), "10.1016/x.y".to_string());
    }
}
