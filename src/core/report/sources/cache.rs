//! Read-through cache over `report_evidence_register`.
//!
//! Avoid re-resolving identifiers within the same run. The schema is owned
//! here for now (Wave 3) — Wave 4 may take it over from `crate::report::schema`
//! once that module lands. Until then, the table is bootstrapped in
//! [`super::open_register_conn`] and read/written exclusively by this cache.

use anyhow::Context;
use anyhow::Result;
use chrono::Utc;
use rusqlite::params;
use rusqlite::Connection;
use rusqlite::OptionalExtension;
use serde::Deserialize;
use serde::Serialize;
use serde_json::Value;

use super::derive_evidence_id;
use super::NormalisedSource;
use super::ResolverName;
use super::SourceKind;

/// One row from `report_evidence_register`. Mirrors [`NormalisedSource`] plus
/// bookkeeping (`evidence_id`, `citations_count`).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EvidenceEntry {
    pub evidence_id: String,
    pub run_id: String,
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
    pub raw_payload: Value,
    pub citations_count: u32,
    pub created_at: String,
    pub updated_at: String,
}

pub struct EvidenceCache<'a> {
    conn: &'a Connection,
    run_id: String,
}

impl<'a> EvidenceCache<'a> {
    pub fn new(conn: &'a Connection, run_id: &str) -> Self {
        Self {
            conn,
            run_id: run_id.to_string(),
        }
    }

    pub fn run_id(&self) -> &str {
        &self.run_id
    }

    /// Cache hit path. Looks up `(run_id, kind, canonical_id)` and returns
    /// the existing row if present.
    pub fn lookup(&self, kind: SourceKind, canonical_id: &str) -> Result<Option<EvidenceEntry>> {
        let evidence_id = derive_evidence_id(kind, canonical_id);
        self.lookup_by_evidence_id(&evidence_id)
    }

    fn lookup_by_evidence_id(&self, evidence_id: &str) -> Result<Option<EvidenceEntry>> {
        let mut stmt = self.conn.prepare(
            "SELECT evidence_id, run_id, kind, canonical_id, title, authors_json, venue, year, \
                    publisher, url_canonical, url_full_text, license, abstract_md, snippet_md, \
                    resolver_used, raw_payload_json, citations_count, created_at, updated_at \
             FROM report_evidence_register \
             WHERE run_id = ?1 AND evidence_id = ?2",
        )?;
        let row = stmt
            .query_row(params![self.run_id, evidence_id], row_to_entry)
            .optional()
            .context("query report_evidence_register row")?;
        Ok(row)
    }

    /// Upsert a [`NormalisedSource`] into the register. Returns the stable
    /// evidence_id. Existing rows are refreshed in place; `created_at` is
    /// preserved on update, `updated_at` always advances.
    pub fn upsert(&self, source: &NormalisedSource) -> Result<String> {
        let evidence_id = derive_evidence_id(source.kind, &source.canonical_id);
        let now = Utc::now().to_rfc3339();
        let authors_json = serde_json::to_string(&source.authors)
            .context("serialise authors for evidence register")?;
        let raw_payload_json = serde_json::to_string(&source.raw_payload)
            .context("serialise raw_payload for evidence register")?;

        // Preserve created_at if a prior row exists; otherwise use `now`.
        let existing_created_at: Option<String> = self
            .conn
            .query_row(
                "SELECT created_at FROM report_evidence_register \
                 WHERE run_id = ?1 AND evidence_id = ?2",
                params![self.run_id, evidence_id],
                |row| row.get::<_, String>(0),
            )
            .optional()?;
        let created_at = existing_created_at.unwrap_or_else(|| now.clone());

        self.conn
            .execute(
                "INSERT OR REPLACE INTO report_evidence_register \
                 (evidence_id, run_id, kind, canonical_id, title, authors_json, venue, year, \
                  publisher, url_canonical, url_full_text, license, abstract_md, snippet_md, \
                  resolver_used, raw_payload_json, citations_count, created_at, updated_at) \
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, \
                         COALESCE((SELECT citations_count FROM report_evidence_register \
                                   WHERE run_id = ?2 AND evidence_id = ?1), 0), \
                         ?17, ?18)",
                params![
                    evidence_id,
                    self.run_id,
                    source.kind.as_str(),
                    source.canonical_id,
                    source.title,
                    authors_json,
                    source.venue,
                    source.year,
                    source.publisher,
                    source.url_canonical,
                    source.url_full_text,
                    source.license,
                    source.abstract_md,
                    source.snippet_md,
                    source.resolver_used.as_str(),
                    raw_payload_json,
                    created_at,
                    now,
                ],
            )
            .context("upsert into report_evidence_register")?;
        Ok(evidence_id)
    }

