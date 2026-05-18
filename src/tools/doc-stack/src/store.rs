use anyhow::{Context, Result};
use chrono::Utc;
use rusqlite::{params, Connection, OptionalExtension};
use serde::Serialize;
use std::collections::{HashMap, HashSet};
use std::path::Path;

use crate::parse::{ParsedChunk, ParsedDocument};

#[derive(Debug, Clone, Serialize)]
pub struct CorpusRootRecord {
    pub root_path: String,
    pub label: Option<String>,
    pub enabled: bool,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct SearchCandidate {
    pub chunk_id: String,
    pub path: String,
    pub title: String,
    pub parser_kind: String,
    pub page_number: Option<usize>,
    pub start_line: Option<usize>,
    pub end_line: Option<usize>,
    pub section_title: Option<String>,
    pub text: String,
    pub lexical_score: Option<f64>,
    pub semantic_score: Option<f64>,
    pub combined_score: f64,
}

#[derive(Debug, Clone)]
pub struct DocumentFreshness {
    pub is_fresh: bool,
}

#[derive(Debug, Clone)]
pub struct UpsertStats {
    pub chunks_written: usize,
}

#[derive(Debug, Clone)]
pub struct IndexedChunk {
    pub chunk: ParsedChunk,
    pub embedding: Option<Vec<f64>>,
}

#[derive(Debug, Clone)]
pub struct IndexedDocument {
    pub path: String,
    pub title: String,
    pub parser_kind: String,
    pub size_bytes: u64,
    pub modified_at: i64,
    pub page_count: Option<usize>,
    pub embedding_model: Option<String>,
    pub chunks: Vec<IndexedChunk>,
}

pub struct DocStore {
    conn: Connection,
}

impl DocStore {
    pub fn open(root: &Path) -> Result<Self> {
        let runtime_dir = root.join("runtime/documents");
        std::fs::create_dir_all(&runtime_dir)
            .with_context(|| format!("failed to create {}", runtime_dir.display()))?;
        let db_path = runtime_dir.join("ctox_doc.db");
        let conn = Connection::open(&db_path)
            .with_context(|| format!("failed to open {}", db_path.display()))?;
        conn.execute_batch(
            r#"
            PRAGMA journal_mode = WAL;
            PRAGMA foreign_keys = ON;

            CREATE TABLE IF NOT EXISTS corpus_roots (
                root_path TEXT PRIMARY KEY,
                label TEXT,
                enabled INTEGER NOT NULL DEFAULT 1,
                created_at TEXT NOT NULL,
                updated_at TEXT NOT NULL
            );

            CREATE TABLE IF NOT EXISTS documents (
                doc_id TEXT PRIMARY KEY,
                path TEXT NOT NULL UNIQUE,
                root_path TEXT,
                title TEXT NOT NULL,
                parser_kind TEXT NOT NULL,
                size_bytes INTEGER NOT NULL,
                modified_at INTEGER NOT NULL,
                content_hash TEXT NOT NULL,
                page_count INTEGER,
                chunk_count INTEGER NOT NULL,
                embedding_model TEXT,
                indexed_at TEXT NOT NULL
            );

            CREATE TABLE IF NOT EXISTS document_chunks (
                chunk_id TEXT PRIMARY KEY,
                doc_id TEXT NOT NULL,
                ordinal INTEGER NOT NULL,
                page_number INTEGER,
                start_line INTEGER,
                end_line INTEGER,
                section_title TEXT,
                text TEXT NOT NULL,
                token_estimate INTEGER NOT NULL,
                embedding_json TEXT,
                FOREIGN KEY(doc_id) REFERENCES documents(doc_id) ON DELETE CASCADE,
                UNIQUE(doc_id, ordinal)
            );

            CREATE VIRTUAL TABLE IF NOT EXISTS document_chunks_fts USING fts5(
                text,
                section_title,
                path UNINDEXED,
                tokenize='unicode61'
            );
            "#,
        )?;
        Ok(Self { conn })
    }

