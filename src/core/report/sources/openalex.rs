//! OpenAlex resolver. Fallback for DOIs that Crossref does not return.
//!
//! API contract: `GET https://api.openalex.org/works/doi:{DOI}`. 404 →
//! `Ok(None)`. The `abstract_inverted_index` reconstruction is the standard
//! OpenAlex pattern: each word maps to a list of positions; we sort by
//! minimum position and join.

use anyhow::Context;
use anyhow::Result;
use serde::Deserialize;
use serde_json::Value;
use std::time::Duration;

use super::NormalisedSource;
use super::ResolverName;
use super::SourceKind;

const OPENALEX_BASE: &str = "https://api.openalex.org/works/doi:";
const CONNECT_TIMEOUT: Duration = Duration::from_secs(5);
const READ_TIMEOUT: Duration = Duration::from_secs(10);
const FALLBACK_USER_AGENT: &str =
    "ctox-deep-research/dev (mailto:contact@metric-space-ai.github.io)";

pub struct OpenAlexClient {
    user_agent: String,
}

impl OpenAlexClient {
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

    pub fn fetch_work_by_doi(&self, doi: &str) -> Result<Option<NormalisedSource>> {
        let trimmed = normalise_doi(doi);
        if trimmed.is_empty() {
            return Ok(None);
        }
        let url = format!("{OPENALEX_BASE}{trimmed}");
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
                    "openalex returned status {code} for {trimmed}: {}",
                    response.status_text()
                ));
            }
            Err(other) => return Err(anyhow::anyhow!("openalex request failed: {other}")),
        };
        let body = resp.into_string().context("read openalex body")?;
        let raw: Value = serde_json::from_str(&body).context("parse openalex body as JSON")?;
        let parsed: OpenAlexWork =
            serde_json::from_value(raw.clone()).context("parse openalex work")?;
        Ok(Some(parsed.into_normalised(trimmed, raw)))
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
    best_oa_location: Option<OpenAlexLocation>,
    #[serde(default)]
    authorships: Vec<OpenAlexAuthorship>,
    #[serde(default)]
    doi: Option<String>,
    #[serde(default)]
    abstract_inverted_index: Option<Value>,
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
    #[serde(default)]
    source: Option<OpenAlexSource>,
}

#[derive(Debug, Deserialize)]
struct OpenAlexSource {
    #[serde(default)]
    display_name: Option<String>,
    #[serde(default)]
    publisher: Option<String>,
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

impl OpenAlexWork {
    fn into_normalised(self, fallback_doi: String, raw: Value) -> NormalisedSource {
        let venue = self
            .host_venue
            .as_ref()
            .and_then(|v| v.display_name.clone())
            .or_else(|| {
                self.primary_location
                    .as_ref()
                    .and_then(|l| l.source.as_ref())
                    .and_then(|s| s.display_name.clone())
            });
        let publisher = self
            .host_venue
            .as_ref()
            .and_then(|v| v.publisher.clone())
            .or_else(|| {
                self.primary_location
                    .as_ref()
                    .and_then(|l| l.source.as_ref())
                    .and_then(|s| s.publisher.clone())
            });
        let url_full_text = self
            .best_oa_location
            .as_ref()
            .and_then(|l| l.pdf_url.clone());
        let license = self
            .best_oa_location
            .as_ref()
            .and_then(|l| l.license.clone())
            .or_else(|| {
                self.primary_location
                    .as_ref()
                    .and_then(|l| l.license.clone())
            });
        let landing = self
            .primary_location
            .as_ref()
            .and_then(|l| l.landing_page_url.clone());
        let canonical_id = self
            .doi
            .as_deref()
            .map(strip_doi_prefix)
            .filter(|s| !s.is_empty())
            .unwrap_or(fallback_doi.clone());
        let authors = self
            .authorships
            .into_iter()
            .filter_map(|a| a.author.and_then(|x| x.display_name))
            .filter(|s| !s.trim().is_empty())
            .collect();
        let abstract_md = self
            .abstract_inverted_index
            .as_ref()
            .and_then(reconstruct_inverted_abstract);
        NormalisedSource {
            kind: SourceKind::Doi,
            canonical_id: canonical_id.clone(),
            title: self.title.filter(|s| !s.trim().is_empty()),
            authors,
            venue,
            year: self.publication_year.map(|y| y as i32),
            publisher,
            url_canonical: landing.or_else(|| Some(format!("https://doi.org/{canonical_id}"))),
            url_full_text,
            license,
            abstract_md,
            snippet_md: None,
            resolver_used: ResolverName::OpenAlex,
            raw_payload: raw,
        }
    }
}

fn strip_doi_prefix(raw: &str) -> String {
    raw.trim()
        .trim_start_matches("https://doi.org/")
        .trim_start_matches("http://doi.org/")
        .trim_start_matches("doi:")
        .to_ascii_lowercase()
}

/// Reconstruct OpenAlex's inverted abstract: a dict word → positions; emit
/// the words in ascending position order.
fn reconstruct_inverted_abstract(value: &Value) -> Option<String> {
    let map = value.as_object()?;
    if map.is_empty() {
        return None;
    }
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
    let words: Vec<String> = positioned.into_iter().map(|(_, w)| w).collect();
    Some(words.join(" "))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn reconstruct_inverted_abstract_orders_by_position() {
        let v = json!({
            "world": [1],
            "Hello": [0],
            "again": [2]
        });
        let s = reconstruct_inverted_abstract(&v).unwrap();
        assert_eq!(s, "Hello world again");
    }

    #[test]
    fn reconstruct_inverted_abstract_returns_none_on_empty() {
        let v = json!({});
        assert!(reconstruct_inverted_abstract(&v).is_none());
    }

    #[test]
    fn user_agent_includes_email() {
        let c = OpenAlexClient::new(Some("bob@example.com"));
        assert!(c.user_agent().contains("bob@example.com"));
    }

    #[test]
    fn strip_doi_prefix_handles_full_url() {
        assert_eq!(
            strip_doi_prefix("https://doi.org/10.1234/AbC"),
            "10.1234/abc".to_string()
        );
    }
}
