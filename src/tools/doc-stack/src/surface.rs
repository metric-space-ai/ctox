use anyhow::{Context, Result};
use serde::Serialize;
use std::cmp::Ordering;
use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};

use crate::formats::{format_specs, ExistingWriteSurface, PreferredSemanticWriteMode};
use crate::parse::{file_fingerprint, parse_document, supported_document_file, ParsedChunk};
use crate::store::{CorpusRootRecord, DocStore, IndexedDocument, SearchCandidate};
use crate::EmbeddingExecutor;

const DEFAULT_SEARCH_LIMIT: usize = 8;
const SEARCH_CANDIDATE_MULTIPLIER: usize = 8;
const READ_EXCERPT_LIMIT: usize = 5;
const FIND_RESULT_LIMIT: usize = 20;
const EMBEDDING_BATCH_MAX_ITEMS: usize = 8;
const EMBEDDING_BATCH_MAX_CHARS: usize = 24_000;

#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "snake_case")]
enum SearchMode {
    Lexical,
    Hybrid,
    Semantic,
}

#[derive(Debug, Serialize)]
struct CorpusListOutput {
    ok: bool,
    roots: Vec<CorpusRootRecord>,
}

#[derive(Debug, Serialize)]
struct CorpusAddOutput {
    ok: bool,
    root: CorpusRootRecord,
}

#[derive(Debug, Serialize)]
struct FormatsOutput {
    ok: bool,
    formats: Vec<FormatOutputItem>,
}

#[derive(Debug, Serialize)]
struct FormatOutputItem {
    parser_kind: String,
    extensions: Vec<String>,
    read_supported: bool,
    existing_write_surface: ExistingWriteSurface,
    preferred_semantic_write_mode: PreferredSemanticWriteMode,
    semantic_write_ready: bool,
    notes: String,
}

#[derive(Debug, Serialize)]
struct IndexOutput {
    ok: bool,
    roots: Vec<String>,
    embedding_model: Option<String>,
    documents_seen: usize,
    documents_indexed: usize,
    documents_skipped: usize,
    documents_pruned: usize,
    chunks_written: usize,
    warnings: Vec<String>,
}

#[derive(Debug, Serialize)]
struct SearchOutput {
    ok: bool,
    query: String,
    mode: SearchMode,
    limit: usize,
    indexed_documents: usize,
    embedding_model: Option<String>,
    warnings: Vec<String>,
    results: Vec<SearchOutputItem>,
}

#[derive(Debug, Serialize)]
struct SearchOutputItem {
    path: String,
    title: String,
    parser_kind: String,
    page_number: Option<usize>,
    start_line: Option<usize>,
    end_line: Option<usize>,
    section_title: Option<String>,
    lexical_score: Option<f64>,
    semantic_score: Option<f64>,
    combined_score: f64,
    excerpt: String,
}

#[derive(Debug, Serialize)]
struct ReadOutput {
    ok: bool,
    path: String,
    title: String,
    parser_kind: String,
    is_pdf: bool,
    page_count: Option<usize>,
    chunk_count: usize,
    summary: String,
    warnings: Vec<String>,
    excerpts: Vec<ReadExcerpt>,
    find_results: Vec<FindResult>,
    context: String,
}

#[derive(Debug, Serialize)]
struct ReadExcerpt {
    page_number: Option<usize>,
    start_line: Option<usize>,
    end_line: Option<usize>,
    section_title: Option<String>,
    score: f64,
    text: String,
}

#[derive(Debug, Serialize)]
struct FindResult {
    pattern: String,
    page_number: Option<usize>,
    start_line: Option<usize>,
    end_line: Option<usize>,
    section_title: Option<String>,
    text: String,
}

struct PreparedReadDocument {
    path: String,
    title: String,
    parser_kind: String,
    is_pdf: bool,
    page_count: Option<usize>,
    chunks: Vec<ParsedChunk>,
    indexed_embedding_model: Option<String>,
    indexed_chunk_embeddings: Option<Vec<Option<Vec<f64>>>>,
}