    pub fn upsert_root(&self, root_path: &Path, label: Option<&str>) -> Result<CorpusRootRecord> {
        let path = root_path.display().to_string();
        let now = iso_now();
        self.conn.execute(
            r#"
            INSERT INTO corpus_roots (root_path, label, enabled, created_at, updated_at)
            VALUES (?1, ?2, 1, ?3, ?3)
            ON CONFLICT(root_path) DO UPDATE SET
                label = COALESCE(excluded.label, corpus_roots.label),
                enabled = 1,
                updated_at = excluded.updated_at
            "#,
            params![path, label, now],
        )?;
        self.list_roots()?
            .into_iter()
            .find(|record| record.root_path == root_path.display().to_string())
            .context("failed to reload stored root")
    }

    pub fn list_roots(&self) -> Result<Vec<CorpusRootRecord>> {
        let mut stmt = self.conn.prepare(
            "SELECT root_path, label, enabled, created_at, updated_at FROM corpus_roots ORDER BY root_path ASC",
        )?;
        let rows = stmt.query_map([], |row| {
            Ok(CorpusRootRecord {
                root_path: row.get(0)?,
                label: row.get(1)?,
                enabled: row.get::<_, i64>(2)? != 0,
                created_at: row.get(3)?,
                updated_at: row.get(4)?,
            })
        })?;
        Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
    }

    pub fn root_paths(&self) -> Result<Vec<String>> {
        Ok(self
            .list_roots()?
            .into_iter()
            .filter(|record| record.enabled)
            .map(|record| record.root_path)
            .collect())
    }

    pub fn indexed_document_count(&self) -> Result<usize> {
        let count: i64 = self
            .conn
            .query_row("SELECT COUNT(*) FROM documents", [], |row| row.get(0))?;
        Ok(count.max(0) as usize)
    }

