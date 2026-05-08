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

/// Walk the bundle's source records and assemble a single text blob suitable
/// for regex DOI / arXiv extraction. We pull `snippet`, `read.summary`, and
/// any `read.excerpts[]` strings.
fn collect_text_corpus(bundle: &Value) -> String {
    let mut buf = String::new();
    if let Some(arr) = bundle.get("sources").and_then(Value::as_array) {
        for src in arr {
            if let Some(s) = src.get("snippet").and_then(Value::as_str) {
                buf.push_str(s);
                buf.push('\n');
            }
            if let Some(s) = src.get("title").and_then(Value::as_str) {
                buf.push_str(s);
                buf.push('\n');
            }
            if let Some(s) = src.get("url").and_then(Value::as_str) {
                buf.push_str(s);
                buf.push('\n');
            }
            if let Some(read) = src.get("read") {
                if let Some(s) = read.get("summary").and_then(Value::as_str) {
                    buf.push_str(s);
                    buf.push('\n');
                }
                if let Some(exs) = read.get("excerpts").and_then(Value::as_array) {
                    for ex in exs {
                        if let Some(s) = ex.as_str() {
                            buf.push_str(s);
                            buf.push('\n');
                        } else if let Some(s) = ex.get("text").and_then(Value::as_str) {
                            buf.push_str(s);
                            buf.push('\n');
                        }
                    }
                }
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
    fn collect_text_corpus_walks_sources_and_reads() {
        let bundle = json!({
            "sources": [
                {
                    "snippet": "see 10.1234/abc.def",
                    "read": {
                        "summary": "another arXiv:2401.12345",
                        "excerpts": ["10.5678/xyz.qq"]
                    }
                }
            ]
        });
        let blob = collect_text_corpus(&bundle);
        assert!(blob.contains("10.1234/abc.def"));
        assert!(blob.contains("arXiv:2401.12345"));
        assert!(blob.contains("10.5678/xyz.qq"));
    }

    #[test]
    fn depth_round_trip() {
        for label in ["quick", "standard", "exhaustive"] {
            assert!(WebResearchDepth::from_label(label).is_some());
        }
    }
}