pub fn handle_doc_command(
    root: &Path,
    args: &[String],
    embedder: &dyn EmbeddingExecutor,
) -> Result<()> {
    match args.first().map(String::as_str) {
        Some("corpus") => handle_corpus_command(root, &args[1..]),
        Some("formats") => handle_formats_command(),
        Some("index") => handle_index_command(root, &args[1..], embedder),
        Some("search") => handle_search_command(root, &args[1..], embedder),
        Some("read") => handle_read_command(root, &args[1..], embedder),
        _ => anyhow::bail!(
            "usage:\n  ctox doc corpus add-root --path <path> [--label <text>]\n  ctox doc corpus list\n  ctox doc formats\n  ctox doc index [--root <path>]... [--reindex]\n  ctox doc search --query <text> [--limit <n>] [--mode <lexical|hybrid|semantic>]\n  ctox doc read --path <path> [--query <text>] [--find <text>]..."
        ),
    }
}

fn handle_corpus_command(root: &Path, args: &[String]) -> Result<()> {
    match args.first().map(String::as_str) {
        Some("add-root") => {
            let raw_path = required_flag_value(args, "--path")
                .context("usage: ctox doc corpus add-root --path <path> [--label <text>]")?;
            let root_path = canonicalize_existing_path(raw_path)?;
            if !root_path.is_dir() {
                anyhow::bail!("{} is not a directory", root_path.display());
            }
            let label = find_flag_value(args, "--label");
            let store = DocStore::open(root)?;
            let record = store.upsert_root(&root_path, label)?;
            print_json(&CorpusAddOutput { ok: true, root: record })
        }
        Some("list") => {
            let store = DocStore::open(root)?;
            let roots = store.list_roots()?;
            print_json(&CorpusListOutput { ok: true, roots })
        }
        _ => anyhow::bail!(
            "usage:\n  ctox doc corpus add-root --path <path> [--label <text>]\n  ctox doc corpus list"
        ),
    }
}

fn handle_formats_command() -> Result<()> {
    let formats = format_specs()
        .iter()
        .map(|spec| FormatOutputItem {
            parser_kind: spec.parser_kind.to_string(),
            extensions: spec
                .extensions
                .iter()
                .map(|value| (*value).to_string())
                .collect(),
            read_supported: true,
            existing_write_surface: spec.existing_write_surface,
            preferred_semantic_write_mode: spec.preferred_semantic_write_mode,
            semantic_write_ready: false,
            notes: spec.notes.to_string(),
        })
        .collect::<Vec<_>>();
    print_json(&FormatsOutput { ok: true, formats })
}

fn handle_index_command(
    root: &Path,
    args: &[String],
    embedder: &dyn EmbeddingExecutor,
) -> Result<()> {
    let explicit_roots = find_flag_values(args, "--root")
        .into_iter()
        .map(canonicalize_existing_path)
        .collect::<Result<Vec<_>>>()?;
    let reindex = args.iter().any(|value| value == "--reindex");
    let mut store = DocStore::open(root)?;
    let roots = resolve_roots(&store, &explicit_roots)?;
    if roots.is_empty() {
        anyhow::bail!(
            "no document roots configured; run `ctox doc corpus add-root --path <path>` first"
        );
    }
    for explicit_root in &explicit_roots {
        store.upsert_root(explicit_root, None)?;
    }

    let mut warnings = Vec::new();
    let embedding_model = embedder.default_model(root).ok();
    if embedding_model.is_none() {
        warnings.push(
            "embedding runtime unavailable; indexing continues with lexical retrieval only"
                .to_string(),
        );
    }

    let mut documents_seen = 0usize;
    let mut documents_indexed = 0usize;
    let mut documents_skipped = 0usize;
    let mut documents_pruned = 0usize;
    let mut chunks_written = 0usize;

    for corpus_root in &roots {
        let files = walk_supported_files(corpus_root)?;
        let alive_paths = files
            .iter()
            .map(|path| path.display().to_string())
            .collect::<HashSet<_>>();
        for file_path in files {
            documents_seen += 1;
            let path_display = file_path.display().to_string();
            let fingerprint = match file_fingerprint(&file_path) {
                Ok(value) => value,
                Err(err) => {
                    warnings.push(format!("{path_display}: {err}"));
                    continue;
                }
            };
            if !reindex {
                let freshness = store.document_freshness(
                    &path_display,
                    fingerprint.size_bytes,
                    fingerprint.modified_at,
                    embedding_model.as_deref(),
                )?;
                if freshness.as_ref().is_some_and(|record| record.is_fresh) {
                    documents_skipped += 1;
                    continue;
                }
            }

            let parsed = match parse_document(&file_path) {
                Ok(value) => value,
                Err(err) => {
                    warnings.push(format!("{path_display}: {err}"));
                    continue;
                }
            };

            let embeddings = if let Some(model) = embedding_model.as_deref() {
                let inputs = parsed
                    .chunks
                    .iter()
                    .map(|chunk| chunk.text.clone())
                    .collect::<Vec<_>>();
                match embed_inputs_batched(root, embedder, model, &inputs) {
                    Ok(vectors) => Some(vectors),
                    Err(err) => {
                        warnings.push(format!(
                            "{path_display}: embedding failed, indexed without semantic vectors ({err})"
                        ));
                        None
                    }
                }
            } else {
                None
            };

            let stats = store.upsert_document(
                Some(corpus_root.as_path()),
                &parsed,
                embedding_model.as_deref().filter(|_| embeddings.is_some()),
                embeddings.as_deref(),
            )?;
            documents_indexed += 1;
            chunks_written += stats.chunks_written;
        }
        documents_pruned += store.prune_missing_for_root(corpus_root, &alive_paths)?;
    }

    print_json(&IndexOutput {
        ok: true,
        roots: roots
            .iter()
            .map(|value| value.display().to_string())
            .collect(),
        embedding_model,
        documents_seen,
        documents_indexed,
        documents_skipped,
        documents_pruned,
        chunks_written,
        warnings,
    })
}

