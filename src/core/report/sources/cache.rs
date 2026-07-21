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
    pub verification_status: String,
    pub http_status: Option<i64>,
    pub snapshot_hash: Option<String>,
    pub evidence_eligible: bool,
}

impl EvidenceEntry {
    pub fn is_evidence_eligible(&self) -> bool {
        self.verification_status == "verified"
            && self
                .http_status
                .is_some_and(|status| (200..=299).contains(&status))
            && self
                .snapshot_hash
                .as_deref()
                .is_some_and(super::web_research::is_sha256_receipt)
            && self.has_content_bound_snapshot()
            && self.evidence_eligible
    }

    fn has_content_bound_snapshot(&self) -> bool {
        let Some(snapshot_hash) = self.snapshot_hash.as_deref() else {
            return false;
        };
        let mut source = self.raw_payload.clone();
        let Value::Object(object) = &mut source else {
            return false;
        };
        object.insert(
            "snapshot_hash".to_string(),
            Value::String(snapshot_hash.to_string()),
        );
        super::web_research::is_content_bound_snapshot(&source)
    }
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
    /// the newest immutable register version if present.
    pub fn lookup(&self, kind: SourceKind, canonical_id: &str) -> Result<Option<EvidenceEntry>> {
        let evidence_id = derive_evidence_id(kind, canonical_id);
        let Some(latest_id) = self.latest_evidence_id(&evidence_id)? else {
            return Ok(None);
        };
        self.lookup_by_evidence_id(&latest_id)
    }

    fn lookup_by_evidence_id(&self, evidence_id: &str) -> Result<Option<EvidenceEntry>> {
        let mut stmt = self.conn.prepare(
            "SELECT evidence_id, run_id, kind, canonical_id, title, authors_json, venue, year, \
                    publisher, url_canonical, url_full_text, license, abstract_md, snippet_md, \
                    resolver_used, raw_payload_json, citations_count, created_at, updated_at, \
                    verification_status, http_status, snapshot_hash, evidence_eligible \
             FROM report_evidence_register \
             WHERE run_id = ?1 AND evidence_id = ?2",
        )?;
        let row = stmt
            .query_row(params![self.run_id, evidence_id], row_to_entry)
            .optional()
            .context("query report_evidence_register row")?;
        Ok(row)
    }