    pub fn list_for_run(&self) -> Result<Vec<EvidenceEntry>> {
        let mut stmt = self.conn.prepare(
            "SELECT evidence_id, run_id, kind, canonical_id, title, authors_json, venue, year, \
                    publisher, url_canonical, url_full_text, license, abstract_md, snippet_md, \
                    resolver_used, raw_payload_json, citations_count, created_at, updated_at \
             FROM report_evidence_register \
             WHERE run_id = ?1 \
             ORDER BY created_at ASC",
        )?;
        let rows = stmt
            .query_map(params![self.run_id], row_to_entry)?
            .collect::<std::result::Result<Vec<_>, _>>()
            .context("collect report_evidence_register rows")?;
        Ok(rows)
    }

    pub fn count(&self) -> Result<usize> {
        let n: i64 = self
            .conn
            .query_row(
                "SELECT COUNT(*) FROM report_evidence_register WHERE run_id = ?1",
                params![self.run_id],
                |row| row.get(0),
            )
            .context("count report_evidence_register rows")?;
        Ok(n.max(0) as usize)
    }

    /// Bump the `citations_count` for an evidence row. Used by the claims
    /// stage when a claim references a row, so LINT-EVIDENCE-CONCENTRATION
    /// can flag over-cited sources.
    pub fn bump_citations(&self, evidence_id: &str) -> Result<()> {
        self.conn
            .execute(
                "UPDATE report_evidence_register \
                 SET citations_count = citations_count + 1, \
                     updated_at = ?3 \
                 WHERE run_id = ?1 AND evidence_id = ?2",
                params![self.run_id, evidence_id, Utc::now().to_rfc3339()],
            )
            .context("bump citations_count in report_evidence_register")?;
        Ok(())
    }
}