fn handle_search_command(
    root: &Path,
    args: &[String],
    embedder: &dyn EmbeddingExecutor,
) -> Result<()> {
    let query = required_flag_value(args, "--query")
        .or_else(|| args.first().map(String::as_str))
        .context("usage: ctox doc search --query <text> [--limit <n>] [--mode <lexical|hybrid|semantic>]")?
        .to_string();
    let limit = find_flag_value(args, "--limit")
        .map(|value| value.parse::<usize>())
        .transpose()
        .context("failed to parse --limit")?
        .unwrap_or(DEFAULT_SEARCH_LIMIT)
        .max(1);
    let mode = find_flag_value(args, "--mode")
        .map(parse_search_mode)
        .transpose()?
        .unwrap_or(SearchMode::Hybrid);
    let store = DocStore::open(root)?;
    let indexed_documents = store.indexed_document_count()?;
    let SearchExecution {
        results,
        warnings,
        embedding_model,
    } = execute_search(&store, root, &query, limit, mode, embedder)?;

    let results = results
        .into_iter()
        .take(limit)
        .map(search_output_item)
        .collect::<Vec<_>>();
    print_json(&SearchOutput {
        ok: true,
        query,
        mode,
        limit,
        indexed_documents,
        embedding_model,
        warnings,
        results,
    })
}

struct SearchExecution {
    results: Vec<SearchCandidate>,
    warnings: Vec<String>,
    embedding_model: Option<String>,
}

fn execute_search(
    store: &DocStore,
    root: &Path,
    query: &str,
    limit: usize,
    mode: SearchMode,
    embedder: &dyn EmbeddingExecutor,
) -> Result<SearchExecution> {
    let mut warnings = Vec::new();
    let mut embedding_model = None;
    let results = match mode {
        SearchMode::Lexical => store.search_lexical(query, limit * SEARCH_CANDIDATE_MULTIPLIER)?,
        SearchMode::Hybrid => {
            let lexical = store.search_lexical(query, limit * SEARCH_CANDIDATE_MULTIPLIER)?;
            if lexical.is_empty() {
                warnings.push("no lexical candidates found".to_string());
                lexical
            } else {
                match embedder.default_model(root) {
                    Ok(model) => {
                        embedding_model = Some(model.clone());
                        match embedder.embed_texts(root, &model, &[query.to_string()]) {
                            Ok(query_embedding) => store.rerank_candidates_with_embedding(
                                &lexical,
                                &query_embedding[0],
                                limit,
                            )?,
                            Err(err) => {
                                warnings.push(format!(
                                    "embedding inference failed for hybrid mode; returning lexical results only ({err})"
                                ));
                                lexical.into_iter().take(limit).collect()
                            }
                        }
                    }
                    Err(err) => {
                        warnings.push(format!(
                            "embedding runtime unavailable; returning lexical results only ({err})"
                        ));
                        lexical.into_iter().take(limit).collect()
                    }
                }
            }
        }
        SearchMode::Semantic => match embedder.default_model(root) {
            Ok(model) => {
                embedding_model = Some(model.clone());
                match embedder.embed_texts(root, &model, &[query.to_string()]) {
                    Ok(query_embedding) => store.search_semantic(&query_embedding[0], limit)?,
                    Err(err) => {
                        warnings.push(format!(
                            "embedding inference failed for semantic mode; falling back to lexical search ({err})"
                        ));
                        store.search_lexical(query, limit)?
                    }
                }
            }
            Err(err) => {
                warnings.push(format!(
                    "embedding runtime unavailable for semantic mode; falling back to lexical search ({err})"
                ));
                store.search_lexical(query, limit)?
            }
        },
    };
    Ok(SearchExecution {
        results,
        warnings,
        embedding_model,
    })
}

