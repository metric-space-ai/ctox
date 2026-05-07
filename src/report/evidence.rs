//! `report evidence` — register evidence rows for the run.
//!
//! Two paths into `report_evidence`:
//! 1. Direct registration from a structured payload (manual or skill-driven).
//! 2. Bulk import from a `tools/web-stack` deep_research evidence bundle,
//!    optionally enriched via Crossref/OpenAlex/arXiv resolvers.
//!
//! Both paths share the same idempotency rule: a citation is identified by
//! `evidence_id = sha256(canonical_id)`. Re-importing the same DOI updates
//! the row in place.

use anyhow::bail;
use anyhow::Context;
use anyhow::Result;
use rusqlite::params;
use rusqlite::Connection;
use rusqlite::OptionalExtension;
use serde::Deserialize;
use serde::Serialize;
use serde_json::Value;
use sha2::Digest;
use sha2::Sha256;
use std::path::Path;

use crate::report::sources;
use crate::report::state_machine::{self, Status};
use crate::report::store;

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct EvidenceInput {
    pub citation_kind: String, // doi | arxiv | url | book | standard | assumption
    pub canonical_id: String,
    #[serde(default)]
    pub title: Option<String>,
    #[serde(default)]
    pub authors: Vec<String>,
    #[serde(default)]
    pub venue: Option<String>,
    #[serde(default)]
    pub year: Option<i64>,
    #[serde(default)]
    pub publisher: Option<String>,
    #[serde(default)]
    pub landing_url: Option<String>,
    #[serde(default)]
    pub full_text_url: Option<String>,
    #[serde(default)]
    pub abstract_md: Option<String>,
    #[serde(default)]
    pub snippet_md: Option<String>,
    #[serde(default)]
    pub license: Option<String>,
    #[serde(default)]
    pub resolver: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct EvidenceView {
    pub evidence_id: String,
    pub citation_kind: String,
    pub canonical_id: String,
    pub title: Option<String>,
    pub authors: Vec<String>,
    pub venue: Option<String>,
    pub year: Option<i64>,
    pub landing_url: Option<String>,
    pub full_text_url: Option<String>,
    pub snippet_md: Option<String>,
    pub resolver: String,
}

const VALID_KINDS: [&str; 6] = ["doi", "arxiv", "url", "book", "standard", "assumption"];

pub fn upsert_evidence(
    conn: &Connection,
    run_id: &str,
    input: &EvidenceInput,
) -> Result<EvidenceView> {
    state_machine::require_at_least(conn, run_id, Status::Scoped)?;
    if !VALID_KINDS.contains(&input.citation_kind.as_str()) {
        bail!(
            "citation_kind must be one of {:?}, got '{}'",
            VALID_KINDS,
            input.citation_kind
        );
    }
    if input.canonical_id.trim().is_empty() {
        bail!("canonical_id must be non-empty");
    }
    let evidence_id = derive_evidence_id(run_id, &input.citation_kind, &input.canonical_id);
    let now = store::now_iso();
    let snippet_for_hash = input.snippet_md.as_deref().unwrap_or("");
    let integrity_hash = sha256_hex(snippet_for_hash);
    conn.execute(
        "INSERT INTO report_evidence(evidence_id, run_id, citation_kind, canonical_id, title,
            authors_json, venue, year, publisher, landing_url, full_text_url, abstract_md,
            snippet_md, retrieved_at, resolver, license, integrity_hash, created_at)
         VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15,?16,?17,?18)
         ON CONFLICT(evidence_id) DO UPDATE SET
            title = excluded.title,
            authors_json = excluded.authors_json,
            venue = excluded.venue,
            year = excluded.year,
            publisher = excluded.publisher,
            landing_url = excluded.landing_url,
            full_text_url = excluded.full_text_url,
            abstract_md = excluded.abstract_md,
            snippet_md = excluded.snippet_md,
            retrieved_at = excluded.retrieved_at,
            resolver = excluded.resolver,
            license = excluded.license,
            integrity_hash = excluded.integrity_hash",
        params![
            evidence_id,
            run_id,
            input.citation_kind,
            input.canonical_id.trim(),
            input.title.as_deref(),
            serde_json::to_string(&input.authors)?,
            input.venue.as_deref(),
            input.year,
            input.publisher.as_deref(),
            input.landing_url.as_deref(),
            input.full_text_url.as_deref(),
            input.abstract_md.as_deref(),
            input.snippet_md.as_deref(),
            now.clone(),
            input.resolver.as_deref().unwrap_or("manual"),
            input.license.as_deref(),
            integrity_hash,
            now,
        ],
    )
    .context("failed to upsert report_evidence")?;
    state_machine::advance_to(conn, run_id, Status::Gathering)?;
    crate::report::runs::set_next_stage(conn, run_id, Some("score"))?;
    load_evidence(conn, run_id, &evidence_id)?.context("evidence row missing after upsert")
}

