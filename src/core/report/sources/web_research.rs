//! Adapter around the existing deep-research engine in
//! `tools/web-stack/src/deep_research.rs`. Used for free-form web queries
//! (when the manager doesn't have a known DOI / arXiv id yet).
//!
//! The adapter is deliberately thin: it does NOT auto-resolve identifiers
//! it extracts from the bundle. The Wave-4 `public_research` tool decides
//! whether to feed those identifiers back into [`super::ResolverStack`].

use anyhow::Result;
use regex::Regex;
use serde_json::Value;
use std::path::Path;
use std::path::PathBuf;
use std::sync::OnceLock;

use ctox_web_stack::run_ctox_deep_research_tool;
use ctox_web_stack::DeepResearchDepth;
use ctox_web_stack::DeepResearchRequest;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WebResearchDepth {
    Quick,
    Standard,
    Exhaustive,
}

impl WebResearchDepth {
    fn into_engine(self) -> DeepResearchDepth {
        match self {
            WebResearchDepth::Quick => DeepResearchDepth::Quick,
            WebResearchDepth::Standard => DeepResearchDepth::Standard,
            WebResearchDepth::Exhaustive => DeepResearchDepth::Exhaustive,
        }
    }

    pub fn from_label(raw: &str) -> Option<Self> {
        match raw.trim().to_ascii_lowercase().as_str() {
            "quick" | "low" => Some(WebResearchDepth::Quick),
            "standard" | "medium" => Some(WebResearchDepth::Standard),
            "exhaustive" | "high" | "deep" => Some(WebResearchDepth::Exhaustive),
            _ => None,
        }
    }
}

impl Default for WebResearchDepth {
    fn default() -> Self {
        WebResearchDepth::Standard
    }
}

#[derive(Debug, Clone)]
pub struct WebResearchQuery {
    pub question: String,
    pub focus: Option<String>,
    pub depth: WebResearchDepth,
    pub max_sources: usize,
    pub workspace_path: Option<PathBuf>,
}

#[derive(Debug, Clone)]
pub struct WebResearchOutcome {
    pub summary: String,
    pub sources_found: usize,
    pub doi_extracted: Vec<String>,
    pub arxiv_extracted: Vec<String>,
    pub raw_bundle: Value,
}

pub struct WebResearchAdapter {
    root: PathBuf,
    #[allow(dead_code)]
    run_id: String,
}

impl WebResearchAdapter {
    pub fn new(root: &Path, run_id: &str) -> Self {
        Self {
            root: root.to_path_buf(),
            run_id: run_id.to_string(),
        }
    }

    pub fn execute(&self, query: &WebResearchQuery) -> Result<WebResearchOutcome> {
        let request = DeepResearchRequest {
            query: query.question.clone(),
            focus: query.focus.clone(),
            depth: query.depth.into_engine(),
            max_sources: query.max_sources,
            include_annas_archive: false,
            include_papers: true,
            workspace: query.workspace_path.clone(),
            persist_workspace: query.workspace_path.is_some(),
        };
        let bundle = run_ctox_deep_research_tool(&self.root, &request)?;
        let sources_found = bundle
            .get("sources")
            .and_then(Value::as_array)
            .map(|arr| arr.len())
            .unwrap_or(0);
        let combined = collect_text_corpus(&bundle);
        let doi_extracted = extract_dois(&combined);
        let arxiv_extracted = extract_arxiv_ids(&combined);
        let summary = build_summary(&bundle, sources_found);
        Ok(WebResearchOutcome {
            summary,
            sources_found,
            doi_extracted,
            arxiv_extracted,
            raw_bundle: bundle,
        })
    }
}

/// Return true only for a source that has passed the complete evidence gate.
/// Discovery metadata and failed or incomplete reads remain in the raw bundle
/// for audit, but must not feed a report corpus or identifier extraction.
pub(crate) fn is_evidence_eligible_source(source: &Value) -> bool {
    let metadata_only = source
        .get("metadata_only")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let source_type = source
        .get("source_type")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_ascii_lowercase();
    let source_tier = source
        .get("source_tier")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_ascii_lowercase();
    let canonical_url = source
        .get("canonical_url")
        .and_then(Value::as_str)
        .is_some_and(is_canonical_http_url);
    let canonical_url_is_metadata = source
        .get("canonical_url")
        .and_then(Value::as_str)
        .is_some_and(is_metadata_canonical_url);
    let successful_transport = source
        .get("http_status")
        .and_then(Value::as_i64)
        .is_some_and(|status| (200..=299).contains(&status));
    let snapshotted = source
        .get("snapshot_hash")
        .and_then(Value::as_str)
        .is_some_and(is_sha256_receipt);
    let has_rejection_reason = source
        .get("evidence_rejection_reason")
        .and_then(Value::as_str)
        .is_some_and(|reason| !reason.trim().is_empty());

    source.get("evidence_eligible").and_then(Value::as_bool) == Some(true)
        && source.get("verification_status").and_then(Value::as_str) == Some("verified")
        && source.get("transport_verified").and_then(Value::as_bool) == Some(true)
        && source.get("content_extracted").and_then(Value::as_bool) == Some(true)
        && source
            .get("actual_full_text_or_data")
            .and_then(Value::as_bool)
            == Some(true)
        && source
            .get("evidence_relevance_score")
            .and_then(Value::as_i64)
            .is_some_and(|score| score >= 8)
        && successful_transport
        && snapshotted
        && canonical_url
        && !canonical_url_is_metadata
        && !metadata_only
        && !source_type.contains("metadata")
        && !source_tier.contains("metadata")
        && source_type != "aggregator"
        && source_tier != "aggregator"
        && !has_rejection_reason
}

