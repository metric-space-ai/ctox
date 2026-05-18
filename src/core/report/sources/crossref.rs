//! Crossref resolver. Maps the Crossref Works API onto [`NormalisedSource`].
//!
//! API contract: `GET https://api.crossref.org/works/{DOI}` with the polite-pool
//! `User-Agent` header. 404 → `Ok(None)`. Network or 5xx → `Err`.

use anyhow::Context;
use anyhow::Result;
use regex::Regex;
use serde::Deserialize;
use serde_json::Value;
use std::sync::OnceLock;
use std::time::Duration;

use super::NormalisedSource;
use super::ResolverName;
use super::SourceKind;

const CROSSREF_BASE: &str = "https://api.crossref.org/works/";
const CONNECT_TIMEOUT: Duration = Duration::from_secs(5);
const READ_TIMEOUT: Duration = Duration::from_secs(10);
const FALLBACK_USER_AGENT: &str =
    "ctox-deep-research/dev (mailto:contact@metric-space-ai.github.io)";

pub struct CrossrefClient {
    user_agent: String,
}

impl CrossrefClient {
    pub fn new(contact_email: Option<&str>) -> Self {
        let user_agent = match contact_email {
            Some(email) if !email.trim().is_empty() => {
                format!("ctox-deep-research/dev (mailto:{})", email.trim())
            }
            _ => FALLBACK_USER_AGENT.to_string(),
        };
        Self { user_agent }
    }

    pub fn user_agent(&self) -> &str {
        &self.user_agent
    }

    /// Fetch a Crossref work by DOI. Returns `Ok(None)` on 404; `Err` on
    /// network failure or 5xx status. Never fabricates a record.
    pub fn fetch_work(&self, doi: &str) -> Result<Option<NormalisedSource>> {
        let trimmed = normalise_doi(doi);
        if trimmed.is_empty() {
            return Ok(None);
        }
        let url = format!("{CROSSREF_BASE}{trimmed}");
        let agent = ureq::AgentBuilder::new()
            .timeout_connect(CONNECT_TIMEOUT)
            .timeout_read(READ_TIMEOUT)
            .user_agent(&self.user_agent)
            .build();
        let resp = match agent.get(&url).call() {
            Ok(r) => r,
            Err(ureq::Error::Status(404, _)) => return Ok(None),
            Err(ureq::Error::Status(code, response)) => {
                return Err(anyhow::anyhow!(
                    "crossref returned status {code} for {trimmed}: {}",
                    response.status_text()
                ));
            }
            Err(other) => return Err(anyhow::anyhow!("crossref request failed: {other}")),
        };
        let body = resp.into_string().context("read crossref body")?;
        let raw: Value = serde_json::from_str(&body).context("parse crossref envelope as JSON")?;
        let envelope: CrossrefEnvelope =
            serde_json::from_value(raw.clone()).context("parse crossref envelope")?;
        Ok(Some(envelope.message.into_normalised(trimmed, raw)))
    }
}

fn normalise_doi(raw: &str) -> String {
    raw.trim()
        .trim_start_matches("https://doi.org/")
        .trim_start_matches("http://doi.org/")
        .trim_start_matches("doi:")
        .trim_end_matches('.')
        .trim_end_matches(',')
        .to_ascii_lowercase()
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
    #[serde(default)]
    link: Vec<CrossrefLink>,
    #[serde(default, rename = "abstract")]
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
struct CrossrefLink {
    #[serde(default, rename = "URL")]
    url: Option<String>,
    #[serde(default, rename = "content-version")]
    content_version: Option<String>,
}

#[derive(Debug, Deserialize)]
struct CrossrefLicense {
    #[serde(default, rename = "URL")]
    url: Option<String>,
}

impl CrossrefWork {
    fn into_normalised(self, doi: String, raw: Value) -> NormalisedSource {
        let title = self.title.into_iter().find(|s| !s.trim().is_empty());
        let authors = self
            .author
            .into_iter()
            .filter_map(|a| match (a.given, a.family) {
                (Some(g), Some(f)) if !g.trim().is_empty() && !f.trim().is_empty() => {
                    Some(format!("{} {}", g.trim(), f.trim()))
                }
                (Some(g), None) if !g.trim().is_empty() => Some(g.trim().to_string()),
                (None, Some(f)) if !f.trim().is_empty() => Some(f.trim().to_string()),
                _ => None,
            })
            .collect();
        let venue = self
            .container_title
            .into_iter()
            .find(|s| !s.trim().is_empty());
        let year = self
            .issued
            .as_ref()
            .and_then(|i| i.date_parts.first())
            .and_then(|p| p.first())
            .map(|y| *y as i32);
        let url_full_text = self
            .link
            .into_iter()
            .find(|l| {
                l.content_version
                    .as_deref()
                    .map(|cv| cv.eq_ignore_ascii_case("vor"))
                    .unwrap_or(false)
                    && l.url.is_some()
            })
            .and_then(|l| l.url);
        let license = self
            .license
            .into_iter()
            .find(|l| l.url.is_some())
            .and_then(|l| l.url);
        let abstract_md = self.abstract_text.as_deref().map(strip_jats_xml);
        NormalisedSource {
            kind: SourceKind::Doi,
            canonical_id: doi.clone(),
            title,
            authors,
            venue,
            year,
            publisher: self.publisher,
            url_canonical: Some(format!("https://doi.org/{doi}")),
            url_full_text,
            license,
            abstract_md,
            snippet_md: None,
            resolver_used: ResolverName::Crossref,
            raw_payload: raw,
        }
    }
}

/// Crossref abstracts are sometimes returned as JATS XML — strip tags with a
/// permissive regex pass. This is a best-effort cleanup, not a parser.
fn strip_jats_xml(raw: &str) -> String {
    static R: OnceLock<Regex> = OnceLock::new();
    let re = R.get_or_init(|| Regex::new(r"<[^>]+>").expect("compile JATS-strip regex"));
    let stripped = re.replace_all(raw, "");
    stripped
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .trim()
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn user_agent_includes_email_when_supplied() {
        let c = CrossrefClient::new(Some("alice@example.org"));
        assert!(c.user_agent().contains("alice@example.org"));
    }

    #[test]
    fn user_agent_falls_back_when_no_email() {
        let c = CrossrefClient::new(None);
        assert!(c.user_agent().contains("metric-space-ai"));
    }

    #[test]
    fn jats_strip_removes_tags_and_collapses_whitespace() {
        let s = strip_jats_xml("<jats:p>Hello\n\n<jats:italic>world</jats:italic>.</jats:p>");
        assert_eq!(s, "Hello world.");
    }

    #[test]
    fn normalise_doi_handles_prefixes_and_punct() {
        assert_eq!(normalise_doi("doi:10.1000/AbC."), "10.1000/abc");
        assert_eq!(
            normalise_doi("https://doi.org/10.1234/x.y"),
            "10.1234/x.y".to_string()
        );
    }
}