pub fn list_evidence(conn: &Connection, run_id: &str) -> Result<Vec<EvidenceView>> {
    let mut stmt = conn.prepare(
        "SELECT evidence_id, citation_kind, canonical_id, title, authors_json, venue, year,
                landing_url, full_text_url, snippet_md, resolver
         FROM report_evidence WHERE run_id = ?1 ORDER BY created_at ASC",
    )?;
    let rows = stmt.query_map(params![run_id], |row| {
        let authors_json: String = row.get(4)?;
        Ok(EvidenceView {
            evidence_id: row.get(0)?,
            citation_kind: row.get(1)?,
            canonical_id: row.get(2)?,
            title: row.get(3)?,
            authors: serde_json::from_str(&authors_json).unwrap_or_default(),
            venue: row.get(5)?,
            year: row.get(6)?,
            landing_url: row.get(7)?,
            full_text_url: row.get(8)?,
            snippet_md: row.get(9)?,
            resolver: row.get::<_, Option<String>>(10)?.unwrap_or_default(),
        })
    })?;
    Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
}

pub fn load_evidence(
    conn: &Connection,
    run_id: &str,
    evidence_id: &str,
) -> Result<Option<EvidenceView>> {
    let row: Option<EvidenceView> = conn
        .query_row(
            "SELECT evidence_id, citation_kind, canonical_id, title, authors_json, venue, year,
                    landing_url, full_text_url, snippet_md, resolver
             FROM report_evidence WHERE run_id = ?1 AND evidence_id = ?2",
            params![run_id, evidence_id],
            |row| {
                let authors_json: String = row.get(4)?;
                Ok(EvidenceView {
                    evidence_id: row.get(0)?,
                    citation_kind: row.get(1)?,
                    canonical_id: row.get(2)?,
                    title: row.get(3)?,
                    authors: serde_json::from_str(&authors_json).unwrap_or_default(),
                    venue: row.get(5)?,
                    year: row.get(6)?,
                    landing_url: row.get(7)?,
                    full_text_url: row.get(8)?,
                    snippet_md: row.get(9)?,
                    resolver: row.get::<_, Option<String>>(10)?.unwrap_or_default(),
                })
            },
        )
        .optional()?;
    Ok(row)
}

pub fn derive_evidence_id(run_id: &str, kind: &str, canonical_id: &str) -> String {
    let basis = format!("{}|{}|{}", run_id, kind, canonical_id.trim());
    format!("ev_{}", &sha256_hex(&basis)[..16])
}

fn sha256_hex(input: &str) -> String {
    let digest = Sha256::digest(input.as_bytes());
    hex::encode_lower(digest)
}