fn handle_read_command(
    root: &Path,
    args: &[String],
    embedder: &dyn EmbeddingExecutor,
) -> Result<()> {
    let path = required_flag_value(args, "--path")
        .or_else(|| args.first().map(String::as_str))
        .context("usage: ctox doc read --path <path> [--query <text>] [--find <text>]...")?;
    let canonical_path = canonicalize_existing_path(path)?;
    let query = find_flag_value(args, "--query").map(ToOwned::to_owned);
    let find_patterns = find_flag_values(args, "--find")
        .into_iter()
        .map(ToOwned::to_owned)
        .collect::<Vec<_>>();
    let mut warnings = Vec::new();
    let store = DocStore::open(root)?;
    let prepared = prepare_read_document(&store, &canonical_path, &mut warnings)?;

    let scored_chunks = score_chunks(
        root,
        &prepared.chunks,
        query.as_deref(),
        prepared.indexed_embedding_model.as_deref(),
        prepared.indexed_chunk_embeddings.as_deref(),
        embedder,
        &mut warnings,
    )?;
    let excerpts = scored_chunks
        .into_iter()
        .take(READ_EXCERPT_LIMIT)
        .map(|(chunk, score)| ReadExcerpt {
            page_number: chunk.page_number,
            start_line: chunk.start_line,
            end_line: chunk.end_line,
            section_title: chunk.section_title.clone(),
            score,
            text: trim_text(&chunk.text, 1_200),
        })
        .collect::<Vec<_>>();

    let find_results = build_find_results(&prepared.chunks, &find_patterns);
    let summary = prepared
        .chunks
        .first()
        .map(|chunk| trim_text(&chunk.text, 320))
        .unwrap_or_else(|| "No extractable text found.".to_string());
    let context = build_read_context(&prepared.path, &excerpts);

    print_json(&ReadOutput {
        ok: true,
        path: prepared.path,
        title: prepared.title,
        parser_kind: prepared.parser_kind,
        is_pdf: prepared.is_pdf,
        page_count: prepared.page_count,
        chunk_count: prepared.chunks.len(),
        summary,
        warnings,
        excerpts,
        find_results,
        context,
    })
}

fn resolve_roots(store: &DocStore, explicit_roots: &[PathBuf]) -> Result<Vec<PathBuf>> {
    if !explicit_roots.is_empty() {
        return Ok(explicit_roots.to_vec());
    }
    let configured = store
        .root_paths()?
        .into_iter()
        .map(PathBuf::from)
        .collect::<Vec<_>>();
    if !configured.is_empty() {
        return Ok(configured);
    }
    Ok(default_documents_root().into_iter().collect())
}

fn default_documents_root() -> Option<PathBuf> {
    std::env::var_os("HOME")
        .map(PathBuf::from)
        .map(|path| path.join("Documents"))
        .filter(|path| path.is_dir())
}

fn walk_supported_files(root: &Path) -> Result<Vec<PathBuf>> {
    let mut out = Vec::new();
    walk_dir_recursive(root, &mut out)?;
    out.sort();
    Ok(out)
}

fn walk_dir_recursive(root: &Path, out: &mut Vec<PathBuf>) -> Result<()> {
    for entry in fs::read_dir(root).with_context(|| format!("failed to read {}", root.display()))? {
        let entry = entry?;
        let path = entry.path();
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if name.starts_with('.') {
            continue;
        }
        if path.is_dir() {
            if matches!(name.as_ref(), "node_modules" | "target" | "__pycache__") {
                continue;
            }
            walk_dir_recursive(&path, out)?;
            continue;
        }
        if supported_document_file(&path) {
            out.push(path);
        }
    }
    Ok(())
}

