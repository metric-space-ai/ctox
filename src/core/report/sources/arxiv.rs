//! arXiv resolver. Atom-XML parsing is intentionally regex-based — we don't
//! pull in an XML library for this; the upstream feed is regular enough.

use anyhow::Context;
use anyhow::Result;
use regex::Regex;
use serde_json::json;
use std::sync::OnceLock;
use std::time::Duration;

use super::NormalisedSource;
use super::ResolverName;
use super::SourceKind;

const ARXIV_BASE: &str = "http://export.arxiv.org/api/query?id_list=";
const CONNECT_TIMEOUT: Duration = Duration::from_secs(5);
const READ_TIMEOUT: Duration = Duration::from_secs(10);
const USER_AGENT: &str = "ctox-deep-research/dev (mailto:contact@metric-space-ai.github.io)";

pub struct ArxivClient;

impl ArxivClient {
    pub fn new() -> Self {
        Self
    }

    /// Fetch a paper by arXiv id (e.g. `2501.12345`). Returns `Ok(None)` if
    /// the feed contains no `<entry>` for the id.
    pub fn fetch_paper(&self, arxiv_id: &str) -> Result<Option<NormalisedSource>> {
        let id = arxiv_id.trim();
        if id.is_empty() {
            return Ok(None);
        }
        let url = format!("{ARXIV_BASE}{id}");
        let agent = ureq::AgentBuilder::new()
            .timeout_connect(CONNECT_TIMEOUT)
            .timeout_read(READ_TIMEOUT)
            .user_agent(USER_AGENT)
            .build();
        let resp = match agent.get(&url).call() {
            Ok(r) => r,
            Err(ureq::Error::Status(404, _)) => return Ok(None),
            Err(ureq::Error::Status(code, response)) => {
                return Err(anyhow::anyhow!(
                    "arxiv returned status {code} for {id}: {}",
                    response.status_text()
                ));
            }
            Err(other) => return Err(anyhow::anyhow!("arxiv request failed: {other}")),
        };
        let body = resp.into_string().context("read arxiv body")?;
        Ok(parse_arxiv_atom(&body, id))
    }
}

impl Default for ArxivClient {
    fn default() -> Self {
        Self::new()
    }
}

fn parse_arxiv_atom(body: &str, id: &str) -> Option<NormalisedSource> {
    let entry = extract_first_entry(body)?;
    let title = extract_tag_text(&entry, "title").map(clean_xml_text);
    let summary = extract_tag_text(&entry, "summary").map(clean_xml_text);
    let published = extract_tag_text(&entry, "published");
    let year = published
        .as_ref()
        .and_then(|s| s.get(..4))
        .and_then(|s| s.parse::<i32>().ok());
    let authors = extract_author_names(&entry);

    // arxiv returns a stub entry with no <title> when the id isn't found —
    // bail out so we don't pretend we resolved a paper.
    let title_str = title.as_ref()?.trim().to_string();
    if title_str.is_empty() {
        return None;
    }

    Some(NormalisedSource {
        kind: SourceKind::Arxiv,
        canonical_id: id.to_string(),
        title: Some(title_str),
        authors: authors.into_iter().map(clean_xml_text).collect(),
        venue: Some("arXiv".to_string()),
        year,
        publisher: Some("arXiv".to_string()),
        url_canonical: Some(format!("https://arxiv.org/abs/{id}")),
        url_full_text: Some(format!("https://arxiv.org/pdf/{id}.pdf")),
        license: None,
        abstract_md: summary,
        snippet_md: None,
        resolver_used: ResolverName::Arxiv,
        raw_payload: json!({
            "arxiv_id": id,
            "atom_xml": body,
        }),
    })
}

/// Extract the first `<entry>...</entry>` block from the Atom feed. The
/// outer `<feed>` may contain a `<title>` of its own — we want the entry's
/// inner text, not the feed banner.
fn extract_first_entry(body: &str) -> Option<String> {
    static OPEN: OnceLock<Regex> = OnceLock::new();
    let open = OPEN.get_or_init(|| Regex::new(r"(?i)<entry[\s>]").expect("compile entry-open"));
    let m = open.find(body)?;
    let after_open_marker = &body[m.start()..];
    let close_idx = after_open_marker.find("</entry>")?;
    Some(after_open_marker[..close_idx].to_string())
}

/// Extract the inner text of the first occurrence of `<TAG>...</TAG>`,
/// allowing optional attributes / namespace prefixes on the open tag.
fn extract_tag_text(scope: &str, tag: &str) -> Option<String> {
    let pattern = format!(r"(?is)<(?:[\w\-]+:)?{tag}\b[^>]*>([\s\S]*?)</(?:[\w\-]+:)?{tag}>");
    let re = Regex::new(&pattern).ok()?;
    let captures = re.captures(scope)?;
    Some(captures.get(1)?.as_str().to_string())
}

/// Extract every `<author><name>...</name></author>` block in document order.
fn extract_author_names(scope: &str) -> Vec<String> {
    static AUTHOR_RE: OnceLock<Regex> = OnceLock::new();
    let re = AUTHOR_RE.get_or_init(|| {
        Regex::new(r"(?is)<author[^>]*>([\s\S]*?)</author>").expect("compile author block")
    });
    static NAME_RE: OnceLock<Regex> = OnceLock::new();
    let name_re = NAME_RE.get_or_init(|| {
        Regex::new(r"(?is)<name[^>]*>([\s\S]*?)</name>").expect("compile author name")
    });
    let mut out = Vec::new();
    for cap in re.captures_iter(scope) {
        if let Some(block) = cap.get(1) {
            if let Some(nm) = name_re.captures(block.as_str()) {
                if let Some(text) = nm.get(1) {
                    let trimmed = text.as_str().trim();
                    if !trimmed.is_empty() {
                        out.push(trimmed.to_string());
                    }
                }
            }
        }
    }
    out
}

fn clean_xml_text(raw: String) -> String {
    raw.replace("&amp;", "&")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&apos;", "'")
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .trim()
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    const FIXTURE: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<feed xmlns="http://www.w3.org/2005/Atom">
  <title>arXiv Query</title>
  <entry>
    <title>Sample Title  About\n  Things</title>
    <summary>Lots of  text\n   here.</summary>
    <published>2024-05-08T00:00:00Z</published>
    <author><name>Jane Q. Researcher</name></author>
    <author><name>Bob Builder</name></author>
  </entry>
</feed>"#;

    #[test]
    fn parse_arxiv_atom_extracts_title_authors_year() {
        let parsed = parse_arxiv_atom(FIXTURE, "2405.00001").unwrap();
        assert_eq!(parsed.canonical_id, "2405.00001");
        assert_eq!(
            parsed.title.as_deref(),
            Some("Sample Title About\\n Things")
        );
        assert_eq!(parsed.authors.len(), 2);
        assert_eq!(parsed.authors[0], "Jane Q. Researcher");
        assert_eq!(parsed.year, Some(2024));
        assert_eq!(parsed.venue.as_deref(), Some("arXiv"));
        assert_eq!(
            parsed.url_canonical.as_deref(),
            Some("https://arxiv.org/abs/2405.00001")
        );
    }

    #[test]
    fn parse_arxiv_atom_returns_none_when_no_entry() {
        let body = r#"<?xml version="1.0"?><feed></feed>"#;
        assert!(parse_arxiv_atom(body, "9999.99999").is_none());
    }

    #[test]
    fn extract_tag_text_handles_namespace_prefix() {
        let body = "<atom:title>NS Title</atom:title>";
        assert_eq!(extract_tag_text(body, "title").as_deref(), Some("NS Title"));
    }
}