mod hex {
    /// Encode bytes to lowercase hex without pulling in a `hex` crate.
    pub fn encode_lower(bytes: impl AsRef<[u8]>) -> String {
        let b = bytes.as_ref();
        let mut s = String::with_capacity(b.len() * 2);
        for byte in b {
            s.push(char::from_digit(((byte >> 4) & 0xF) as u32, 16).unwrap());
            s.push(char::from_digit((byte & 0xF) as u32, 16).unwrap());
        }
        s
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct ImportSummary {
    pub bundle_sources: usize,
    pub registered: usize,
    pub resolved_dois: usize,
    pub unresolved_dois: Vec<String>,
}

pub fn import_from_deep_research_bundle(
    root: &Path,
    conn: &Connection,
    run_id: &str,
    query: &str,
    focus: Option<&str>,
    depth: &str,
    max_sources: usize,
    resolve_via_crossref: bool,
) -> Result<ImportSummary> {
    let bundle = sources::web::deep_research(root, query, focus, depth, max_sources)?;
    let sources_arr = bundle
        .get("sources")
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default();
    let mut registered = 0usize;
    let mut resolved_dois = 0usize;
    let mut unresolved = Vec::new();
    for src in &sources_arr {
        let url = src
            .get("url")
            .and_then(Value::as_str)
            .or_else(|| src.get("landing_url").and_then(Value::as_str));
        let title = src
            .get("title")
            .and_then(Value::as_str)
            .map(|s| s.to_string());
        let snippet = src
            .get("snippet")
            .or_else(|| src.get("read").and_then(|r| r.get("summary")))
            .and_then(Value::as_str)
            .map(|s| trim_snippet(s));
        let combined_text = format!(
            "{}\n{}\n{}",
            url.unwrap_or(""),
            title.as_deref().unwrap_or(""),
            snippet.as_deref().unwrap_or("")
        );
        let dois = sources::scholarly::extract_dois_from_text(&combined_text);
        let arxivs = sources::scholarly::extract_arxiv_from_text(&combined_text);
        if !dois.is_empty() {
            for doi in dois {
                let resolved = if resolve_via_crossref {
                    match sources::scholarly::resolve_doi_via_crossref(&doi) {
                        Ok(Some(c)) => {
                            resolved_dois += 1;
                            Some(c)
                        }
                        Ok(None) => match sources::scholarly::resolve_doi_via_openalex(&doi) {
                            Ok(Some(c)) => {
                                resolved_dois += 1;
                                Some(c)
                            }
                            _ => {
                                unresolved.push(doi.clone());
                                None
                            }
                        },
                        Err(_) => {
                            unresolved.push(doi.clone());
                            None
                        }
                    }
                } else {
                    None
                };
                let input = match resolved {
                    Some(c) => EvidenceInput {
                        citation_kind: c.citation_kind,
                        canonical_id: c.canonical_id,
                        title: c.title,
                        authors: c.authors,
                        venue: c.venue,
                        year: c.year,
                        publisher: c.publisher,
                        landing_url: c.landing_url,
                        full_text_url: c.full_text_url,
                        abstract_md: c.abstract_md,
                        snippet_md: snippet.clone(),
                        license: c.license,
                        resolver: Some(c.resolver),
                    },
                    None => EvidenceInput {
                        citation_kind: "doi".to_string(),
                        canonical_id: doi.clone(),
                        title: title.clone(),
                        authors: vec![],
                        venue: None,
                        year: None,
                        publisher: None,
                        landing_url: url.map(|s| s.to_string()),
                        full_text_url: None,
                        abstract_md: None,
                        snippet_md: snippet.clone(),
                        license: None,
                        resolver: Some("web".to_string()),
                    },
                };
                upsert_evidence(conn, run_id, &input)?;
                registered += 1;
            }
        } else if !arxivs.is_empty() {
            for ax in arxivs {
                let resolved = sources::scholarly::resolve_arxiv(&ax).ok().flatten();
                let input = match resolved {
                    Some(c) => EvidenceInput {
                        citation_kind: c.citation_kind,
                        canonical_id: c.canonical_id,
                        title: c.title,
                        authors: c.authors,
                        venue: c.venue,
                        year: c.year,
                        publisher: c.publisher,
                        landing_url: c.landing_url,
                        full_text_url: c.full_text_url,
                        abstract_md: c.abstract_md,
                        snippet_md: snippet.clone(),
                        license: c.license,
                        resolver: Some(c.resolver),
                    },
                    None => EvidenceInput {
                        citation_kind: "arxiv".to_string(),
                        canonical_id: ax,
                        title: title.clone(),
                        authors: vec![],
                        venue: Some("arXiv".to_string()),
                        year: None,
                        publisher: None,
                        landing_url: url.map(|s| s.to_string()),
                        full_text_url: None,
                        abstract_md: None,
                        snippet_md: snippet.clone(),
                        license: None,
                        resolver: Some("web".to_string()),
                    },
                };
                upsert_evidence(conn, run_id, &input)?;
                registered += 1;
            }
        } else if let Some(u) = url {
            let input = EvidenceInput {
                citation_kind: "url".to_string(),
                canonical_id: u.to_string(),
                title,
                authors: vec![],
                venue: None,
                year: None,
                publisher: None,
                landing_url: Some(u.to_string()),
                full_text_url: None,
                abstract_md: None,
                snippet_md: snippet.clone(),
                license: None,
                resolver: Some("web".to_string()),
            };
            upsert_evidence(conn, run_id, &input)?;
            registered += 1;
        }
    }
    Ok(ImportSummary {
        bundle_sources: sources_arr.len(),
        registered,
        resolved_dois,
        unresolved_dois: unresolved,
    })
}

fn trim_snippet(s: &str) -> String {
    const MAX: usize = 4096;
    if s.chars().count() <= MAX {
        s.to_string()
    } else {
        s.chars().take(MAX).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::report::store;
    use tempfile::tempdir;

    fn seed_run(conn: &Connection) -> String {
        let now = store::now_iso();
        let run_id = store::new_id("run");
        conn.execute(
            "INSERT INTO report_runs(run_id, preset, blueprint_version, topic, language,
                status, created_at, updated_at)
             VALUES(?1,'feasibility','1','t','en','scoped',?2,?2)",
            params![run_id, now],
        )
        .unwrap();
        run_id
    }

    #[test]
    fn upsert_is_idempotent_on_canonical_id() {
        let dir = tempdir().unwrap();
        let conn = store::open(dir.path()).unwrap();
        let run_id = seed_run(&conn);
        let input = EvidenceInput {
            citation_kind: "doi".to_string(),
            canonical_id: "10.1016/x.y".to_string(),
            title: Some("First".to_string()),
            authors: vec![],
            venue: None,
            year: None,
            publisher: None,
            landing_url: None,
            full_text_url: None,
            abstract_md: None,
            snippet_md: Some("first".to_string()),
            license: None,
            resolver: Some("manual".to_string()),
        };
        let a = upsert_evidence(&conn, &run_id, &input).unwrap();
        let mut input2 = input.clone();
        input2.title = Some("Second".to_string());
        let b = upsert_evidence(&conn, &run_id, &input2).unwrap();
        assert_eq!(a.evidence_id, b.evidence_id);
        let list = list_evidence(&conn, &run_id).unwrap();
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].title.as_deref(), Some("Second"));
    }

    #[test]
    fn invalid_kind_rejected() {
        let dir = tempdir().unwrap();
        let conn = store::open(dir.path()).unwrap();
        let run_id = seed_run(&conn);
        let bad = EvidenceInput {
            citation_kind: "podcast".to_string(),
            canonical_id: "x".to_string(),
            title: None,
            authors: vec![],
            venue: None,
            year: None,
            publisher: None,
            landing_url: None,
            full_text_url: None,
            abstract_md: None,
            snippet_md: None,
            license: None,
            resolver: None,
        };
        assert!(upsert_evidence(&conn, &run_id, &bad).is_err());
    }
}
