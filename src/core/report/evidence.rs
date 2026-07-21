//! `report evidence` — register evidence rows for the run.
//!
//! Two paths into the canonical `report_evidence_register`:
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
    let evidence_id = derive_evidence_id(&input.citation_kind, &input.canonical_id);
    let now = store::now_iso();
    let snippet_for_hash = input.snippet_md.as_deref().unwrap_or("");
    let integrity_hash = sha256_hex(snippet_for_hash);
    crate::report::schema::ensure_schema(conn)?;
    conn.execute(
        "INSERT INTO report_evidence_register(
            evidence_id, run_id, kind, canonical_id, title, authors_json, venue,
            year, publisher, url_canonical, url_full_text, abstract_md, snippet_md,
            retrieved_at, resolver_used, license, integrity_hash, created_at,
            updated_at, verification_status, evidence_eligible)
         VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15,?16,?17,?18,?18,
                'unverified', 0)
         ON CONFLICT(evidence_id) DO UPDATE SET
            title = excluded.title,
            authors_json = excluded.authors_json,
            venue = excluded.venue,
            year = excluded.year,
            publisher = excluded.publisher,
            url_canonical = excluded.url_canonical,
            url_full_text = excluded.url_full_text,
            abstract_md = excluded.abstract_md,
            snippet_md = excluded.snippet_md,
            retrieved_at = excluded.retrieved_at,
            resolver_used = excluded.resolver_used,
            license = excluded.license,
            integrity_hash = excluded.integrity_hash,
            updated_at = excluded.updated_at,
            verification_status = 'unverified',
            http_status = NULL,
            snapshot_hash = NULL,
            evidence_eligible = 0",
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
    .context("failed to upsert report_evidence_register")?;
    state_machine::advance_to(conn, run_id, Status::Gathering)?;
    crate::report::runs::set_next_stage(conn, run_id, Some("score"))?;
    load_evidence(conn, run_id, &evidence_id)?.context("evidence row missing after upsert")
}

pub fn list_evidence(conn: &Connection, run_id: &str) -> Result<Vec<EvidenceView>> {
    let mut stmt = conn.prepare(
        "SELECT evidence_id, kind, canonical_id, title, authors_json, venue, year,
                url_canonical, url_full_text, snippet_md, resolver_used,
                verification_status, http_status, snapshot_hash, evidence_eligible
         FROM report_evidence_register WHERE run_id = ?1 ORDER BY created_at ASC",
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
            "SELECT evidence_id, kind, canonical_id, title, authors_json, venue, year,
                    url_canonical, url_full_text, snippet_md, resolver_used,
                    verification_status, http_status, snapshot_hash, evidence_eligible
             FROM report_evidence_register WHERE run_id = ?1 AND evidence_id = ?2",
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

pub fn derive_evidence_id(kind: &str, canonical_id: &str) -> String {
    let basis = format!("{}|{}", kind, canonical_id.trim());
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
pub struct ImportDiagnostic {
    pub source_index: usize,
    pub canonical_url: Option<String>,
    pub reason: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct ImportSummary {
    pub bundle_sources: usize,
    pub registered: usize,
    pub resolved_dois: usize,
    pub unresolved_dois: Vec<String>,
    pub rejected_sources: Vec<ImportDiagnostic>,
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
    import_source_candidates(conn, run_id, &sources_arr, resolve_via_crossref)
}

fn import_source_candidates(
    conn: &Connection,
    run_id: &str,
    sources_arr: &[Value],
    resolve_via_crossref: bool,
) -> Result<ImportSummary> {
    let mut registered = 0usize;
    let mut resolved_dois = 0usize;
    let mut unresolved = Vec::new();
    let mut rejected_sources = Vec::new();
    for (source_index, src) in sources_arr.iter().enumerate() {
        if !sources::web_research::is_evidence_eligible_source(src) {
            rejected_sources.push(ImportDiagnostic {
                source_index,
                canonical_url: src
                    .get("canonical_url")
                    .and_then(Value::as_str)
                    .map(ToOwned::to_owned),
                reason: source_rejection_reason(src),
            });
            continue;
        }

        let url = src
            .get("canonical_url")
            .and_then(Value::as_str)
            .expect("eligible source must have a canonical URL");
        let title = src
            .get("title")
            .and_then(Value::as_str)
            .map(|s| s.to_string());
        let snippet = imported_snippet(src);
        let combined_text = sources::web_research::evidence_text(src);
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
                        landing_url: Some(url.to_string()),
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
                        landing_url: Some(url.to_string()),
                        full_text_url: None,
                        abstract_md: None,
                        snippet_md: snippet.clone(),
                        license: None,
                        resolver: Some("web".to_string()),
                    },
                };
                upsert_verified_import(conn, run_id, &input, src)?;
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
                        landing_url: Some(url.to_string()),
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
                        landing_url: Some(url.to_string()),
                        full_text_url: None,
                        abstract_md: None,
                        snippet_md: snippet.clone(),
                        license: None,
                        resolver: Some("web".to_string()),
                    },
                };
                upsert_verified_import(conn, run_id, &input, src)?;
                registered += 1;
            }
        } else {
            let input = EvidenceInput {
                citation_kind: "url".to_string(),
                canonical_id: url.to_string(),
                title,
                authors: vec![],
                venue: None,
                year: None,
                publisher: None,
                landing_url: Some(url.to_string()),
                full_text_url: None,
                abstract_md: None,
                snippet_md: snippet.clone(),
                license: None,
                resolver: Some("web".to_string()),
            };
            upsert_verified_import(conn, run_id, &input, src)?;
            registered += 1;
        }
    }
    Ok(ImportSummary {
        bundle_sources: sources_arr.len(),
        registered,
        resolved_dois,
        unresolved_dois: unresolved,
        rejected_sources,
    })
}