fn is_canonical_http_url(raw: &str) -> bool {
    url::Url::parse(raw.trim())
        .is_ok_and(|url| matches!(url.scheme(), "http" | "https") && url.host_str().is_some())
}

fn is_sha256_receipt(raw: &str) -> bool {
    raw.strip_prefix("sha256:").is_some_and(|digest| {
        digest.len() == 64 && digest.bytes().all(|byte| byte.is_ascii_hexdigit())
    })
}

fn is_metadata_canonical_url(url: &str) -> bool {
    let normalized = url.trim().to_ascii_lowercase();
    [
        "https://doi.org/",
        "http://doi.org/",
        "https://api.crossref.org/",
        "https://api.openalex.org/",
        "https://api.semanticscholar.org/",
        "https://www.semanticscholar.org/",
        "https://scholar.google.",
        "https://www.researchgate.net/",
        "https://www.academia.edu/",
    ]
    .iter()
    .any(|prefix| normalized.starts_with(prefix))
}

/// Extract only source content from an eligible record. In particular, do not
/// use search snippets or scholarly metadata as a substitute for a snapshot.
pub(crate) fn evidence_text(source: &Value) -> String {
    let mut buf = String::new();
    if let Some(s) = source.get("canonical_url").and_then(Value::as_str) {
        append_line(&mut buf, s);
    }
    if let Some(s) = source.get("title").and_then(Value::as_str) {
        append_line(&mut buf, s);
    }
    if let Some(read) = source.get("read") {
        if let Some(s) = read.get("summary").and_then(Value::as_str) {
            append_line(&mut buf, s);
        }
        if let Some(excerpts) = read.get("excerpts").and_then(Value::as_array) {
            for excerpt in excerpts {
                if let Some(s) = excerpt.as_str() {
                    append_line(&mut buf, s);
                } else if let Some(s) = excerpt.get("text").and_then(Value::as_str) {
                    append_line(&mut buf, s);
                }
            }
        }
        if let Some(find_results) = read.get("find_results").and_then(Value::as_array) {
            for result in find_results {
                if let Some(matches) = result.get("matches").and_then(Value::as_array) {
                    for value in matches.iter().filter_map(Value::as_str) {
                        append_line(&mut buf, value);
                    }
                }
            }
        }
    }
    buf
}

fn append_line(buf: &mut String, text: &str) {
    buf.push_str(text);
    buf.push('\n');
}

/// Walk only eligible source records and assemble a text blob suitable for
/// regex DOI / arXiv extraction. Rejected candidates are intentionally absent.
fn collect_text_corpus(bundle: &Value) -> String {
    let mut buf = String::new();
    if let Some(arr) = bundle.get("sources").and_then(Value::as_array) {
        for src in arr {
            if is_evidence_eligible_source(src) {
                buf.push_str(&evidence_text(src));
            }
        }
    }
    buf
}

fn doi_regex() -> &'static Regex {
    static R: OnceLock<Regex> = OnceLock::new();
    R.get_or_init(|| {
        Regex::new(r"\b10\.\d{4,9}/[-._;()/:A-Za-z0-9]+\b").expect("compile DOI regex")
    })
}

fn arxiv_regex() -> &'static Regex {
    static R: OnceLock<Regex> = OnceLock::new();
    R.get_or_init(|| {
        Regex::new(r"\barXiv:?\s*(\d{4}\.\d{4,5})(?:v\d+)?").expect("compile arXiv regex")
    })
}

fn extract_dois(text: &str) -> Vec<String> {
    let mut out: Vec<String> = doi_regex()
        .find_iter(text)
        .map(|m| {
            m.as_str()
                .trim_end_matches('.')
                .trim_end_matches(',')
                .to_ascii_lowercase()
        })
        .collect();
    out.sort();
    out.dedup();
    out
}

fn extract_arxiv_ids(text: &str) -> Vec<String> {
    let mut out: Vec<String> = arxiv_regex()
        .captures_iter(text)
        .filter_map(|c| c.get(1).map(|m| m.as_str().to_string()))
        .collect();
    out.sort();
    out.dedup();
    out
}