fn score_chunks(
    root: &Path,
    chunks: &[ParsedChunk],
    query: Option<&str>,
    indexed_embedding_model: Option<&str>,
    indexed_chunk_embeddings: Option<&[Option<Vec<f64>>]>,
    embedder: &dyn EmbeddingExecutor,
    warnings: &mut Vec<String>,
) -> Result<Vec<(ParsedChunk, f64)>> {
    if chunks.is_empty() {
        return Ok(Vec::new());
    }
    let Some(query) = query.filter(|value| !value.trim().is_empty()) else {
        return Ok(chunks
            .iter()
            .take(READ_EXCERPT_LIMIT)
            .cloned()
            .enumerate()
            .map(|(index, chunk)| (chunk, 1.0 / (index + 1) as f64))
            .collect());
    };

    let model = match embedder.default_model(root) {
        Ok(model) => model,
        Err(err) => {
            warnings.push(format!(
                "embedding runtime unavailable for excerpt ranking; using lexical scoring instead ({err})"
            ));
            return Ok(finalize_scored_chunks(lexical_chunk_scores(chunks, query)));
        }
    };

    let query_embedding = match embedder.embed_texts(root, &model, &[query.to_string()]) {
        Ok(embeddings) => embeddings,
        Err(err) => {
            warnings.push(format!(
                "embedding-assisted excerpt ranking failed; using lexical scoring instead ({err})"
            ));
            return Ok(finalize_scored_chunks(lexical_chunk_scores(chunks, query)));
        }
    };

    if indexed_embedding_model == Some(model.as_str()) {
        if let Some(stored_embeddings) = indexed_chunk_embeddings {
            let scored = chunks
                .iter()
                .cloned()
                .zip(stored_embeddings.iter())
                .filter_map(|(chunk, embedding)| {
                    embedding.as_ref().and_then(|values| {
                        cosine_similarity(&query_embedding[0], values).map(|score| (chunk, score))
                    })
                })
                .collect::<Vec<_>>();
            if !scored.is_empty() {
                return Ok(finalize_scored_chunks(scored));
            }
        }
    }

    let inputs = chunks
        .iter()
        .map(|chunk| chunk.text.clone())
        .collect::<Vec<_>>();
    match embed_inputs_batched(root, embedder, &model, &inputs) {
        Ok(chunk_embeddings) => {
            let scored = chunks
                .iter()
                .cloned()
                .zip(chunk_embeddings.into_iter())
                .map(|(chunk, embedding)| {
                    let score = cosine_similarity(&query_embedding[0], &embedding).unwrap_or(0.0);
                    (chunk, score)
                })
                .collect::<Vec<_>>();
            Ok(finalize_scored_chunks(scored))
        }
        Err(err) => {
            warnings.push(format!(
                "embedding-assisted excerpt ranking failed; using lexical scoring instead ({err})"
            ));
            Ok(finalize_scored_chunks(lexical_chunk_scores(chunks, query)))
        }
    }
}

fn lexical_chunk_scores(chunks: &[ParsedChunk], query: &str) -> Vec<(ParsedChunk, f64)> {
    let terms = query_terms(query);
    chunks
        .iter()
        .cloned()
        .map(|chunk| {
            let lowered = chunk.text.to_ascii_lowercase();
            let mut score = 0.0;
            for term in &terms {
                if lowered.contains(term) {
                    score += 1.0;
                }
            }
            (chunk, score / terms.len().max(1) as f64)
        })
        .collect()
}

fn finalize_scored_chunks(mut scored: Vec<(ParsedChunk, f64)>) -> Vec<(ParsedChunk, f64)> {
    scored.retain(|(_, score)| *score > 0.0);
    scored.sort_by(|left, right| right.1.partial_cmp(&left.1).unwrap_or(Ordering::Equal));
    scored
}

fn embed_inputs_batched(
    root: &Path,
    embedder: &dyn EmbeddingExecutor,
    model: &str,
    inputs: &[String],
) -> Result<Vec<Vec<f64>>> {
    if inputs.is_empty() {
        return Ok(Vec::new());
    }

    let mut vectors = Vec::with_capacity(inputs.len());
    let mut start = 0usize;
    while start < inputs.len() {
        let mut end = start;
        let mut chars = 0usize;
        while end < inputs.len() && end - start < EMBEDDING_BATCH_MAX_ITEMS {
            let next_chars = inputs[end].chars().count();
            if end > start && chars + next_chars > EMBEDDING_BATCH_MAX_CHARS {
                break;
            }
            chars += next_chars;
            end += 1;
            if chars >= EMBEDDING_BATCH_MAX_CHARS {
                break;
            }
        }
        if end == start {
            end += 1;
        }

        let batch = &inputs[start..end];
        let batch_vectors = embedder.embed_texts(root, model, batch)?;
        if batch_vectors.len() != batch.len() {
            anyhow::bail!(
                "embedding response count mismatch: expected {}, got {}",
                batch.len(),
                batch_vectors.len()
            );
        }
        vectors.extend(batch_vectors);
        start = end;
    }
    Ok(vectors)
}

