//! External source resolvers and adapters around the existing
//! `tools/web-stack` deep_research engine.
//!
//! Two layers live here side by side:
//!
//! * The legacy `scholarly` + `web` modules still drive
//!   `crate::report::evidence` (Wave 2). They normalise into
//!   [`scholarly::CanonicalCitation`] and write to `report_evidence`.
//! * The new resolver subsystem (Wave 3) — `crossref`, `openalex`, `arxiv`,
//!   `web_research`, `cache` — fronts the `report_evidence_register` table
//!   that the deep-research skill manager consults before it can call
//!   `write_with_skill`. The release_guard lints LINT-FAB-DOI,
//!   LINT-DOI-NOT-RESOLVED, LINT-CITED-BUT-MISSING, LINT-EVIDENCE-FLOOR,
//!   and LINT-EVIDENCE-CONCENTRATION all read from this register.
//!
//! The two layers share zero state; the legacy layer continues to use
//! `report_evidence`, the new layer writes `report_evidence_register`. They
//! coexist while higher waves migrate the consumer side.

pub mod arxiv;
pub mod cache;
pub mod crossref;
pub mod openalex;
pub mod scholarly;
pub mod web;
pub mod web_research;

// Legacy re-exports — kept stable for `crate::report::evidence`.
pub use scholarly::extract_dois_from_text;
pub use scholarly::resolve_arxiv;
pub use scholarly::resolve_doi_via_crossref;
pub use scholarly::resolve_doi_via_openalex;
pub use scholarly::CanonicalCitation;

// New Wave-3 surface.
pub use arxiv::ArxivClient;
pub use cache::EvidenceCache;
pub use cache::EvidenceEntry;
pub use crossref::CrossrefClient;
pub use openalex::OpenAlexClient;
pub use web_research::WebResearchAdapter;
pub use web_research::WebResearchDepth;
pub use web_research::WebResearchOutcome;
pub use web_research::WebResearchQuery;

use anyhow::Context;
use anyhow::Result;
use rusqlite::Connection;
use serde::Deserialize;
use serde::Serialize;
use sha2::Digest;
use sha2::Sha256;
use std::path::Path;
use std::path::PathBuf;

use crate::paths;
use crate::persistence;

/// What external object a [`NormalisedSource`] points at. The string form is
/// stable on disk because it is part of the `evidence_id` hash input.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SourceKind {
    Doi,
    Arxiv,
    Url,
    Standard,
    Patent,
    Book,
}

impl SourceKind {
    pub fn as_str(self) -> &'static str {
        match self {
            SourceKind::Doi => "doi",
            SourceKind::Arxiv => "arxiv",
            SourceKind::Url => "url",
            SourceKind::Standard => "standard",
            SourceKind::Patent => "patent",
            SourceKind::Book => "book",
        }
    }

    pub fn from_str(raw: &str) -> Option<Self> {
        match raw.trim().to_ascii_lowercase().as_str() {
            "doi" => Some(SourceKind::Doi),
            "arxiv" => Some(SourceKind::Arxiv),
            "url" => Some(SourceKind::Url),
            "standard" => Some(SourceKind::Standard),
            "patent" => Some(SourceKind::Patent),
            "book" => Some(SourceKind::Book),
            _ => None,
        }
    }
}

/// Which resolver produced the [`NormalisedSource`]. Stored on the row so
/// release_guard lints can attribute records to a source of truth.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ResolverName {
    Crossref,
    OpenAlex,
    Arxiv,
    Web,
    Cache,
    Manual,
}

impl ResolverName {
    pub fn as_str(self) -> &'static str {
        match self {
            ResolverName::Crossref => "crossref",
            ResolverName::OpenAlex => "openalex",
            ResolverName::Arxiv => "arxiv",
            ResolverName::Web => "web",
            ResolverName::Cache => "cache",
            ResolverName::Manual => "manual",
        }
    }

    pub fn from_str(raw: &str) -> Option<Self> {
        match raw.trim().to_ascii_lowercase().as_str() {
            "crossref" => Some(ResolverName::Crossref),
            "openalex" => Some(ResolverName::OpenAlex),
            "arxiv" => Some(ResolverName::Arxiv),
            "web" => Some(ResolverName::Web),
            "cache" => Some(ResolverName::Cache),
            "manual" => Some(ResolverName::Manual),
            _ => None,
        }
    }
}

/// Resolver-agnostic record. One per external source. The `raw_payload` keeps
/// the upstream JSON intact so audit / re-normalisation is possible without
/// re-fetching.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NormalisedSource {
    pub kind: SourceKind,
    pub canonical_id: String,
    pub title: Option<String>,
    pub authors: Vec<String>,
    pub venue: Option<String>,
    pub year: Option<i32>,
    pub publisher: Option<String>,
    pub url_canonical: Option<String>,
    pub url_full_text: Option<String>,
    pub license: Option<String>,
    pub abstract_md: Option<String>,
    pub snippet_md: Option<String>,
    pub resolver_used: ResolverName,
    pub raw_payload: serde_json::Value,
}