fn upsert_verified_import(
    conn: &Connection,
    run_id: &str,
    input: &EvidenceInput,
    source: &Value,
) -> Result<()> {
    let evidence = upsert_evidence(conn, run_id, input)?;
    let http_status = source
        .get("http_status")
        .and_then(Value::as_i64)
        .expect("eligible source must have an HTTP status");
    let snapshot_hash = source
        .get("snapshot_hash")
        .and_then(Value::as_str)
        .expect("eligible source must have a snapshot hash");
    conn.execute(
        "UPDATE report_evidence_register
         SET verification_status = 'verified', http_status = ?3,
             snapshot_hash = ?4, evidence_eligible = 1, updated_at = ?5
         WHERE run_id = ?1 AND evidence_id = ?2",
        params![
            run_id,
            evidence.evidence_id,
            http_status,
            snapshot_hash,
            store::now_iso(),
        ],
    )?;
    Ok(())
}

fn imported_snippet(source: &Value) -> Option<String> {
    let read = source.get("read")?;
    if let Some(summary) = read.get("summary").and_then(Value::as_str) {
        return Some(trim_snippet(summary));
    }
    let excerpts = read.get("excerpts").and_then(Value::as_array)?;
    let text = excerpts
        .iter()
        .filter_map(|excerpt| {
            excerpt
                .as_str()
                .or_else(|| excerpt.get("text").and_then(Value::as_str))
        })
        .collect::<Vec<_>>()
        .join("\n");
    (!text.is_empty()).then(|| trim_snippet(&text))
}

fn source_rejection_reason(source: &Value) -> String {
    source
        .get("evidence_rejection_reason")
        .and_then(Value::as_str)
        .filter(|reason| !reason.trim().is_empty())
        .map(ToOwned::to_owned)
        .unwrap_or_else(|| {
            if source.get("metadata_only").and_then(Value::as_bool) == Some(true) {
                "metadata_only".to_string()
            } else {
                "evidence_gate_incomplete".to_string()
            }
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

    #[test]
    fn bundle_import_keeps_rejected_candidates_out_of_register() {
        let dir = tempdir().unwrap();
        let conn = store::open(dir.path()).unwrap();
        let run_id = seed_run(&conn);
        let sources = vec![
            serde_json::json!({
                "canonical_url": "https://metadata.example/10.9999/metadata",
                "source_type": "paper_metadata",
                "source_tier": "metadata",
                "metadata_only": true,
                "verification_status": "verified",
                "transport_verified": true,
                "content_extracted": true,
                "actual_full_text_or_data": true,
                "evidence_relevance_score": 32,
                "http_status": 200,
                "snapshot_hash": format!("sha256:{}", "b".repeat(64)),
                "evidence_eligible": true,
                "read": {"summary": "metadata DOI 10.9999/metadata"}
            }),
            serde_json::json!({
                "canonical_url": "https://publisher.example/article/10.1234/eligible",
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
                "read": {"summary": "The accepted source contains the measured result."}
            }),
        ];

        let summary = import_source_candidates(&conn, &run_id, &sources, false).unwrap();
        assert_eq!(summary.bundle_sources, 2);
        assert_eq!(summary.registered, 1);
        assert_eq!(summary.rejected_sources.len(), 1);
        assert_eq!(summary.rejected_sources[0].reason, "metadata_only");

        let rows = list_evidence(&conn, &run_id).unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].canonical_id, "10.1234/eligible");
        let gate: (String, i64, String, i64) = conn
            .query_row(
                "SELECT verification_status, http_status, snapshot_hash, evidence_eligible
                 FROM report_evidence_register WHERE run_id = ?1",
                params![run_id],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
            )
            .unwrap();
        assert_eq!(
            gate,
            (
                "verified".to_string(),
                200,
                format!("sha256:{}", "a".repeat(64)),
                1
            )
        );
    }
}