fn build_find_results(chunks: &[ParsedChunk], patterns: &[String]) -> Vec<FindResult> {
    let mut results = Vec::new();
    for pattern in patterns {
        let lowered_pattern = pattern.to_ascii_lowercase();
        for chunk in chunks {
            if chunk.text.to_ascii_lowercase().contains(&lowered_pattern) {
                results.push(FindResult {
                    pattern: pattern.clone(),
                    page_number: chunk.page_number,
                    start_line: chunk.start_line,
                    end_line: chunk.end_line,
                    section_title: chunk.section_title.clone(),
                    text: trim_text(&chunk.text, 500),
                });
                if results.len() >= FIND_RESULT_LIMIT {
                    return results;
                }
            }
        }
    }
    results
}

fn prepare_read_document(
    store: &DocStore,
    path: &Path,
    warnings: &mut Vec<String>,
) -> Result<PreparedReadDocument> {
    let path_display = path.display().to_string();
    let fingerprint = file_fingerprint(path)?;
    if let Some(indexed) = store.load_document_by_path(&path_display)? {
        if indexed.size_bytes == fingerprint.size_bytes
            && indexed.modified_at == fingerprint.modified_at
        {
            return Ok(prepared_from_indexed(indexed));
        }
        warnings.push("document index is stale; reparsing current file contents".to_string());
    }

    let parsed = parse_document(path)?;
    Ok(PreparedReadDocument {
        path: parsed.path,
        title: parsed.title,
        parser_kind: parsed.parser_kind.clone(),
        is_pdf: parsed.is_pdf,
        page_count: parsed.page_count,
        chunks: parsed.chunks,
        indexed_embedding_model: None,
        indexed_chunk_embeddings: None,
    })
}

fn prepared_from_indexed(indexed: IndexedDocument) -> PreparedReadDocument {
    let chunk_embeddings = indexed
        .chunks
        .iter()
        .map(|chunk| chunk.embedding.clone())
        .collect::<Vec<_>>();
    let chunks = indexed
        .chunks
        .into_iter()
        .map(|chunk| chunk.chunk)
        .collect::<Vec<_>>();
    PreparedReadDocument {
        path: indexed.path,
        title: indexed.title,
        parser_kind: indexed.parser_kind.clone(),
        is_pdf: indexed.parser_kind == "pdf",
        page_count: indexed.page_count,
        chunks,
        indexed_embedding_model: indexed.embedding_model,
        indexed_chunk_embeddings: chunk_embeddings
            .iter()
            .any(|value| value.is_some())
            .then_some(chunk_embeddings),
    }
}

fn build_read_context(path: &str, excerpts: &[ReadExcerpt]) -> String {
    let mut out = format!("Document: {path}\n");
    for excerpt in excerpts {
        out.push_str("\n[excerpt]\n");
        if let Some(page_number) = excerpt.page_number {
            out.push_str(&format!("page: {page_number}\n"));
        }
        if let (Some(start_line), Some(end_line)) = (excerpt.start_line, excerpt.end_line) {
            out.push_str(&format!("lines: {start_line}-{end_line}\n"));
        }
        if let Some(section_title) = &excerpt.section_title {
            out.push_str(&format!("section: {section_title}\n"));
        }
        out.push_str(&excerpt.text);
        out.push('\n');
    }
    out.trim().to_string()
}

fn search_output_item(result: SearchCandidate) -> SearchOutputItem {
    SearchOutputItem {
        path: result.path,
        title: result.title,
        parser_kind: result.parser_kind,
        page_number: result.page_number,
        start_line: result.start_line,
        end_line: result.end_line,
        section_title: result.section_title,
        lexical_score: result.lexical_score,
        semantic_score: result.semantic_score,
        combined_score: result.combined_score,
        excerpt: trim_text(&result.text, 900),
    }
}

fn parse_search_mode(raw: &str) -> Result<SearchMode> {
    match raw {
        "lexical" => Ok(SearchMode::Lexical),
        "hybrid" => Ok(SearchMode::Hybrid),
        "semantic" => Ok(SearchMode::Semantic),
        other => {
            anyhow::bail!("unsupported --mode `{other}`; expected lexical, hybrid, or semantic")
        }
    }
}

fn query_terms(query: &str) -> Vec<String> {
    query
        .split(|value: char| !value.is_alphanumeric())
        .filter(|value| !value.is_empty())
        .map(|value| value.to_ascii_lowercase())
        .collect()
}