/// Stable evidence id derived from `(kind, canonical_id)`. Format:
/// `ev_{first 16 hex of sha256(kind:canonical_id_lower)}`.
pub fn derive_evidence_id(kind: SourceKind, canonical_id: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(kind.as_str().as_bytes());
    hasher.update(b":");
    hasher.update(canonical_id.trim().to_ascii_lowercase().as_bytes());
    let digest = hasher.finalize();
    let mut hex = String::with_capacity(2 + 16);
    hex.push_str("ev_");
    for byte in digest.iter().take(8) {
        use std::fmt::Write as _;
        let _ = write!(&mut hex, "{:02x}", byte);
    }
    hex
}

/// Top-level resolver entry point. Owns:
///
/// * a connection to `runtime/ctox.sqlite3` (lazily opened on first need),
/// * the per-resolver clients (Crossref / OpenAlex / arXiv),
/// * the deep-research engine adapter (wraps `tools/web-stack`),
/// * the per-run cache (read-through over `report_evidence_register`).
pub struct ResolverStack {
    root: PathBuf,
    run_id: String,
    contact_email: Option<String>,
    crossref: CrossrefClient,
    openalex: OpenAlexClient,
    arxiv_client: ArxivClient,
    web_research: WebResearchAdapter,
}

impl ResolverStack {
    pub fn new(root: &Path, run_id: &str, contact_email: Option<&str>) -> Result<Self> {
        let crossref = CrossrefClient::new(contact_email);
        let openalex = OpenAlexClient::new(contact_email);
        let arxiv_client = ArxivClient::new();
        let web_research = WebResearchAdapter::new(root, run_id);
        Ok(Self {
            root: root.to_path_buf(),
            run_id: run_id.to_string(),
            contact_email: contact_email.map(|s| s.to_string()),
            crossref,
            openalex,
            arxiv_client,
            web_research,
        })
    }

    pub fn run_id(&self) -> &str {
        &self.run_id
    }

    pub fn contact_email(&self) -> Option<&str> {
        self.contact_email.as_deref()
    }

    /// Crossref → OpenAlex fallback chain. `Ok(None)` means both resolvers
    /// returned `404` (i.e. the DOI does not resolve at all). A network error
    /// at one resolver does not fall through; it bubbles up as `Err` so the
    /// caller can decide whether to retry.
    pub fn resolve_doi(&self, doi: &str) -> Result<Option<NormalisedSource>> {
        let trimmed = normalise_doi(doi);
        if trimmed.is_empty() {
            return Ok(None);
        }
        match self.crossref.fetch_work(&trimmed)? {
            Some(s) => Ok(Some(s)),
            None => self.openalex.fetch_work_by_doi(&trimmed),
        }
    }

    pub fn resolve_arxiv(&self, arxiv_id: &str) -> Result<Option<NormalisedSource>> {
        self.arxiv_client.fetch_paper(arxiv_id)
    }

    /// If the URL embeds a DOI (very common for `doi.org`, publisher landing
    /// pages, etc.), recurse through the DOI chain. Otherwise emit a bare
    /// [`SourceKind::Url`] record with no enrichment — the caller decides
    /// whether to fetch the page contents separately.
    pub fn resolve_url(&self, url: &str) -> Result<Option<NormalisedSource>> {
        let trimmed = url.trim();
        if trimmed.is_empty() {
            return Ok(None);
        }
        if let Some(doi) = extract_doi_from_url(trimmed) {
            if let Some(resolved) = self.resolve_doi(&doi)? {
                return Ok(Some(resolved));
            }
        }
        Ok(Some(NormalisedSource {
            kind: SourceKind::Url,
            canonical_id: trimmed.to_string(),
            title: None,
            authors: Vec::new(),
            venue: None,
            year: None,
            publisher: None,
            url_canonical: Some(trimmed.to_string()),
            url_full_text: None,
            license: None,
            abstract_md: None,
            snippet_md: None,
            resolver_used: ResolverName::Web,
            raw_payload: serde_json::json!({ "url": trimmed }),
        }))
    }

    pub fn execute_query(&self, query: &WebResearchQuery) -> Result<WebResearchOutcome> {
        self.web_research.execute(query)
    }