    /// Append a new [`NormalisedSource`] version to the register. The first
    /// version keeps the historical base id; later versions use `:vN`.
    /// Refreshing a source always starts unverified, which invalidates the
    /// current cache result without mutating its prior audit record.
    pub fn upsert(&self, source: &NormalisedSource) -> Result<String> {
        let base_id = derive_evidence_id(source.kind, &source.canonical_id);
        let latest_id = self.latest_evidence_id(&base_id)?;
        let evidence_id = self.next_version_id(&base_id, latest_id.as_deref())?;
        let now = Utc::now().to_rfc3339();
        let authors_json = serde_json::to_string(&source.authors)
            .context("serialise authors for evidence register")?;
        let raw_payload_json = serde_json::to_string(&source.raw_payload)
            .context("serialise raw_payload for evidence register")?;

        let (created_at, citations_count) = if let Some(ref current_id) = latest_id {
            self.conn.query_row(
                "SELECT created_at, citations_count FROM report_evidence_register \
                     WHERE run_id = ?1 AND evidence_id = ?2",
                params![self.run_id, current_id],
                |row| Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?)),
            )?
        } else {
            (now.clone(), 0)
        };

        self.conn
            .execute(
                "INSERT INTO report_evidence_register \
                 (evidence_id, run_id, kind, canonical_id, title, authors_json, venue, year, \
                  publisher, url_canonical, url_full_text, license, abstract_md, snippet_md, \
                  full_text_md, full_text_source, full_text_chars, resolver_used, raw_payload_json, \
                  verification_status, http_status, snapshot_hash, evidence_eligible, \
                  citations_count, created_at, updated_at) \
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, NULL, NULL, NULL, ?15, ?16, \
                         'unverified', NULL, NULL, 0, \
                         ?17, ?18, ?19)",
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
                    citations_count.max(0),
                    created_at,
                    now,
                ],
            )
            .context("append report_evidence_register version")?;
        Ok(evidence_id)
    }

    pub fn mark_verified(
        &self,
        evidence_id: &str,
        http_status: i64,
        snapshot_hash: &str,
    ) -> Result<()> {
        if !(200..=299).contains(&http_status) {
            anyhow::bail!("evidence verification requires a 2xx HTTP status");
        }
        if !super::web_research::is_sha256_receipt(snapshot_hash) {
            anyhow::bail!("evidence verification requires a sha256 receipt");
        }
        let base_id = base_evidence_id(evidence_id);
        let current_id = self
            .latest_evidence_id(&base_id)?
            .ok_or_else(|| anyhow::anyhow!("evidence row {evidence_id} was not found"))?;
        let current = self
            .lookup_by_evidence_id(&current_id)?
            .ok_or_else(|| anyhow::anyhow!("evidence row {evidence_id} was not found"))?;
        let mut source = current.raw_payload;
        if let Value::Object(object) = &mut source {
            object.insert(
                "snapshot_hash".to_string(),
                Value::String(snapshot_hash.to_string()),
            );
        } else {
            anyhow::bail!("evidence verification requires snapshot metadata");
        }
        if !super::web_research::is_content_bound_snapshot(&source) {
            anyhow::bail!(
                "evidence verification requires a persisted snapshot matching the receipt"
            );
        }
        let next_id = self.next_version_id(&base_id, Some(&current_id))?;
        self.append_existing_version(
            &current_id,
            &next_id,
            Some("verified"),
            Some(http_status),
            Some(snapshot_hash),
            Some(1),
            0,
        )?;
        Ok(())
    }

    pub fn list_for_run(&self) -> Result<Vec<EvidenceEntry>> {
        let mut stmt = self.conn.prepare(
            "SELECT evidence_id, run_id, kind, canonical_id, title, authors_json, venue, year, \
                    publisher, url_canonical, url_full_text, license, abstract_md, snippet_md, \
                    resolver_used, raw_payload_json, citations_count, created_at, updated_at, \
                    verification_status, http_status, snapshot_hash, evidence_eligible \
             FROM report_evidence_register AS current \
             WHERE run_id = ?1
               AND NOT EXISTS (
                   SELECT 1 FROM report_evidence_register AS newer
                   WHERE newer.run_id = current.run_id
                     AND newer.kind = current.kind
                     AND newer.canonical_id = current.canonical_id
                     AND newer.rowid > current.rowid
               )
             ORDER BY current.rowid ASC",
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
                "SELECT COUNT(*) FROM report_evidence_register AS current
                 WHERE run_id = ?1
                   AND NOT EXISTS (
                       SELECT 1 FROM report_evidence_register AS newer
                       WHERE newer.run_id = current.run_id
                         AND newer.kind = current.kind
                         AND newer.canonical_id = current.canonical_id
                         AND newer.rowid > current.rowid
                   )",
                params![self.run_id],
                |row| row.get(0),
            )
            .context("count report_evidence_register rows")?;
        Ok(n.max(0) as usize)
    }

    /// Append a citation-count version instead of mutating the current row.
    pub fn bump_citations(&self, evidence_id: &str) -> Result<()> {
        let base_id = base_evidence_id(evidence_id);
        let current_id = self
            .latest_evidence_id(&base_id)?
            .ok_or_else(|| anyhow::anyhow!("evidence row {evidence_id} was not found"))?;
        let next_id = self.next_version_id(&base_id, Some(&current_id))?;
        self.append_existing_version(&current_id, &next_id, None, None, None, None, 1)?;
        Ok(())
    }

    fn latest_evidence_id(&self, base_id: &str) -> Result<Option<String>> {
        self.conn
            .query_row(
                "SELECT evidence_id FROM report_evidence_register
                 WHERE run_id = ?1 AND (evidence_id = ?2 OR evidence_id LIKE ?3)
                 ORDER BY rowid DESC LIMIT 1",
                params![self.run_id, base_id, format!("{base_id}:v%")],
                |row| row.get(0),
            )
            .optional()
            .context("find latest report_evidence_register version")
    }

    fn next_version_id(&self, base_id: &str, latest_id: Option<&str>) -> Result<String> {
        let Some(latest_id) = latest_id else {
            return Ok(base_id.to_string());
        };
        let next = latest_id
            .strip_prefix(&format!("{base_id}:v"))
            .and_then(|version| version.parse::<u64>().ok())
            .unwrap_or(1)
            .checked_add(1)
            .ok_or_else(|| anyhow::anyhow!("evidence version overflow for {base_id}"))?;
        Ok(format!("{base_id}:v{next}"))
    }

    fn append_existing_version(
        &self,
        current_id: &str,
        next_id: &str,
        verification_status: Option<&str>,
        http_status: Option<i64>,
        snapshot_hash: Option<&str>,
        evidence_eligible: Option<i64>,
        citations_delta: i64,
    ) -> Result<()> {
        let now = Utc::now().to_rfc3339();
        self.conn
            .execute(
                "INSERT INTO report_evidence_register (
                     evidence_id, run_id, kind, canonical_id, title, authors_json,
                     venue, year, publisher, url_canonical, url_full_text, license,
                     abstract_md, snippet_md, full_text_md, full_text_source, full_text_chars,
                     resolver_used, raw_payload_json, citations_count, created_at, updated_at,
                     verification_status, http_status, snapshot_hash, evidence_eligible
                 )
                 SELECT ?1, run_id, kind, canonical_id, title, authors_json,
                        venue, year, publisher, url_canonical, url_full_text, license,
                        abstract_md, snippet_md, full_text_md, full_text_source, full_text_chars,
                        resolver_used, raw_payload_json, citations_count + ?7, created_at, ?2,
                        COALESCE(?3, verification_status), COALESCE(?4, http_status),
                        COALESCE(?5, snapshot_hash), COALESCE(?6, evidence_eligible)
                 FROM report_evidence_register
                 WHERE run_id = ?8 AND evidence_id = ?9",
                params![
                    next_id,
                    now,
                    verification_status,
                    http_status,
                    snapshot_hash,
                    evidence_eligible,
                    citations_delta,
                    self.run_id,
                    current_id,
                ],
            )
            .context("append report_evidence_register version")?;
        Ok(())
    }
}