fn trim_text(text: &str, max_chars: usize) -> String {
    if text.chars().count() <= max_chars {
        text.to_string()
    } else {
        text.chars().take(max_chars).collect::<String>() + "..."
    }
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

fn required_flag_value<'a>(args: &'a [String], flag: &str) -> Option<&'a str> {
    find_flag_value(args, flag)
}

fn find_flag_value<'a>(args: &'a [String], flag: &str) -> Option<&'a str> {
    let index = args.iter().position(|value| value == flag)?;
    args.get(index + 1).map(String::as_str)
}

fn find_flag_values<'a>(args: &'a [String], flag: &str) -> Vec<&'a str> {
    let mut out = Vec::new();
    let mut index = 0usize;
    while index < args.len() {
        if args[index] == flag {
            if let Some(value) = args.get(index + 1) {
                out.push(value.as_str());
            }
            index += 2;
        } else {
            index += 1;
        }
    }
    out
}

fn canonicalize_existing_path(raw: &str) -> Result<PathBuf> {
    std::fs::canonicalize(raw).with_context(|| format!("failed to resolve {}", raw))
}

fn print_json<T: Serialize>(value: &T) -> Result<()> {
    println!("{}", serde_json::to_string_pretty(value)?);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{embed_inputs_batched, execute_search, score_chunks, SearchMode};
    use crate::parse::ParsedChunk;
    use crate::store::DocStore;
    use crate::EmbeddingExecutor;
    use anyhow::{anyhow, Result};
    use std::path::Path;
    use std::sync::{Arc, Mutex};
    use tempfile::tempdir;

    struct FakeEmbedder {
        model: Option<String>,
        model_error: Option<String>,
        embeddings: Option<Vec<Vec<f64>>>,
        embedding_error: Option<String>,
    }

    impl EmbeddingExecutor for FakeEmbedder {
        fn default_model(&self, _root: &Path) -> Result<String> {
            match (&self.model, &self.model_error) {
                (Some(model), _) => Ok(model.clone()),
                (None, Some(message)) => Err(anyhow!(message.clone())),
                _ => Err(anyhow!("missing fake model configuration")),
            }
        }

        fn embed_texts(
            &self,
            _root: &Path,
            _model: &str,
            _inputs: &[String],
        ) -> Result<Vec<Vec<f64>>> {
            match (&self.embeddings, &self.embedding_error) {
                (Some(embeddings), _) => Ok(embeddings.clone()),
                (None, Some(message)) => Err(anyhow!(message.clone())),
                _ => Err(anyhow!("missing fake embedding configuration")),
            }
        }
    }

    struct RecordingEmbedder {
        calls: Arc<Mutex<Vec<usize>>>,
    }

    impl EmbeddingExecutor for RecordingEmbedder {
        fn default_model(&self, _root: &Path) -> Result<String> {
            Ok("fake-embed".to_string())
        }

        fn embed_texts(
            &self,
            _root: &Path,
            _model: &str,
            inputs: &[String],
        ) -> Result<Vec<Vec<f64>>> {
            self.calls.lock().unwrap().push(inputs.len());
            Ok(inputs.iter().map(|_| vec![1.0, 0.0]).collect())
        }
    }

    struct QueryOnlyEmbedder;

    impl EmbeddingExecutor for QueryOnlyEmbedder {
        fn default_model(&self, _root: &Path) -> Result<String> {
            Ok("fake-embed".to_string())
        }

        fn embed_texts(
            &self,
            _root: &Path,
            _model: &str,
            inputs: &[String],
        ) -> Result<Vec<Vec<f64>>> {
            if inputs.len() == 1 {
                Ok(vec![vec![1.0, 0.0]])
            } else {
                Err(anyhow!("chunk re-embedding should not be needed"))
            }
        }
    }

    fn seed_store(root: &Path) -> Result<DocStore> {
        let corpus = root.join("docs");
        std::fs::create_dir_all(&corpus)?;
        std::fs::write(
            corpus.join("roadmap.md"),
            "# Roadmap\n\nOwners and milestones are tracked in the launch roadmap.\n",
        )?;
        std::fs::write(
            corpus.join("ops.txt"),
            "Operations owner\n\nIncident response timeline and escalation notes.\n",
        )?;

        let mut store = DocStore::open(root)?;
        for path in [corpus.join("roadmap.md"), corpus.join("ops.txt")] {
            let parsed = crate::parse::parse_document(&path)?;
            store.upsert_document(Some(corpus.as_path()), &parsed, None, None)?;
        }
        Ok(store)
    }

    #[test]
    fn hybrid_search_falls_back_to_lexical_when_embedding_inference_fails() {
        let root = tempdir().unwrap();
        let store = seed_store(root.path()).unwrap();
        let embedder = FakeEmbedder {
            model: Some("fake-embed".to_string()),
            model_error: None,
            embeddings: None,
            embedding_error: Some("socket timeout".to_string()),
        };

        let execution = execute_search(
            &store,
            root.path(),
            "owners milestones",
            5,
            SearchMode::Hybrid,
            &embedder,
        )
        .unwrap();

        assert!(!execution.results.is_empty());
        assert!(execution
            .warnings
            .iter()
            .any(|warning| warning.contains("hybrid mode")));
        assert_eq!(execution.results[0].path.ends_with("roadmap.md"), true);
    }

    #[test]
    fn semantic_search_falls_back_to_lexical_when_embedding_inference_fails() {
        let root = tempdir().unwrap();
        let store = seed_store(root.path()).unwrap();
        let embedder = FakeEmbedder {
            model: Some("fake-embed".to_string()),
            model_error: None,
            embeddings: None,
            embedding_error: Some("embedding backend died".to_string()),
        };

        let execution = execute_search(
            &store,
            root.path(),
            "timeline escalation",
            5,
            SearchMode::Semantic,
            &embedder,
        )
        .unwrap();

        assert!(!execution.results.is_empty());
        assert!(execution
            .warnings
            .iter()
            .any(|warning| warning.contains("semantic mode")));
        assert_eq!(execution.results[0].path.ends_with("ops.txt"), true);
    }

    #[test]
    fn read_chunk_scoring_falls_back_to_lexical_when_embeddings_fail() {
        let root = tempdir().unwrap();
        let path = root.path().join("roadmap.md");
        std::fs::write(
            &path,
            "# Roadmap\n\nOwners and milestones are tracked here.\n\nBudget review happens later.\n",
        )
        .unwrap();
        let parsed = crate::parse::parse_document(&path).unwrap();
        let embedder = FakeEmbedder {
            model: Some("fake-embed".to_string()),
            model_error: None,
            embeddings: None,
            embedding_error: Some("transport failure".to_string()),
        };
        let mut warnings = Vec::new();
        let scored = score_chunks(
            root.path(),
            &parsed.chunks,
            Some("milestones owners"),
            None,
            None,
            &embedder,
            &mut warnings,
        )
        .unwrap();

        assert!(!scored.is_empty());
        assert!(warnings
            .iter()
            .any(|warning| warning.contains("lexical scoring instead")));
        assert!(scored[0].0.text.to_ascii_lowercase().contains("milestones"));
    }

    #[test]
    fn embedding_batches_respect_batch_item_limit() {
        let root = tempdir().unwrap();
        let calls = Arc::new(Mutex::new(Vec::new()));
        let embedder = RecordingEmbedder {
            calls: calls.clone(),
        };
        let inputs = (0..20)
            .map(|index| format!("chunk {index}"))
            .collect::<Vec<_>>();

        let vectors = embed_inputs_batched(root.path(), &embedder, "fake-embed", &inputs).unwrap();

        assert_eq!(vectors.len(), inputs.len());
        assert_eq!(*calls.lock().unwrap(), vec![8, 8, 4]);
    }

    #[test]
    fn read_chunk_scoring_prefers_stored_embeddings_when_available() {
        let root = tempdir().unwrap();
        let chunks = vec![
            ParsedChunk {
                ordinal: 0,
                text: "Owners and milestones".to_string(),
                page_number: Some(1),
                start_line: None,
                end_line: None,
                section_title: None,
            },
            ParsedChunk {
                ordinal: 1,
                text: "Escalation timeline".to_string(),
                page_number: Some(1),
                start_line: None,
                end_line: None,
                section_title: None,
            },
        ];
        let stored_embeddings = vec![Some(vec![1.0, 0.0]), Some(vec![0.0, 1.0])];
        let mut warnings = Vec::new();

        let scored = score_chunks(
            root.path(),
            &chunks,
            Some("owners milestones"),
            Some("fake-embed"),
            Some(&stored_embeddings),
            &QueryOnlyEmbedder,
            &mut warnings,
        )
        .unwrap();

        assert!(warnings.is_empty());
        assert_eq!(scored[0].0.ordinal, 0);
    }
}