    /// Upsert into `report_evidence_register`. Returns the stable
    /// `evidence_id`. This is the only entry point that writes to the
    /// register — keep that invariant so lint regressions are auditable.
    pub fn record_into_register(&self, source: &NormalisedSource) -> Result<String> {
        let conn = open_register_conn(&self.root)?;
        let cache = EvidenceCache::new(&conn, &self.run_id);
        cache.upsert(source)
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

/// Best-effort DOI extraction from a URL. Returns the DOI as it appears in
/// the URL, lower-cased, with no surrounding punctuation.
fn extract_doi_from_url(url: &str) -> Option<String> {
    // The DOI grammar is permissive; we look for the canonical
    // `10.\d{4,9}/...` pattern and stop at common URL terminators.
    let needle = url.find("10.")?;
    let tail = &url[needle..];
    let mut end = tail.len();
    for (idx, ch) in tail.char_indices() {
        if matches!(ch, '?' | '#' | ' ' | '\n' | '\t') {
            end = idx;
            break;
        }
    }
    let candidate = &tail[..end];
    // Sanity check: must be `10.<digits>/<rest>`.
    let mut iter = candidate.splitn(2, '/');
    let prefix = iter.next()?;
    let _suffix = iter.next()?;
    if !prefix.starts_with("10.") {
        return None;
    }
    let digits = &prefix[3..];
    if digits.len() < 4 || !digits.chars().all(|c| c.is_ascii_digit()) {
        return None;
    }
    Some(
        candidate
            .trim_end_matches('.')
            .trim_end_matches(',')
            .to_ascii_lowercase(),
    )
}

/// Open a connection to the core DB with the WAL + busy-timeout pragmas the
/// rest of the runtime expects. Schema bootstrap for
/// `report_evidence_register` happens here so resolvers don't have to
/// coordinate with a separate migration step.
pub(crate) fn open_register_conn(root: &Path) -> Result<Connection> {
    let path = paths::core_db(root);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("failed to create runtime dir {}", parent.display()))?;
    }
    let conn = Connection::open(&path)
        .with_context(|| format!("failed to open core db {}", path.display()))?;
    conn.busy_timeout(persistence::sqlite_busy_timeout_duration())
        .context("failed to configure SQLite busy_timeout")?;
    let busy_ms = persistence::sqlite_busy_timeout_millis();
    conn.execute_batch(&format!(
        "PRAGMA journal_mode = WAL;
         PRAGMA busy_timeout = {busy_ms};
         PRAGMA foreign_keys = ON;"
    ))
    .context("failed to set SQLite pragmas for evidence register")?;
    ensure_register_schema(&conn)?;
    Ok(conn)
}

fn ensure_register_schema(conn: &Connection) -> Result<()> {
    conn.execute_batch(
        r#"
        CREATE TABLE IF NOT EXISTS report_evidence_register (
            evidence_id TEXT NOT NULL,
            run_id TEXT NOT NULL,
            kind TEXT NOT NULL,
            canonical_id TEXT NOT NULL,
            title TEXT,
            authors_json TEXT NOT NULL,
            venue TEXT,
            year INTEGER,
            publisher TEXT,
            url_canonical TEXT,
            url_full_text TEXT,
            license TEXT,
            abstract_md TEXT,
            snippet_md TEXT,
            resolver_used TEXT NOT NULL,
            raw_payload_json TEXT NOT NULL,
            citations_count INTEGER NOT NULL DEFAULT 0,
            created_at TEXT NOT NULL,
            updated_at TEXT NOT NULL,
            PRIMARY KEY (run_id, evidence_id)
        );

        CREATE INDEX IF NOT EXISTS idx_report_evidence_register_run
            ON report_evidence_register(run_id, kind);
        CREATE INDEX IF NOT EXISTS idx_report_evidence_register_canonical
            ON report_evidence_register(canonical_id);
        "#,
    )
    .context("failed to ensure report_evidence_register schema")?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn evidence_id_is_stable_for_same_inputs() {
        let a = derive_evidence_id(SourceKind::Doi, "10.1000/abc");
        let b = derive_evidence_id(SourceKind::Doi, "10.1000/ABC");
        assert_eq!(a, b);
        assert!(a.starts_with("ev_"));
        assert_eq!(a.len(), 3 + 16);
    }

    #[test]
    fn evidence_id_differs_across_kinds() {
        let a = derive_evidence_id(SourceKind::Doi, "10.1000/abc");
        let b = derive_evidence_id(SourceKind::Arxiv, "10.1000/abc");
        assert_ne!(a, b);
    }

    #[test]
    fn extract_doi_from_url_handles_doi_org() {
        let doi = extract_doi_from_url("https://doi.org/10.1016/j.foo.2020.01.001");
        assert_eq!(doi.as_deref(), Some("10.1016/j.foo.2020.01.001"));
    }

    #[test]
    fn extract_doi_from_url_handles_publisher_path() {
        let doi = extract_doi_from_url("https://example.com/path/10.1234/xyz?token=1");
        assert_eq!(doi.as_deref(), Some("10.1234/xyz"));
    }

    #[test]
    fn extract_doi_from_url_rejects_bare_url() {
        assert!(extract_doi_from_url("https://example.com/about").is_none());
    }

    #[test]
    fn normalise_doi_strips_prefixes() {
        assert_eq!(normalise_doi("doi:10.1000/ABC."), "10.1000/abc");
        assert_eq!(
            normalise_doi("https://doi.org/10.1000/abc"),
            "10.1000/abc".to_string()
        );
    }
}