    pub fn load_document_by_path(&self, path: &str) -> Result<Option<IndexedDocument>> {
        let header = self
            .conn
            .query_row(
                r#"
                SELECT
                    doc_id,
                    path,
                    title,
                    parser_kind,
                    size_bytes,
                    modified_at,
                    page_count,
                    embedding_model
                FROM documents
                WHERE path = ?1
                "#,
                [path],
                |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, String>(2)?,
                        row.get::<_, String>(3)?,
                        row.get::<_, i64>(4)?,
                        row.get::<_, i64>(5)?,
                        row.get::<_, Option<i64>>(6)?,
                        row.get::<_, Option<String>>(7)?,
                    ))
                },
            )
            .optional()?;
        let Some((
            doc_id,
            path,
            title,
            parser_kind,
            size_bytes,
            modified_at,
            page_count,
            embedding_model,
        )) = header
        else {
            return Ok(None);
        };

        let mut stmt = self.conn.prepare(
            r#"
            SELECT
                ordinal,
                page_number,
                start_line,
                end_line,
                section_title,
                text,
                embedding_json
            FROM document_chunks
            WHERE doc_id = ?1
            ORDER BY ordinal ASC
            "#,
        )?;
        let rows = stmt.query_map([doc_id], |row| {
            let ordinal = row.get::<_, i64>(0)? as usize;
            Ok((
                ParsedChunk {
                    ordinal,
                    page_number: row.get::<_, Option<i64>>(1)?.map(|value| value as usize),
                    start_line: row.get::<_, Option<i64>>(2)?.map(|value| value as usize),
                    end_line: row.get::<_, Option<i64>>(3)?.map(|value| value as usize),
                    section_title: row.get(4)?,
                    text: row.get(5)?,
                },
                row.get::<_, Option<String>>(6)?,
            ))
        })?;
        let chunks = rows
            .collect::<rusqlite::Result<Vec<_>>>()?
            .into_iter()
            .map(|(chunk, embedding_json)| {
                Ok(IndexedChunk {
                    chunk,
                    embedding: embedding_json
                        .as_deref()
                        .map(parse_embedding_json)
                        .transpose()?,
                })
            })
            .collect::<Result<Vec<_>>>()?;

        Ok(Some(IndexedDocument {
            path,
            title,
            parser_kind,
            size_bytes: size_bytes.max(0) as u64,
            modified_at,
            page_count: page_count.map(|value| value as usize),
            embedding_model,
            chunks,
        }))
    }

    pub fn document_freshness(
        &self,
        path: &str,
        size_bytes: u64,
        modified_at: i64,
        embedding_model: Option<&str>,
    ) -> Result<Option<DocumentFreshness>> {
        self.conn
            .query_row(
                "SELECT size_bytes, modified_at, embedding_model FROM documents WHERE path = ?1",
                [path],
                |row| {
                    let stored_size: i64 = row.get(0)?;
                    let stored_modified: i64 = row.get(1)?;
                    let stored_model: Option<String> = row.get(2)?;
                    let models_match = match (stored_model.as_deref(), embedding_model) {
                        (None, None) => true,
                        (Some(left), Some(right)) => left == right,
                        _ => false,
                    };
                    Ok(DocumentFreshness {
                        is_fresh: stored_size == size_bytes as i64
                            && stored_modified == modified_at
                            && models_match,
                    })
                },
            )
            .optional()
            .map_err(Into::into)
    }

    pub fn upsert_document(
        &mut self,
        root_path: Option<&Path>,
        parsed: &ParsedDocument,
        embedding_model: Option<&str>,
        embeddings: Option<&[Vec<f64>]>,
    ) -> Result<UpsertStats> {
        let doc_id = document_id(&parsed.path);
        let tx = self.conn.transaction()?;
        delete_document_rows(&tx, &doc_id)?;

        let indexed_at = iso_now();
        tx.execute(
            r#"
            INSERT INTO documents (
                doc_id, path, root_path, title, parser_kind, size_bytes, modified_at,
                content_hash, page_count, chunk_count, embedding_model, indexed_at
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)
            "#,
            params![
                doc_id,
                parsed.path,
                root_path.map(|value| value.display().to_string()),
                parsed.title,
                parsed.parser_kind,
                parsed.size_bytes as i64,
                parsed.modified_at,
                parsed.content_hash,
                parsed.page_count.map(|value| value as i64),
                parsed.chunks.len() as i64,
                embedding_model,
                indexed_at,
            ],
        )?;

        for chunk in &parsed.chunks {
            let chunk_id = format!("{doc_id}:{}", chunk.ordinal);
            tx.execute(
                r#"
                INSERT INTO document_chunks (
                    chunk_id, doc_id, ordinal, page_number, start_line, end_line,
                    section_title, text, token_estimate, embedding_json
                ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)
                "#,
                params![
                    chunk_id,
                    doc_id,
                    chunk.ordinal as i64,
                    chunk.page_number.map(|value| value as i64),
                    chunk.start_line.map(|value| value as i64),
                    chunk.end_line.map(|value| value as i64),
                    chunk.section_title,
                    chunk.text,
                    estimate_tokens(&chunk.text) as i64,
                    embeddings
                        .and_then(|items| items.get(chunk.ordinal))
                        .map(|embedding| serde_json::to_string(embedding))
                        .transpose()?,
                ],
            )?;
            let rowid = tx.last_insert_rowid();
            tx.execute(
                "INSERT INTO document_chunks_fts(rowid, text, section_title, path) VALUES (?1, ?2, ?3, ?4)",
                params![rowid, chunk.text, chunk.section_title, parsed.path],
            )?;
        }

        tx.commit()?;
        Ok(UpsertStats {
            chunks_written: parsed.chunks.len(),
        })
    }

    pub fn prune_missing_for_root(
        &mut self,
        root_path: &Path,
        alive_paths: &HashSet<String>,
    ) -> Result<usize> {
        let mut stale = Vec::new();
        {
            let mut stmt = self
                .conn
                .prepare("SELECT path FROM documents WHERE root_path = ?1 ORDER BY path ASC")?;
            let rows = stmt.query_map([root_path.display().to_string()], |row| row.get(0))?;
            for row in rows {
                let path: String = row?;
                if !alive_paths.contains(&path) {
                    stale.push(path);
                }
            }
        }
        for path in &stale {
            self.delete_document_by_path(path)?;
        }
        Ok(stale.len())
    }

    pub fn delete_document_by_path(&mut self, path: &str) -> Result<()> {
        let doc_id = self
            .conn
            .query_row(
                "SELECT doc_id FROM documents WHERE path = ?1",
                [path],
                |row| row.get::<_, String>(0),
            )
            .optional()?;
        if let Some(doc_id) = doc_id {
            let tx = self.conn.transaction()?;
            delete_document_rows(&tx, &doc_id)?;
            tx.commit()?;
        }
        Ok(())
    }

    pub fn search_lexical(&self, query: &str, limit: usize) -> Result<Vec<SearchCandidate>> {
        let fts_query =
            sanitize_fts_query(query).context("query does not contain searchable terms")?;
        let mut stmt = self.conn.prepare(
            r#"
            SELECT
                c.chunk_id,
                d.path,
                d.title,
                d.parser_kind,
                c.page_number,
                c.start_line,
                c.end_line,
                c.section_title,
                c.text,
                bm25(document_chunks_fts) AS lexical_score
            FROM document_chunks_fts
            JOIN document_chunks c ON c.rowid = document_chunks_fts.rowid
            JOIN documents d ON d.doc_id = c.doc_id
            WHERE document_chunks_fts MATCH ?1
            ORDER BY lexical_score ASC, d.path ASC, c.ordinal ASC
            LIMIT ?2
            "#,
        )?;
        let rows = stmt.query_map(params![fts_query, limit as i64], |row| {
            let lexical_score: f64 = row.get(9)?;
            Ok(SearchCandidate {
                chunk_id: row.get(0)?,
                path: row.get(1)?,
                title: row.get(2)?,
                parser_kind: row.get(3)?,
                page_number: row.get::<_, Option<i64>>(4)?.map(|value| value as usize),
                start_line: row.get::<_, Option<i64>>(5)?.map(|value| value as usize),
                end_line: row.get::<_, Option<i64>>(6)?.map(|value| value as usize),
                section_title: row.get(7)?,
                text: row.get(8)?,
                lexical_score: Some(lexical_score),
                semantic_score: None,
                combined_score: lexical_score,
            })
        })?;
        Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
    }

    pub fn rerank_candidates_with_embedding(
        &self,
        candidates: &[SearchCandidate],
        query_embedding: &[f64],
        limit: usize,
    ) -> Result<Vec<SearchCandidate>> {
        let by_chunk = self.load_chunk_embeddings(
            &candidates
                .iter()
                .map(|candidate| candidate.chunk_id.clone())
                .collect::<Vec<_>>(),
        )?;
        let mut reranked = candidates.to_vec();
        for candidate in &mut reranked {
            if let Some(embedding) = by_chunk.get(&candidate.chunk_id) {
                let semantic = cosine_similarity(query_embedding, embedding).unwrap_or(0.0);
                let lexical = candidate
                    .lexical_score
                    .map(lexical_score_to_similarity)
                    .unwrap_or(0.0);
                candidate.semantic_score = Some(semantic);
                candidate.combined_score = (semantic * 0.7) + (lexical * 0.3);
            } else {
                candidate.combined_score = candidate
                    .lexical_score
                    .map(lexical_score_to_similarity)
                    .unwrap_or(0.0);
            }
        }
        reranked.sort_by(|left, right| {
            right
                .combined_score
                .partial_cmp(&left.combined_score)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        reranked.truncate(limit);
        Ok(reranked)
    }

    pub fn search_semantic(
        &self,
        query_embedding: &[f64],
        limit: usize,
    ) -> Result<Vec<SearchCandidate>> {
        let mut stmt = self.conn.prepare(
            r#"
            SELECT
                c.chunk_id,
                d.path,
                d.title,
                d.parser_kind,
                c.page_number,
                c.start_line,
                c.end_line,
                c.section_title,
                c.text,
                c.embedding_json
            FROM document_chunks c
            JOIN documents d ON d.doc_id = c.doc_id
            WHERE c.embedding_json IS NOT NULL
            "#,
        )?;
        let mut rows = stmt.query([])?;
        let mut results = Vec::new();
        while let Some(row) = rows.next()? {
            let embedding_json: String = row.get(9)?;
            let embedding = parse_embedding_json(&embedding_json)?;
            if let Some(semantic_score) = cosine_similarity(query_embedding, &embedding) {
                results.push(SearchCandidate {
                    chunk_id: row.get(0)?,
                    path: row.get(1)?,
                    title: row.get(2)?,
                    parser_kind: row.get(3)?,
                    page_number: row.get::<_, Option<i64>>(4)?.map(|value| value as usize),
                    start_line: row.get::<_, Option<i64>>(5)?.map(|value| value as usize),
                    end_line: row.get::<_, Option<i64>>(6)?.map(|value| value as usize),
                    section_title: row.get(7)?,
                    text: row.get(8)?,
                    lexical_score: None,
                    semantic_score: Some(semantic_score),
                    combined_score: semantic_score,
                });
            }
        }
        results.sort_by(|left, right| {
            right
                .combined_score
                .partial_cmp(&left.combined_score)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        results.truncate(limit);
        Ok(results)
    }

    fn load_chunk_embeddings(&self, chunk_ids: &[String]) -> Result<HashMap<String, Vec<f64>>> {
        let mut map = HashMap::new();
        if chunk_ids.is_empty() {
            return Ok(map);
        }
        let mut stmt = self.conn.prepare(
            "SELECT chunk_id, embedding_json FROM document_chunks WHERE chunk_id = ?1 AND embedding_json IS NOT NULL",
        )?;
        for chunk_id in chunk_ids {
            let row = stmt
                .query_row([chunk_id], |row| {
                    Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
                })
                .optional()?;
            if let Some((chunk_id, embedding_json)) = row {
                map.insert(chunk_id, parse_embedding_json(&embedding_json)?);
            }
        }
        Ok(map)
    }
}

fn delete_document_rows(tx: &rusqlite::Transaction<'_>, doc_id: &str) -> Result<()> {
    let mut rowid_stmt = tx.prepare("SELECT rowid FROM document_chunks WHERE doc_id = ?1")?;
    let rowids = rowid_stmt
        .query_map([doc_id], |row| row.get::<_, i64>(0))?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    for rowid in rowids {
        tx.execute("DELETE FROM document_chunks_fts WHERE rowid = ?1", [rowid])?;
    }
    tx.execute("DELETE FROM document_chunks WHERE doc_id = ?1", [doc_id])?;
    tx.execute("DELETE FROM documents WHERE doc_id = ?1", [doc_id])?;
    Ok(())
}

fn document_id(path: &str) -> String {
    format!("doc:{}", sha256_text(path))
}

fn sha256_text(text: &str) -> String {
    use sha2::{Digest, Sha256};

    let mut hasher = Sha256::new();
    hasher.update(text.as_bytes());
    format!("{:x}", hasher.finalize())
}

fn sanitize_fts_query(query: &str) -> Option<String> {
    let tokens = query
        .split(|value: char| !value.is_alphanumeric())
        .filter(|value| !value.is_empty())
        .map(|value| format!("\"{}\"", value.replace('"', "")))
        .collect::<Vec<_>>();
    if tokens.is_empty() {
        None
    } else {
        Some(tokens.join(" OR "))
    }
}

fn parse_embedding_json(raw: &str) -> Result<Vec<f64>> {
    serde_json::from_str::<Vec<f64>>(raw).context("failed to parse stored embedding")
}

fn estimate_tokens(text: &str) -> usize {
    (text.len() / 4).max(text.split_whitespace().count())
}

fn cosine_similarity(left: &[f64], right: &[f64]) -> Option<f64> {
    if left.is_empty() || left.len() != right.len() {
        return None;
    }
    let mut dot = 0.0;
    let mut left_norm = 0.0;
    let mut right_norm = 0.0;
    for (left_value, right_value) in left.iter().zip(right.iter()) {
        dot += left_value * right_value;
        left_norm += left_value * left_value;
        right_norm += right_value * right_value;
    }
    if left_norm <= f64::EPSILON || right_norm <= f64::EPSILON {
        None
    } else {
        Some(dot / (left_norm.sqrt() * right_norm.sqrt()))
    }
}

fn lexical_score_to_similarity(score: f64) -> f64 {
    1.0 / (1.0 + score.abs())
}

fn iso_now() -> String {
    Utc::now().to_rfc3339()
}

#[cfg(test)]
mod tests {
    use super::DocStore;
    use crate::parse::parse_document;
    use tempfile::tempdir;

    #[test]
    fn lexical_search_roundtrip_returns_indexed_chunk() {
        let root = tempdir().unwrap();
        let file_path = root.path().join("notes.md");
        std::fs::create_dir_all(root.path().join("runtime/documents")).unwrap();
        std::fs::write(
            &file_path,
            "# Remote Rollout\n\nPatch the rollout script and verify smoke checks.\n",
        )
        .unwrap();
        let parsed = parse_document(&file_path).unwrap();
        let mut store = DocStore::open(root.path()).unwrap();
        store
            .upsert_document(None, &parsed, None, None)
            .expect("index document");
        let results = store.search_lexical("rollout smoke", 5).unwrap();
        assert_eq!(results.len(), 1);
        assert!(results[0].text.contains("smoke checks"));
    }
}