fn base_evidence_id(evidence_id: &str) -> String {
    evidence_id
        .split_once(":v")
        .map(|(base, _)| base.to_string())
        .unwrap_or_else(|| evidence_id.to_string())
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
    let verification_status: String = row
        .get::<_, Option<String>>(19)?
        .unwrap_or_else(|| "unverified".to_string());
    let http_status: Option<i64> = row.get(20)?;
    let snapshot_hash: Option<String> = row.get(21)?;
    let evidence_eligible = row.get::<_, Option<i64>>(22)?.unwrap_or(0) != 0;
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
        verification_status,
        http_status,
        snapshot_hash,
        evidence_eligible,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::report::sources::open_register_conn;
    use serde_json::json;
    use sha2::Digest;
    use sha2::Sha256;
    use std::fs;
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

    fn source_with_snapshot() -> (TempDir, NormalisedSource, String) {
        let dir = TempDir::new().unwrap();
        let bytes = b"cache evidence snapshot";
        let path = dir.path().join("snapshot.txt");
        fs::write(&path, bytes).unwrap();
        let digest = Sha256::digest(bytes);
        let receipt = format!("sha256:{digest:x}");
        let mut source = sample_source();
        source.raw_payload = json!({
            "snapshot_path": path.to_string_lossy(),
            "snapshot_id": "snapshot.txt"
        });
        (dir, source, receipt)
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
    fn refresh_appends_a_new_unverified_version() {
        let tmp = fresh_root();
        let conn = open_register_conn(tmp.path()).unwrap();
        let cache = EvidenceCache::new(&conn, "run-append-only");
        let (_snapshot_dir, source, receipt) = source_with_snapshot();
        let first_id = cache.upsert(&source).unwrap();
        cache.mark_verified(&first_id, 200, &receipt).unwrap();
        let refreshed_id = cache.upsert(&source).unwrap();

        assert_ne!(first_id, refreshed_id);
        assert!(refreshed_id.ends_with(":v3"));
        assert!(!cache
            .lookup(SourceKind::Doi, "10.1234/test")
            .unwrap()
            .unwrap()
            .is_evidence_eligible());
        let physical_rows: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM report_evidence_register
                 WHERE run_id = 'run-append-only'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(physical_rows, 3);
        assert_eq!(cache.count().unwrap(), 1);
        assert_eq!(cache.list_for_run().unwrap().len(), 1);
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

    #[test]
    fn evidence_is_ineligible_until_verified_with_document_snapshot() {
        let tmp = fresh_root();
        let conn = open_register_conn(tmp.path()).unwrap();
        let cache = EvidenceCache::new(&conn, "run-verification");
        let (_snapshot_dir, source, receipt) = source_with_snapshot();
        let evidence_id = cache.upsert(&source).unwrap();
        let before = cache
            .lookup(SourceKind::Doi, "10.1234/test")
            .unwrap()
            .unwrap();
        assert!(!before.is_evidence_eligible());

        cache.mark_verified(&evidence_id, 200, &receipt).unwrap();
        let after = cache
            .lookup(SourceKind::Doi, "10.1234/test")
            .unwrap()
            .unwrap();
        assert!(after.is_evidence_eligible());

        cache
            .mark_verified(&evidence_id, 302, "other-hash")
            .unwrap_err();
        cache.mark_verified(&evidence_id, 200, "").unwrap_err();
    }
}