fn row_to_entry(row: &rusqlite::Row<'_>) -> rusqlite::Result<EvidenceEntry> {
    let evidence_id: String = row.get(0)?;
    let run_id: String = row.get(1)?;
    let kind_str: String = row.get(2)?;
    let canonical_id: String = row.get(3)?;
    let title: Option<String> = row.get(4)?;
    let authors_json: String = row.get(5)?;
    let venue: Option<String> = row.get(6)?;
    let year: Option<i64> = row.get(7)?;
    let publisher: Option<String> = row.get(8)?;
    let url_canonical: Option<String> = row.get(9)?;
    let url_full_text: Option<String> = row.get(10)?;
    let license: Option<String> = row.get(11)?;
    let abstract_md: Option<String> = row.get(12)?;
    let snippet_md: Option<String> = row.get(13)?;
    let resolver_str: String = row.get(14)?;
    let raw_payload_json: String = row.get(15)?;
    let citations_count: i64 = row.get(16)?;
    let created_at: String = row.get(17)?;
    let updated_at: String = row.get(18)?;
    let authors: Vec<String> = serde_json::from_str(&authors_json).unwrap_or_default();
    let raw_payload: Value = serde_json::from_str(&raw_payload_json).unwrap_or(Value::Null);
    Ok(EvidenceEntry {
        evidence_id,
        run_id,
        kind: SourceKind::from_str(&kind_str).unwrap_or(SourceKind::Url),
        canonical_id,
        title,
        authors,
        venue,
        year: year.map(|y| y as i32),
        publisher,
        url_canonical,
        url_full_text,
        license,
        abstract_md,
        snippet_md,
        resolver_used: ResolverName::from_str(&resolver_str).unwrap_or(ResolverName::Manual),
        raw_payload,
        citations_count: citations_count.max(0) as u32,
        created_at,
        updated_at,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::report::sources::open_register_conn;
    use serde_json::json;
    use tempfile::TempDir;

    fn fresh_root() -> TempDir {
        TempDir::new().expect("tempdir")
    }

    fn sample_source() -> NormalisedSource {
        NormalisedSource {
            kind: SourceKind::Doi,
            canonical_id: "10.1234/test".to_string(),
            title: Some("A Title".to_string()),
            authors: vec!["A. Author".to_string(), "B. Author".to_string()],
            venue: Some("Journal".to_string()),
            year: Some(2024),
            publisher: Some("Pub".to_string()),
            url_canonical: Some("https://doi.org/10.1234/test".to_string()),
            url_full_text: None,
            license: None,
            abstract_md: Some("Abstract.".to_string()),
            snippet_md: None,
            resolver_used: ResolverName::Crossref,
            raw_payload: json!({"x": 1}),
        }
    }

    #[test]
    fn upsert_then_lookup_round_trips() {
        let tmp = fresh_root();
        let conn = open_register_conn(tmp.path()).unwrap();
        let cache = EvidenceCache::new(&conn, "run-1");
        let evidence_id = cache.upsert(&sample_source()).unwrap();
        assert!(evidence_id.starts_with("ev_"));
        let hit = cache.lookup(SourceKind::Doi, "10.1234/test").unwrap();
        assert!(hit.is_some());
        let entry = hit.unwrap();
        assert_eq!(entry.evidence_id, evidence_id);
        assert_eq!(entry.title.as_deref(), Some("A Title"));
        assert_eq!(entry.authors.len(), 2);
        assert_eq!(entry.year, Some(2024));
    }

    #[test]
    fn upsert_preserves_created_at() {
        let tmp = fresh_root();
        let conn = open_register_conn(tmp.path()).unwrap();
        let cache = EvidenceCache::new(&conn, "run-2");
        let _ = cache.upsert(&sample_source()).unwrap();
        let first = cache
            .lookup(SourceKind::Doi, "10.1234/test")
            .unwrap()
            .unwrap();
        std::thread::sleep(std::time::Duration::from_millis(20));
        let _ = cache.upsert(&sample_source()).unwrap();
        let second = cache
            .lookup(SourceKind::Doi, "10.1234/test")
            .unwrap()
            .unwrap();
        assert_eq!(first.created_at, second.created_at);
        assert!(second.updated_at >= first.updated_at);
    }

    #[test]
    fn count_and_list_match() {
        let tmp = fresh_root();
        let conn = open_register_conn(tmp.path()).unwrap();
        let cache = EvidenceCache::new(&conn, "run-3");
        let mut s = sample_source();
        cache.upsert(&s).unwrap();
        s.canonical_id = "10.9999/two".to_string();
        cache.upsert(&s).unwrap();
        assert_eq!(cache.count().unwrap(), 2);
        assert_eq!(cache.list_for_run().unwrap().len(), 2);
    }

    #[test]
    fn bump_citations_increments() {
        let tmp = fresh_root();
        let conn = open_register_conn(tmp.path()).unwrap();
        let cache = EvidenceCache::new(&conn, "run-4");
        let evidence_id = cache.upsert(&sample_source()).unwrap();
        cache.bump_citations(&evidence_id).unwrap();
        cache.bump_citations(&evidence_id).unwrap();
        let entry = cache
            .lookup(SourceKind::Doi, "10.1234/test")
            .unwrap()
            .unwrap();
        assert_eq!(entry.citations_count, 2);
    }
}