/// Build a short human-readable summary from the bundle. We don't try to
/// duplicate the agent-side formatting in the deep-research engine — just
/// collect the top-level summary fields.
fn build_summary(bundle: &Value, sources_found: usize) -> String {
    let mut out = String::new();
    if let Some(q) = bundle.get("query").and_then(Value::as_str) {
        out.push_str("Query: ");
        out.push_str(q);
        out.push('\n');
    }
    if let Some(d) = bundle.get("depth").and_then(Value::as_str) {
        out.push_str("Depth: ");
        out.push_str(d);
        out.push('\n');
    }
    if let Some(counts) = bundle.get("research_call_counts") {
        out.push_str("Counts: ");
        out.push_str(&counts.to_string());
        out.push('\n');
    }
    out.push_str(&format!("Sources returned: {sources_found}\n"));
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn extract_dois_deduplicates_and_lowercases() {
        let txt = "see 10.1016/J.PAEROSCI.2013.07.002 and 10.1016/j.paerosci.2013.07.002.";
        let dois = extract_dois(txt);
        assert_eq!(dois.len(), 1);
        assert_eq!(dois[0], "10.1016/j.paerosci.2013.07.002");
    }

    #[test]
    fn extract_arxiv_ids_strips_version() {
        let txt = "see arXiv: 2401.12345v3 and arXiv:2310.99999.";
        let ax = extract_arxiv_ids(txt);
        assert_eq!(ax, vec!["2310.99999".to_string(), "2401.12345".to_string()]);
    }

    #[test]
    fn collect_text_corpus_excludes_rejected_and_metadata_sources() {
        let bundle = json!({
            "sources": [
                {
                    "url": "https://dead.example/source",
                    "canonical_url": "https://publisher.example/source",
                    "source_type": "scholarly",
                    "source_tier": "scholarly",
                    "verification_status": "verified",
                    "transport_verified": true,
                    "content_extracted": true,
                    "actual_full_text_or_data": true,
                    "evidence_relevance_score": 32,
                    "http_status": 200,
                    "snapshot_hash": format!("sha256:{}", "a".repeat(64)),
                    "evidence_eligible": true,
                    "title": "Accepted source",
                    "read": {
                        "summary": "accepted arXiv:2401.12345",
                        "excerpts": ["accepted DOI 10.5678/xyz.qq"]
                    }
                },
                {
                    "canonical_url": "https://metadata.example/paper",
                    "source_type": "paper_metadata",
                    "source_tier": "metadata",
                    "metadata_only": true,
                    "verification_status": "verified",
                    "transport_verified": true,
                    "content_extracted": true,
                    "http_status": 200,
                    "snapshot_hash": "sha256:metadata",
                    "evidence_eligible": true,
                    "read": {
                        "summary": "metadata DOI 10.9999/metadata"
                    }
                },
                {
                    "canonical_url": "https://dead.example/paper",
                    "verification_status": "failed",
                    "transport_verified": false,
                    "content_extracted": false,
                    "http_status": 404,
                    "snapshot_hash": null,
                    "evidence_eligible": false,
                    "read": {
                        "summary": "dead DOI 10.8888/dead"
                    }
                }
            ]
        });
        let blob = collect_text_corpus(&bundle);
        assert!(blob.contains("accepted arXiv:2401.12345"));
        assert!(blob.contains("10.5678/xyz.qq"));
        assert!(!blob.contains("10.9999/metadata"));
        assert!(!blob.contains("10.8888/dead"));
        assert_eq!(extract_dois(&blob), vec!["10.5678/xyz.qq"]);
    }

    #[test]
    fn eligibility_requires_canonical_snapshot_and_verified_transport() {
        let mut source = json!({
            "canonical_url": "https://publisher.example/source",
            "source_type": "web",
            "source_tier": "web",
            "verification_status": "verified",
            "transport_verified": true,
            "content_extracted": true,
            "actual_full_text_or_data": true,
            "evidence_relevance_score": 32,
            "http_status": 200,
            "snapshot_hash": format!("sha256:{}", "a".repeat(64)),
            "evidence_eligible": true
        });
        assert!(is_evidence_eligible_source(&source));

        for field in ["canonical_url", "snapshot_hash"] {
            let mut invalid = source.clone();
            invalid[field] = Value::String(String::new());
            assert!(!is_evidence_eligible_source(&invalid), "missing {field}");
        }
        source["snapshot_hash"] = json!("sha256:not-a-content-hash");
        assert!(!is_evidence_eligible_source(&source));
        source["snapshot_hash"] = json!(format!("sha256:{}", "a".repeat(64)));
        source["http_status"] = json!(500);
        assert!(!is_evidence_eligible_source(&source));
        source["http_status"] = json!(200);
        source["canonical_url"] = json!("https://doi.org/10.1234/metadata");
        assert!(!is_evidence_eligible_source(&source));
        source["canonical_url"] = json!("https://publisher.example/source");
        source["actual_full_text_or_data"] = json!(false);
        assert!(!is_evidence_eligible_source(&source));
    }

    #[test]
    fn depth_round_trip() {
        for label in ["quick", "standard", "exhaustive"] {
            assert!(WebResearchDepth::from_label(label).is_some());
        }
    }
}
