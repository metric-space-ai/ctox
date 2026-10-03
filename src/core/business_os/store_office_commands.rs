// Origin: CTOX
// License: Apache-2.0

use super::store::{
    first_string_field, now_ms, open_store, rxdb_store_path, short_hash, sqlite_table_exists,
    upsert_rxdb_collection_record, BusinessCommand, DOCUMENT_BLOB_CHUNK_SIZE,
};
use super::store_projections::upsert_business_record;
use anyhow::Context;
use base64::Engine;
use rusqlite::{params, Connection, OpenFlags, OptionalExtension};
use serde_json::Value;
use std::path::Path;

pub(super) fn handle_office_control_command(
    root: &Path,
    command: &BusinessCommand,
) -> anyhow::Result<Value> {
    use super::office_engine::{
        apply_changes, compile_document_to_email, delimited_text_to_xlsx, export, prepare,
        sha256_hex, ApplyChangesOptions, OfficeKind, PrepareOptions,
    };

    let (kind, module_id, records_collection, versions_collection, chunks_collection) =
        if command.command_type.starts_with("office.document.") {
            (
                OfficeKind::Document,
                "documents",
                "documents",
                "document_versions",
                "document_blob_chunks",
            )
        } else {
            (
                OfficeKind::Spreadsheet,
                "spreadsheets",
                "spreadsheets",
                "spreadsheet_versions",
                "spreadsheet_blob_chunks",
            )
        };
    anyhow::ensure!(
        command.module == module_id,
        "office command module mismatch: expected {module_id}, got {}",
        command.module
    );
    let record_id = first_string_field(
        &command.payload,
        &["document_id", "spreadsheet_id", "record_id"],
    )
    .or_else(|| command.record_id.clone())
    .context("office command record id is required")?;
    let record = load_rxdb_office_document(root, records_collection, &record_id)?
        .with_context(|| format!("office record not found: {record_id}"))?;
    let requested_version_id =
        first_string_field(&command.payload, &["version_id", "base_version_id"]);
    let version_id = requested_version_id
        .or_else(|| {
            record
                .get("current_version_id")
                .and_then(Value::as_str)
                .map(str::to_string)
        })
        .filter(|value| !value.is_empty())
        .context("office command version id is required")?;
    let version = load_rxdb_office_document(root, versions_collection, &version_id)?
        .with_context(|| format!("office version not found: {version_id}"))?;

    match command.command_type.as_str() {
        "office.document.prepare" | "office.spreadsheet.prepare" => {
            let source_blob_id = version
                .get("blob_id")
                .and_then(Value::as_str)
                .filter(|value| !value.is_empty())
                .context("office version has no canonical blob")?;
            let source = load_rxdb_office_blob(root, chunks_collection, source_blob_id)?;
            let delimited_source = kind == OfficeKind::Spreadsheet
                && !source.starts_with(b"PK")
                && (version
                    .get("source_kind")
                    .and_then(Value::as_str)
                    .is_some_and(|value| {
                        value.contains("csv") || value.contains("tsv") || value == "created_blank"
                    })
                    || record
                        .get("mime_type")
                        .and_then(Value::as_str)
                        .is_some_and(|value| {
                            matches!(value, "text/csv" | "text/tab-separated-values")
                        })
                    || record
                        .get("filename")
                        .and_then(Value::as_str)
                        .is_some_and(|value| {
                            let lower = value.to_ascii_lowercase();
                            lower.ends_with(".csv") || lower.ends_with(".tsv")
                        }));
            let (source, canonical_blob_id) = if delimited_source {
                let canonical = delimited_text_to_xlsx(&source)?;
                let blob_id = persist_office_canonical_spreadsheet_blob(
                    root,
                    chunks_collection,
                    &record_id,
                    &version_id,
                    &canonical,
                )?;
                (canonical, Some(blob_id))
            } else {
                (source, None)
            };
            let prepared = prepare(
                kind,
                &source,
                PrepareOptions {
                    implemented_features: if kind == OfficeKind::Document {
                        vec![
                            "document.open-render-zoom".to_string(),
                            "document.edit-save".to_string(),
                            "document.undo-clipboard-keyboard".to_string(),
                            "document.character-paragraph-formatting".to_string(),
                            "document.styles-lists-numbering".to_string(),
                            "document.tables".to_string(),
                            "document.images-positioning".to_string(),
                            "document.sections-headers-footers".to_string(),
                            "document.links-bookmarks-fields".to_string(),
                            "document.comments-track-changes".to_string(),
                            "document.drawings-charts".to_string(),
                        ]
                    } else {
                        vec!["spreadsheet.open-render-sheets".to_string()]
                    },
                },
            )?;
            let editor_blob_id = if prepared.editor_payload == source {
                source_blob_id.to_string()
            } else {
                persist_office_editor_blob(
                    root,
                    kind,
                    chunks_collection,
                    &record_id,
                    &version_id,
                    &prepared,
                )?
            };
            let mut next_version = version.clone();
            let object = next_version
                .as_object_mut()
                .context("office version must be an object")?;
            object.insert(
                "editor_blob_id".to_string(),
                Value::String(editor_blob_id.clone()),
            );
            if let Some(canonical_blob_id) = canonical_blob_id.as_ref() {
                object
                    .entry("original_blob_id".to_string())
                    .or_insert_with(|| Value::String(source_blob_id.to_string()));
                object.insert(
                    "blob_id".to_string(),
                    Value::String(canonical_blob_id.clone()),
                );
                object.insert(
                    "canonical_source_kind".to_string(),
                    Value::String("delimited_text_to_xlsx".to_string()),
                );
                object.insert(
                    "canonical_mime_type".to_string(),
                    Value::String(OfficeKind::Spreadsheet.canonical_mime().to_string()),
                );
            }
            object.insert(
                "editor_protocol".to_string(),
                Value::String(prepared.protocol.clone()),
            );
            object.insert(
                "editor_protocol_version".to_string(),
                Value::from(prepared.protocol_version),
            );
            object.insert(
                "source_sha256".to_string(),
                Value::String(prepared.source_sha256.clone()),
            );
            object.insert(
                "editor_sha256".to_string(),
                Value::String(prepared.editor_sha256.clone()),
            );
            object.insert(
                "conversion_state".to_string(),
                Value::String("prepared".to_string()),
            );
            object.insert(
                "implemented_features".to_string(),
                serde_json::to_value(&prepared.implemented_features)?,
            );
            object.insert(
                "office_manifest".to_string(),
                serde_json::to_value(&prepared.manifest)?,
            );
            object.insert(
                "editor_manifest".to_string(),
                serde_json::to_value(&prepared.editor_manifest)?,
            );
            object.insert("updated_at_ms".to_string(), Value::from(now_ms() as i64));
            let conn = open_store(root)?;
            upsert_business_record(
                &conn,
                versions_collection,
                &version_id,
                now_ms() as i64,
                next_version.clone(),
            )?;
            // Prepare is a terminal control command. Publish the prepared
            // version before the terminal result can reach browser peers so
            // the editor never observes metadata without its projection.
            upsert_rxdb_collection_record(
                root,
                versions_collection,
                &version_id,
                now_ms() as i64,
                next_version,
            )?;
            Ok(serde_json::json!({
                "ok": true,
                "operation": "prepare",
                "record_id": record_id,
                "version_id": version_id,
                "blob_id": canonical_blob_id.unwrap_or_else(|| source_blob_id.to_string()),
                "editor_blob_id": editor_blob_id,
                "editor_protocol": prepared.protocol,
                "editor_protocol_version": prepared.protocol_version,
                "source_sha256": prepared.source_sha256,
                "editor_sha256": prepared.editor_sha256,
                "manifest": prepared.manifest,
                "editor_manifest": prepared.editor_manifest,
                "diagnostics": prepared.diagnostics,
            }))
        }
        "office.document.commit" | "office.spreadsheet.commit" => {
            let base_version_id = first_string_field(&command.payload, &["base_version_id"])
                .context("office commit requires base_version_id")?;
            anyhow::ensure!(
                base_version_id == version_id,
                "version_conflict: requested base version differs from loaded version"
            );
            let current_version_id = record
                .get("current_version_id")
                .and_then(Value::as_str)
                .unwrap_or_default();
            anyhow::ensure!(
                current_version_id == base_version_id,
                "version_conflict: current version is {current_version_id}, expected {base_version_id}"
            );
            let editor_blob_id = first_string_field(&command.payload, &["editor_blob_id"])
                .context("office commit requires editor_blob_id")?;
            let changed_payload = load_rxdb_office_blob(root, chunks_collection, &editor_blob_id)?;
            if let Some(expected) = first_string_field(&command.payload, &["editor_sha256"]) {
                anyhow::ensure!(
                    sha256_hex(&changed_payload) == expected,
                    "office staged editor payload hash mismatch"
                );
            }
            let canonical_base_blob_id = version
                .get("blob_id")
                .and_then(Value::as_str)
                .filter(|value| !value.is_empty())
                .context("base office version has no blob")?;
            let canonical_base_payload =
                load_rxdb_office_blob(root, chunks_collection, canonical_base_blob_id)?;
            let base_editor_blob_id = version
                .get("editor_blob_id")
                .and_then(Value::as_str)
                .filter(|value| !value.is_empty())
                .unwrap_or(canonical_base_blob_id);
            let base_payload = load_rxdb_office_blob(root, chunks_collection, base_editor_blob_id)?;
            let expected_base_sha256 = version
                .get("editor_sha256")
                .or_else(|| version.get("source_sha256"))
                .and_then(Value::as_str)
                .map(str::to_string)
                .unwrap_or_else(|| sha256_hex(&base_payload));
            let implemented_features = command
                .payload
                .get("implemented_features")
                .and_then(Value::as_array)
                .map(|values| {
                    values
                        .iter()
                        .filter_map(Value::as_str)
                        .map(str::to_string)
                        .collect::<Vec<_>>()
                })
                .unwrap_or_default();
            let prepared = apply_changes(
                kind,
                &base_payload,
                &changed_payload,
                ApplyChangesOptions {
                    expected_base_sha256,
                    implemented_features,
                },
            )?;
            let package = export(
                kind,
                &prepared.editor_payload,
                Some(&canonical_base_payload),
            )?;
            match kind {
                super::office_engine::OfficeKind::Document => commit_office_document_version(
                    root,
                    command,
                    &record,
                    &version,
                    &base_version_id,
                    &editor_blob_id,
                    &package,
                    &prepared,
                ),
                super::office_engine::OfficeKind::Spreadsheet => commit_office_spreadsheet_version(
                    root,
                    command,
                    &record,
                    &version,
                    &base_version_id,
                    &editor_blob_id,
                    &package,
                    &prepared,
                ),
            }
        }
        "office.document.freeze_email_content" => {
            anyhow::ensure!(
                kind == OfficeKind::Document,
                "email content can only be compiled from a document"
            );
            let source_blob_id = version
                .get("blob_id")
                .and_then(Value::as_str)
                .filter(|value| !value.is_empty())
                .context("office version has no canonical blob")?;
            let source = load_rxdb_office_blob(root, chunks_collection, source_blob_id)?;
            let compiled = compile_document_to_email(&source)?;
            if let Some(expected) = version
                .get("source_sha256")
                .and_then(Value::as_str)
                .filter(|value| !value.is_empty())
            {
                anyhow::ensure!(
                    expected == compiled.source_sha256,
                    "office version source hash mismatch"
                );
            }
            let html_blob_id = persist_document_email_blob(
                root,
                &record_id,
                &version_id,
                "email_html",
                "text/html; charset=utf-8",
                &compiled.html_sha256,
                compiled.html.as_bytes(),
            )?;
            let text_blob_id = persist_document_email_blob(
                root,
                &record_id,
                &version_id,
                "email_text",
                "text/plain; charset=utf-8",
                &compiled.text_sha256,
                compiled.text.as_bytes(),
            )?;
            let mut asset_artifacts = Vec::with_capacity(compiled.assets.len());
            for asset in &compiled.assets {
                let blob_id = persist_document_email_blob(
                    root,
                    &record_id,
                    &version_id,
                    "email_asset",
                    &asset.mime_type,
                    &asset.sha256,
                    &asset.bytes,
                )?;
                asset_artifacts.push(serde_json::json!({
                    "content_id": asset.content_id,
                    "filename": asset.filename,
                    "mime_type": asset.mime_type,
                    "blob_id": blob_id,
                    "sha256": asset.sha256,
                    "bytes": asset.bytes.len(),
                }));
            }
            let now = now_ms() as i64;
            let artifact = serde_json::json!({
                "schema_version": compiled.schema_version,
                "state": "frozen",
                "compiler_id": compiled.compiler_id,
                "document_id": record_id,
                "document_version_id": version_id,
                "source_blob_id": source_blob_id,
                "source_sha256": compiled.source_sha256,
                "html_blob_id": html_blob_id,
                "html_sha256": compiled.html_sha256,
                "html_bytes": compiled.html.len(),
                "text_blob_id": text_blob_id,
                "text_sha256": compiled.text_sha256,
                "text_bytes": compiled.text.len(),
                "assets": asset_artifacts,
                "diagnostics": compiled.diagnostics,
                "frozen_at_ms": now,
            });
            let mut next_version = version.clone();
            let object = next_version
                .as_object_mut()
                .context("office version must be an object")?;
            object.insert("email_render_artifact".to_string(), artifact.clone());
            object.insert("updated_at_ms".to_string(), Value::from(now));
            let conn = open_store(root)?;
            upsert_business_record(
                &conn,
                versions_collection,
                &version_id,
                now,
                next_version.clone(),
            )?;
            upsert_rxdb_collection_record(
                root,
                versions_collection,
                &version_id,
                now,
                next_version,
            )?;
            Ok(serde_json::json!({
                "ok": true,
                "operation": "freeze_email_content",
                "record_id": record_id,
                "version_id": version_id,
                "artifact": artifact,
            }))
        }
        "office.document.export" | "office.spreadsheet.export" => {
            let blob_id = version
                .get("blob_id")
                .and_then(Value::as_str)
                .filter(|value| !value.is_empty())
                .context("office version has no export blob")?;
            let bytes = load_rxdb_office_blob(root, chunks_collection, blob_id)?;
            let package = export(kind, &bytes, None)?;
            Ok(serde_json::json!({
                "ok": true,
                "operation": "export",
                "record_id": record_id,
                "version_id": version_id,
                "blob_id": blob_id,
                "mime_type": package.mime_type,
                "extension": package.extension,
                "sha256": package.sha256,
                "bytes": package.bytes.len(),
                "manifest": package.manifest,
                "diagnostics": package.diagnostics,
            }))
        }
        other => anyhow::bail!("unsupported office command type: {other}"),
    }
}

fn persist_document_email_blob(
    root: &Path,
    document_id: &str,
    version_id: &str,
    role: &str,
    mime_type: &str,
    expected_sha256: &str,
    bytes: &[u8],
) -> anyhow::Result<String> {
    anyhow::ensure!(
        !document_id.is_empty(),
        "email artifact document id is empty"
    );
    anyhow::ensure!(!version_id.is_empty(), "email artifact version id is empty");
    anyhow::ensure!(
        super::office_engine::sha256_hex(bytes) == expected_sha256,
        "email artifact hash mismatch"
    );
    let suffix = expected_sha256.get(..12).unwrap_or(expected_sha256);
    let blob_id = format!("{version_id}_{role}_{suffix}");
    let chunks = if bytes.is_empty() {
        vec![&[][..]]
    } else {
        bytes.chunks(DOCUMENT_BLOB_CHUNK_SIZE).collect::<Vec<_>>()
    };
    let total = chunks.len();
    let now = now_ms() as i64;
    let conn = open_store(root)?;
    let tx = conn.unchecked_transaction()?;
    let mut projections = Vec::with_capacity(total);
    for (index, chunk) in chunks.into_iter().enumerate() {
        let chunk_id = format!("{blob_id}_{index:04}");
        let payload = serde_json::json!({
            "id": chunk_id,
            "blob_id": blob_id,
            "document_id": document_id,
            "version_id": version_id,
            "idx": index,
            "total": total,
            "mime_type": mime_type,
            "encoding": "base64",
            "data": base64::engine::general_purpose::STANDARD.encode(chunk),
            "created_at_ms": now,
        });
        upsert_business_record(&tx, "document_blob_chunks", &chunk_id, now, payload.clone())?;
        projections.push((chunk_id, payload));
    }
    tx.commit()?;
    for (chunk_id, payload) in projections {
        upsert_rxdb_collection_record(root, "document_blob_chunks", &chunk_id, now, payload)?;
    }
    Ok(blob_id)
}

fn persist_office_canonical_spreadsheet_blob(
    root: &Path,
    chunks_collection: &str,
    spreadsheet_id: &str,
    version_id: &str,
    bytes: &[u8],
) -> anyhow::Result<String> {
    anyhow::ensure!(
        chunks_collection == "spreadsheet_blob_chunks",
        "canonical spreadsheet blob used the wrong collection"
    );
    let digest = super::office_engine::sha256_hex(bytes);
    let suffix = digest.get(..12).unwrap_or(&digest);
    let blob_id = format!("{version_id}_canonical_{suffix}");
    let total = bytes.len().div_ceil(DOCUMENT_BLOB_CHUNK_SIZE).max(1);
    let now = now_ms() as i64;
    let conn = open_store(root)?;
    let tx = conn.unchecked_transaction()?;
    let mut projections = Vec::with_capacity(total);
    for (index, chunk) in bytes.chunks(DOCUMENT_BLOB_CHUNK_SIZE).enumerate() {
        let chunk_id = format!("{blob_id}_{index:04}");
        let payload = serde_json::json!({
            "id": chunk_id,
            "blob_id": blob_id,
            "spreadsheet_id": spreadsheet_id,
            "version_id": version_id,
            "idx": index,
            "total": total,
            "mime_type": super::office_engine::OfficeKind::Spreadsheet.canonical_mime(),
            "encoding": "base64",
            "data": base64::engine::general_purpose::STANDARD.encode(chunk),
            "sha256": super::office_engine::sha256_hex(chunk),
            "created_at_ms": now,
        });
        upsert_business_record(&tx, chunks_collection, &chunk_id, now, payload.clone())?;
        projections.push((chunk_id, payload));
    }
    tx.commit()?;
    for (chunk_id, payload) in projections {
        upsert_rxdb_collection_record(root, chunks_collection, &chunk_id, now, payload)?;
    }
    Ok(blob_id)
}

fn load_rxdb_office_document(
    root: &Path,
    collection: &str,
    document_id: &str,
) -> anyhow::Result<Option<Value>> {
    let table = match collection {
        "documents" => "ctox_business_os__documents__v0",
        "document_versions" => "ctox_business_os__document_versions__v0",
        "spreadsheets" => "ctox_business_os__spreadsheets__v0",
        "spreadsheet_versions" => "ctox_business_os__spreadsheet_versions__v0",
        _ => anyhow::bail!("unsupported office collection: {collection}"),
    };
    let path = rxdb_store_path(root);
    if !path.exists() {
        return Ok(None);
    }
    let conn = Connection::open_with_flags(&path, OpenFlags::SQLITE_OPEN_READ_ONLY)
        .with_context(|| format!("open Office RxDB store {}", path.display()))?;
    if !sqlite_table_exists(&conn, table)? {
        return Ok(None);
    }
    let raw: Option<String> = conn
        .query_row(
            &format!("SELECT data FROM \"{table}\" WHERE id = ?1 AND COALESCE(deleted, 0) = 0"),
            params![document_id],
            |row| row.get(0),
        )
        .optional()?;
    raw.map(|value| serde_json::from_str(&value).context("decode Office RxDB document"))
        .transpose()
}

pub(super) fn load_rxdb_office_blob(
    root: &Path,
    collection: &str,
    blob_id: &str,
) -> anyhow::Result<Vec<u8>> {
    let table = match collection {
        "document_blob_chunks" => "ctox_business_os__document_blob_chunks__v0",
        "spreadsheet_blob_chunks" => "ctox_business_os__spreadsheet_blob_chunks__v0",
        _ => anyhow::bail!("unsupported office blob collection: {collection}"),
    };
    let path = rxdb_store_path(root);
    let conn = Connection::open_with_flags(&path, OpenFlags::SQLITE_OPEN_READ_ONLY)
        .with_context(|| format!("open Office RxDB store {}", path.display()))?;
    anyhow::ensure!(
        sqlite_table_exists(&conn, table)?,
        "office blob table is missing"
    );
    let lower = format!("{blob_id}_");
    let upper = format!("{lower}\u{10ffff}");
    let mut stmt = conn.prepare(&format!(
        "SELECT data FROM \"{table}\" WHERE id >= ?1 AND id < ?2 AND COALESCE(deleted, 0) = 0 ORDER BY id"
    ))?;
    let rows = stmt.query_map(params![lower, upper], |row| row.get::<_, String>(0))?;
    let mut chunks = Vec::new();
    for row in rows {
        let value: Value = serde_json::from_str(&row?)?;
        if value.get("blob_id").and_then(Value::as_str) != Some(blob_id) {
            continue;
        }
        chunks.push(value);
    }
    chunks.sort_by_key(|value| value.get("idx").and_then(Value::as_u64).unwrap_or(u64::MAX));
    anyhow::ensure!(!chunks.is_empty(), "office blob has no chunks: {blob_id}");
    let total = chunks[0]
        .get("total")
        .and_then(Value::as_u64)
        .context("office blob chunk has no total")?;
    anyhow::ensure!(
        chunks.len() == total as usize,
        "office blob is incomplete: {blob_id}"
    );
    let mut bytes = Vec::new();
    for (expected_idx, chunk) in chunks.iter().enumerate() {
        anyhow::ensure!(
            chunk.get("idx").and_then(Value::as_u64) == Some(expected_idx as u64)
                && chunk.get("total").and_then(Value::as_u64) == Some(total),
            "office blob chunk sequence is invalid: {blob_id}"
        );
        let encoded = chunk
            .get("data")
            .and_then(Value::as_str)
            .context("office blob chunk has no data")?;
        bytes.extend(base64::engine::general_purpose::STANDARD.decode(encoded)?);
    }
    Ok(bytes)
}

fn persist_office_editor_blob(
    root: &Path,
    kind: super::office_engine::OfficeKind,
    chunks_collection: &str,
    record_id: &str,
    version_id: &str,
    prepared: &super::office_engine::PreparedEditorPayload,
) -> anyhow::Result<String> {
    let suffix = prepared
        .editor_sha256
        .get(..12)
        .unwrap_or(&prepared.editor_sha256);
    let blob_id = format!("{version_id}_editor_{suffix}");
    let total = prepared
        .editor_payload
        .len()
        .div_ceil(DOCUMENT_BLOB_CHUNK_SIZE)
        .max(1);
    let now = now_ms() as i64;
    let conn = open_store(root)?;
    let tx = conn.unchecked_transaction()?;
    let record_field = match kind {
        super::office_engine::OfficeKind::Document => "document_id",
        super::office_engine::OfficeKind::Spreadsheet => "spreadsheet_id",
    };
    let mut chunk_projections = Vec::with_capacity(total);
    for (index, chunk) in prepared
        .editor_payload
        .chunks(DOCUMENT_BLOB_CHUNK_SIZE)
        .enumerate()
    {
        let chunk_id = format!("{blob_id}_{index:04}");
        let mut payload = serde_json::json!({
            "id": chunk_id,
            "blob_id": blob_id,
            "version_id": version_id,
            "idx": index,
            "total": total,
            "mime_type": "application/vnd.ctox.euro-office-editor-binary",
            "encoding": "base64",
            "data": base64::engine::general_purpose::STANDARD.encode(chunk),
            "sha256": super::office_engine::sha256_hex(chunk),
            "created_at_ms": now,
        });
        payload
            .as_object_mut()
            .expect("editor chunk payload object")
            .insert(
                record_field.to_string(),
                Value::String(record_id.to_string()),
            );
        upsert_business_record(&tx, chunks_collection, &chunk_id, now, payload.clone())?;
        chunk_projections.push((chunk_id, payload));
    }
    tx.commit()?;
    // Keep the generic reconciliation store and the live RxDB projection in
    // lockstep for terminal prepare commands. The periodic projector remains
    // the repair path after crashes.
    for (chunk_id, payload) in chunk_projections {
        upsert_rxdb_collection_record(root, chunks_collection, &chunk_id, now, payload)?;
    }
    Ok(blob_id)
}

fn commit_office_document_version(
    root: &Path,
    command: &BusinessCommand,
    record: &Value,
    base_version: &Value,
    base_version_id: &str,
    staged_editor_blob_id: &str,
    package: &super::office_engine::OfficePackage,
    prepared: &super::office_engine::PreparedEditorPayload,
) -> anyhow::Result<Value> {
    let document_id = record
        .get("id")
        .or_else(|| record.get("document_id"))
        .and_then(Value::as_str)
        .context("office document id is missing")?;
    let now = now_ms() as i64;
    let version_number = base_version
        .get("version")
        .and_then(Value::as_i64)
        .unwrap_or_default()
        + 1;
    let command_id = command
        .id
        .as_deref()
        .context("office commit command id is required")?;
    let version_id = format!(
        "{document_id}_office_v{version_number}_{}",
        short_hash(command_id)
    );
    let blob_id = format!("{version_id}_blob");
    let conn = open_store(root)?;
    let replay: Option<String> = conn
        .query_row(
            "SELECT payload_json FROM business_records
             WHERE collection = 'document_versions' AND record_id = ?1 AND deleted = 0",
            params![version_id],
            |row| row.get(0),
        )
        .optional()?;
    if let Some(replay) = replay {
        let value: Value = serde_json::from_str(&replay)?;
        return Ok(serde_json::json!({
            "ok": true,
            "idempotent_replay": true,
            "operation": "commit",
            "document_id": document_id,
            "base_version_id": base_version_id,
            "version_id": version_id,
            "blob_id": value.get("blob_id").and_then(Value::as_str).unwrap_or(&blob_id),
            "editor_blob_id": value.get("editor_blob_id").and_then(Value::as_str).unwrap_or(&blob_id),
            "source_sha256": value.get("source_sha256").cloned().unwrap_or(Value::Null),
            "editor_sha256": value.get("editor_sha256").cloned().unwrap_or(Value::Null),
            "manifest": value.get("office_manifest").cloned().unwrap_or(Value::Null),
            "diagnostics": value.get("diagnostics").cloned().unwrap_or(Value::Null),
        }));
    }
    let mut document_payload = record.clone();
    let document_object = document_payload
        .as_object_mut()
        .context("office document payload must be an object")?;
    document_object.insert(
        "current_version_id".to_string(),
        Value::String(version_id.clone()),
    );
    document_object.insert("status".to_string(), Value::String("Draft".to_string()));
    document_object.insert(
        "source_sha256".to_string(),
        Value::String(package.sha256.clone()),
    );
    document_object.insert(
        "index_text".to_string(),
        Value::String(package.manifest.primary_text.chars().take(20_000).collect()),
    );
    document_object.insert("updated_at_ms".to_string(), Value::from(now));

    let diagnostics = [prepared.diagnostics.clone(), package.diagnostics.clone()].concat();
    let version_payload = serde_json::json!({
        "id": version_id,
        "version_id": version_id,
        "document_id": document_id,
        "version": version_number,
        "source_kind": "office_edited_docx",
        "blob_id": blob_id,
        "base_version_id": base_version_id,
        "editor_blob_id": staged_editor_blob_id,
        "staged_editor_blob_id": staged_editor_blob_id,
        "editor_protocol": prepared.protocol,
        "editor_protocol_version": prepared.protocol_version,
        "source_sha256": package.sha256,
        "editor_sha256": prepared.editor_sha256,
        "conversion_state": "prepared",
        "implemented_features": prepared.implemented_features,
        "office_manifest": package.manifest,
        "diagnostics": diagnostics,
        "model_json": {
            "type": "docx",
            "text": package.manifest.primary_text.chars().take(20_000).collect::<String>(),
            "parts": package.manifest.parts.len(),
        },
        "business_command_id": command.id,
        "created_at_ms": now,
        "updated_at_ms": now,
    });

    let tx = conn.unchecked_transaction()?;
    let title = record
        .get("title")
        .and_then(Value::as_str)
        .unwrap_or("Document");
    let filename = record
        .get("filename")
        .and_then(Value::as_str)
        .unwrap_or("document.docx");
    let created_at_ms = record
        .get("created_at_ms")
        .and_then(Value::as_i64)
        .unwrap_or(now);
    let tags_json = serde_json::to_string(record.get("tags").unwrap_or(&Value::Null))?;
    tx.execute(
        "INSERT INTO business_documents
            (document_id, title, filename, mime_type, status, document_type, current_version_id,
             source_sha256, page_count, diagnostics_count, tags_json, index_text, deleted,
             created_at_ms, updated_at_ms, payload_json)
         VALUES (?1, ?2, ?3, ?4, ?5, 'word_document', ?6, ?7, 0, 0, ?8, ?9, 0, ?10, ?11, ?12)
         ON CONFLICT(document_id) DO NOTHING",
        params![
            document_id,
            title,
            filename,
            super::office_engine::OfficeKind::Document.canonical_mime(),
            record
                .get("status")
                .and_then(Value::as_str)
                .unwrap_or("Draft"),
            base_version_id,
            record
                .get("source_sha256")
                .and_then(Value::as_str)
                .unwrap_or(""),
            tags_json,
            record
                .get("index_text")
                .and_then(Value::as_str)
                .unwrap_or(""),
            created_at_ms,
            now,
            serde_json::to_string(record)?,
        ],
    )?;
    let authoritative_base: String = tx.query_row(
        "SELECT current_version_id FROM business_documents WHERE document_id = ?1",
        params![document_id],
        |row| row.get(0),
    )?;
    anyhow::ensure!(
        authoritative_base == base_version_id,
        "version_conflict: authoritative current version is {authoritative_base}, expected {base_version_id}"
    );

    let total = package
        .bytes
        .len()
        .div_ceil(DOCUMENT_BLOB_CHUNK_SIZE)
        .max(1);
    let mut chunk_projections = Vec::with_capacity(total);
    for (idx, chunk) in package.bytes.chunks(DOCUMENT_BLOB_CHUNK_SIZE).enumerate() {
        let chunk_id = format!("{blob_id}_{idx:04}");
        let encoded = base64::engine::general_purpose::STANDARD.encode(chunk);
        let payload = serde_json::json!({
            "id": chunk_id,
            "blob_id": blob_id,
            "document_id": document_id,
            "version_id": version_id,
            "idx": idx,
            "total": total,
            "mime_type": super::office_engine::OfficeKind::Document.canonical_mime(),
            "encoding": "base64",
            "data": encoded,
            "created_at_ms": now,
        });
        tx.execute(
            "INSERT INTO business_document_blob_chunks
                (chunk_id, blob_id, document_id, version_id, idx, total, mime_type, encoding,
                 data, deleted, created_at_ms, payload_json)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, 'base64', ?8, 0, ?9, ?10)",
            params![
                chunk_id,
                blob_id,
                document_id,
                version_id,
                idx as i64,
                total as i64,
                super::office_engine::OfficeKind::Document.canonical_mime(),
                encoded,
                now,
                serde_json::to_string(&payload)?,
            ],
        )?;
        upsert_business_record(&tx, "document_blob_chunks", &chunk_id, now, payload.clone())?;
        chunk_projections.push((chunk_id, payload));
    }

    tx.execute(
        "INSERT INTO business_document_versions
            (version_id, document_id, version, source_kind, blob_id, diagnostics_json,
             model_json, deleted, created_at_ms, updated_at_ms, payload_json)
         VALUES (?1, ?2, ?3, 'office_edited_docx', ?4, ?5, ?6, 0, ?7, ?8, ?9)",
        params![
            version_id,
            document_id,
            version_number,
            blob_id,
            serde_json::to_string(&diagnostics)?,
            serde_json::to_string(version_payload.get("model_json").unwrap_or(&Value::Null))?,
            now,
            now,
            serde_json::to_string(&version_payload)?,
        ],
    )?;
    upsert_business_record(
        &tx,
        "document_versions",
        &version_id,
        now,
        version_payload.clone(),
    )?;
    let updated = tx.execute(
        "UPDATE business_documents
         SET current_version_id = ?3, status = 'Draft', source_sha256 = ?4,
             index_text = ?5, updated_at_ms = ?6, payload_json = ?7
         WHERE document_id = ?1 AND current_version_id = ?2",
        params![
            document_id,
            base_version_id,
            version_id,
            package.sha256,
            package
                .manifest
                .primary_text
                .chars()
                .take(20_000)
                .collect::<String>(),
            now,
            serde_json::to_string(&document_payload)?,
        ],
    )?;
    anyhow::ensure!(
        updated == 1,
        "version_conflict: document changed during commit"
    );
    upsert_business_record(&tx, "documents", document_id, now, document_payload.clone())?;
    tx.commit()?;

    // Office commits are terminal control commands, so their new version and
    // blob must be visible over RxDB/WebRTC when the terminal command result
    // is observed. The periodic generic projector remains the reconciliation
    // path, while these direct writes close the command/projection race.
    for (chunk_id, payload) in chunk_projections {
        upsert_rxdb_collection_record(root, "document_blob_chunks", &chunk_id, now, payload)?;
    }
    upsert_rxdb_collection_record(root, "document_versions", &version_id, now, version_payload)?;
    upsert_rxdb_collection_record(root, "documents", document_id, now, document_payload)?;

    let mut statement = conn.prepare(
        "SELECT data FROM business_document_blob_chunks
         WHERE blob_id = ?1 AND deleted = 0 ORDER BY idx",
    )?;
    let encoded_chunks = statement.query_map(params![blob_id], |row| row.get::<_, String>(0))?;
    let mut verified_bytes = Vec::with_capacity(package.bytes.len());
    for encoded in encoded_chunks {
        verified_bytes.extend(base64::engine::general_purpose::STANDARD.decode(encoded?)?);
    }
    anyhow::ensure!(
        verified_bytes.len() == package.bytes.len()
            && super::office_engine::sha256_hex(&verified_bytes) == package.sha256,
        "office commit integrity verification failed for {blob_id}"
    );

    Ok(serde_json::json!({
        "ok": true,
        "operation": "commit",
        "document_id": document_id,
        "base_version_id": base_version_id,
        "version_id": version_id,
        "blob_id": blob_id,
        "editor_blob_id": blob_id,
        "source_sha256": package.sha256,
        "editor_sha256": prepared.editor_sha256,
        "bytes": package.bytes.len(),
        "chunks": total,
        "manifest": package.manifest,
        "diagnostics": diagnostics,
    }))
}

fn commit_office_spreadsheet_version(
    root: &Path,
    command: &BusinessCommand,
    record: &Value,
    base_version: &Value,
    base_version_id: &str,
    staged_editor_blob_id: &str,
    package: &super::office_engine::OfficePackage,
    prepared: &super::office_engine::PreparedEditorPayload,
) -> anyhow::Result<Value> {
    let spreadsheet_id = record
        .get("id")
        .or_else(|| record.get("spreadsheet_id"))
        .and_then(Value::as_str)
        .context("office spreadsheet id is missing")?;
    let command_id = command
        .id
        .as_deref()
        .context("office commit command id is required")?;
    let now = now_ms() as i64;
    let version_number = base_version
        .get("version")
        .and_then(Value::as_i64)
        .unwrap_or_default()
        + 1;
    let version_id = format!(
        "{spreadsheet_id}_office_v{version_number}_{}",
        short_hash(command_id)
    );
    let blob_id = format!("{version_id}_blob");
    let conn = open_store(root)?;
    let replay: Option<String> = conn
        .query_row(
            "SELECT payload_json FROM business_records
             WHERE collection = 'spreadsheet_versions' AND record_id = ?1 AND deleted = 0",
            params![version_id],
            |row| row.get(0),
        )
        .optional()?;
    if let Some(replay) = replay {
        let value: Value = serde_json::from_str(&replay)?;
        return Ok(serde_json::json!({
            "ok": true, "idempotent_replay": true, "operation": "commit",
            "spreadsheet_id": spreadsheet_id, "base_version_id": base_version_id,
            "version_id": version_id,
            "blob_id": value.get("blob_id").and_then(Value::as_str).unwrap_or(&blob_id),
            "editor_blob_id": value.get("editor_blob_id").and_then(Value::as_str).unwrap_or(&blob_id),
            "source_sha256": value.get("source_sha256").cloned().unwrap_or(Value::Null),
            "editor_sha256": value.get("editor_sha256").cloned().unwrap_or(Value::Null),
            "manifest": value.get("office_manifest").cloned().unwrap_or(Value::Null),
            "diagnostics": value.get("diagnostics").cloned().unwrap_or(Value::Null),
        }));
    }

    let mut spreadsheet_payload = record.clone();
    let spreadsheet_object = spreadsheet_payload
        .as_object_mut()
        .context("office spreadsheet payload must be an object")?;
    spreadsheet_object.insert(
        "current_version_id".into(),
        Value::String(version_id.clone()),
    );
    spreadsheet_object.insert("status".into(), Value::String("Draft".into()));
    spreadsheet_object.insert(
        "source_sha256".into(),
        Value::String(package.sha256.clone()),
    );
    spreadsheet_object.insert(
        "index_text".into(),
        Value::String(package.manifest.primary_text.chars().take(20_000).collect()),
    );
    spreadsheet_object.insert("updated_at_ms".into(), Value::from(now));
    let diagnostics = [prepared.diagnostics.clone(), package.diagnostics.clone()].concat();
    let mut version_payload = serde_json::json!({
        "id": version_id, "version_id": version_id, "spreadsheet_id": spreadsheet_id,
        "version": version_number, "source_kind": "office_edited_xlsx", "blob_id": blob_id,
        "base_version_id": base_version_id, "editor_blob_id": staged_editor_blob_id,
        "staged_editor_blob_id": staged_editor_blob_id, "editor_protocol": prepared.protocol,
        "editor_protocol_version": prepared.protocol_version, "source_sha256": package.sha256,
        "editor_sha256": prepared.editor_sha256, "conversion_state": "prepared",
        "implemented_features": prepared.implemented_features, "office_manifest": package.manifest,
        "diagnostics": diagnostics,
        "model_json": {
            "type": "xlsx",
            "text": package.manifest.primary_text.chars().take(20_000).collect::<String>(),
            "parts": package.manifest.parts.len()
        },
        "business_command_id": command.id, "created_at_ms": now, "updated_at_ms": now
    });
    if let Some(version_object) = version_payload.as_object_mut() {
        for key in [
            "ingestion_kind",
            "linked_records",
            "source_receipt_snapshot_hashes",
            "knowledge_version",
            "knowledge_lineage",
        ] {
            if let Some(value) = base_version.get(key).or_else(|| record.get(key)) {
                version_object.insert(key.to_string(), value.clone());
            }
        }
    }

    let tx = conn.unchecked_transaction()?;
    let stored: Option<String> = tx
        .query_row(
            "SELECT payload_json FROM business_records
             WHERE collection = 'spreadsheets' AND record_id = ?1 AND deleted = 0",
            params![spreadsheet_id],
            |row| row.get(0),
        )
        .optional()?;
    let authoritative = stored
        .as_deref()
        .map(serde_json::from_str::<Value>)
        .transpose()?
        .unwrap_or_else(|| record.clone());
    let authoritative_base = authoritative
        .get("current_version_id")
        .and_then(Value::as_str)
        .unwrap_or_default();
    anyhow::ensure!(
        authoritative_base == base_version_id,
        "version_conflict: authoritative current version is {authoritative_base}, expected {base_version_id}"
    );

    let total = package
        .bytes
        .len()
        .div_ceil(DOCUMENT_BLOB_CHUNK_SIZE)
        .max(1);
    let mut chunk_projections = Vec::with_capacity(total);
    for (idx, chunk) in package.bytes.chunks(DOCUMENT_BLOB_CHUNK_SIZE).enumerate() {
        let chunk_id = format!("{blob_id}_{idx:04}");
        let payload = serde_json::json!({
            "id": chunk_id, "blob_id": blob_id, "spreadsheet_id": spreadsheet_id,
            "version_id": version_id, "idx": idx, "total": total,
            "mime_type": super::office_engine::OfficeKind::Spreadsheet.canonical_mime(),
            "encoding": "base64", "data": base64::engine::general_purpose::STANDARD.encode(chunk),
            "created_at_ms": now
        });
        upsert_business_record(
            &tx,
            "spreadsheet_blob_chunks",
            &chunk_id,
            now,
            payload.clone(),
        )?;
        chunk_projections.push((chunk_id, payload));
    }
    upsert_business_record(
        &tx,
        "spreadsheet_versions",
        &version_id,
        now,
        version_payload.clone(),
    )?;
    upsert_business_record(
        &tx,
        "spreadsheets",
        spreadsheet_id,
        now,
        spreadsheet_payload.clone(),
    )?;
    tx.commit()?;

    for (chunk_id, payload) in chunk_projections {
        upsert_rxdb_collection_record(root, "spreadsheet_blob_chunks", &chunk_id, now, payload)?;
    }
    upsert_rxdb_collection_record(
        root,
        "spreadsheet_versions",
        &version_id,
        now,
        version_payload,
    )?;
    upsert_rxdb_collection_record(
        root,
        "spreadsheets",
        spreadsheet_id,
        now,
        spreadsheet_payload,
    )?;

    let conn = open_store(root)?;
    let lower = format!("{blob_id}_");
    let upper = format!("{lower}\u{10ffff}");
    let mut statement = conn.prepare(
        "SELECT payload_json FROM business_records
         WHERE collection = 'spreadsheet_blob_chunks' AND record_id >= ?1 AND record_id < ?2
           AND deleted = 0 ORDER BY record_id",
    )?;
    let rows = statement.query_map(params![lower, upper], |row| row.get::<_, String>(0))?;
    let mut verified_bytes = Vec::with_capacity(package.bytes.len());
    for row in rows {
        let payload: Value = serde_json::from_str(&row?)?;
        let encoded = payload
            .get("data")
            .and_then(Value::as_str)
            .context("spreadsheet blob chunk has no data")?;
        verified_bytes.extend(base64::engine::general_purpose::STANDARD.decode(encoded)?);
    }
    anyhow::ensure!(
        verified_bytes.len() == package.bytes.len()
            && super::office_engine::sha256_hex(&verified_bytes) == package.sha256,
        "office commit integrity verification failed for {blob_id}"
    );
    Ok(serde_json::json!({
        "ok": true, "operation": "commit", "spreadsheet_id": spreadsheet_id,
        "base_version_id": base_version_id, "version_id": version_id, "blob_id": blob_id,
        "editor_blob_id": staged_editor_blob_id, "source_sha256": package.sha256,
        "editor_sha256": prepared.editor_sha256, "bytes": package.bytes.len(), "chunks": total,
        "manifest": package.manifest, "diagnostics": diagnostics
    }))
}

#[cfg(test)]
mod tests {
    use super::super::store::CommandOrigin;
    use super::*;
    use std::fs;

    #[test]
    fn office_prepare_projects_editor_blob_to_live_rxdb_store() -> anyhow::Result<()> {
        use super::super::office_engine::{prepare, OfficeKind, PrepareOptions};

        let root = tempfile::tempdir()?;
        fs::create_dir_all(root.path().join("runtime"))?;
        let rxdb_path = rxdb_store_path(root.path());
        let rxdb = Connection::open(&rxdb_path)?;
        rxdb.execute(
            "CREATE TABLE ctox_business_os__document_blob_chunks__v0 (
                id TEXT PRIMARY KEY NOT NULL,
                revision TEXT,
                deleted INTEGER NOT NULL DEFAULT 0,
                lastWriteTime REAL NOT NULL DEFAULT 0,
                data TEXT NOT NULL
            )",
            [],
        )?;
        drop(rxdb);

        let source =
            include_bytes!("../../../tests/fixtures/office/document/open-render-zoom.docx");
        let prepared = prepare(
            OfficeKind::Document,
            source,
            PrepareOptions {
                implemented_features: vec!["document.open-render-zoom".to_string()],
            },
        )?;
        let blob_id = persist_office_editor_blob(
            root.path(),
            OfficeKind::Document,
            "document_blob_chunks",
            "doc_prepare_projection",
            "doc_prepare_projection_v1",
            &prepared,
        )?;

        let rxdb = Connection::open(&rxdb_path)?;
        let payload: String = rxdb.query_row(
            "SELECT data FROM ctox_business_os__document_blob_chunks__v0
             WHERE json_extract(data, '$.blob_id') = ?1",
            [&blob_id],
            |row| row.get(0),
        )?;
        let payload: Value = serde_json::from_str(&payload)?;
        assert_eq!(
            payload.get("blob_id").and_then(Value::as_str),
            Some(blob_id.as_str())
        );
        assert_eq!(
            payload.get("document_id").and_then(Value::as_str),
            Some("doc_prepare_projection")
        );
        assert!(payload
            .get("data")
            .and_then(Value::as_str)
            .is_some_and(|value| !value.is_empty()));
        Ok(())
    }

    #[test]
    fn frozen_email_blobs_are_content_addressed_and_reload_from_live_rxdb() -> anyhow::Result<()> {
        let root = tempfile::tempdir()?;
        fs::create_dir_all(root.path().join("runtime"))?;
        let rxdb_path = rxdb_store_path(root.path());
        let rxdb = Connection::open(&rxdb_path)?;
        rxdb.execute(
            "CREATE TABLE ctox_business_os__document_blob_chunks__v0 (
                id TEXT PRIMARY KEY NOT NULL,
                revision TEXT,
                deleted INTEGER NOT NULL DEFAULT 0,
                lastWriteTime REAL NOT NULL DEFAULT 0,
                data TEXT NOT NULL
            )",
            [],
        )?;
        drop(rxdb);

        let html = b"<div><strong>Hallo CTOX</strong></div>";
        let html_sha = super::super::office_engine::sha256_hex(html);
        let html_blob_id = persist_document_email_blob(
            root.path(),
            "doc_mail",
            "doc_mail_v1",
            "email_html",
            "text/html; charset=utf-8",
            &html_sha,
            html,
        )?;
        let empty_text_sha = super::super::office_engine::sha256_hex(b"");
        let text_blob_id = persist_document_email_blob(
            root.path(),
            "doc_mail",
            "doc_mail_v1",
            "email_text",
            "text/plain; charset=utf-8",
            &empty_text_sha,
            b"",
        )?;

        assert!(html_blob_id.contains(&html_sha[..12]));
        assert_eq!(
            load_rxdb_office_blob(root.path(), "document_blob_chunks", &html_blob_id)?,
            html
        );
        assert_eq!(
            load_rxdb_office_blob(root.path(), "document_blob_chunks", &text_blob_id)?,
            Vec::<u8>::new()
        );
        let rxdb = Connection::open(&rxdb_path)?;
        let payload: String = rxdb.query_row(
            "SELECT data FROM ctox_business_os__document_blob_chunks__v0
             WHERE json_extract(data, '$.blob_id') = ?1",
            [&html_blob_id],
            |row| row.get(0),
        )?;
        let payload: Value = serde_json::from_str(&payload)?;
        assert_eq!(
            payload.get("mime_type").and_then(Value::as_str),
            Some("text/html; charset=utf-8")
        );
        assert_eq!(
            payload.get("version_id").and_then(Value::as_str),
            Some("doc_mail_v1")
        );
        Ok(())
    }

    #[test]
    fn office_commits_survive_store_reopen_and_spreadsheets_never_write_document_collections(
    ) -> anyhow::Result<()> {
        use super::super::office_engine::{export, prepare, OfficeKind, PrepareOptions};

        let root = tempfile::tempdir()?;
        let source =
            include_bytes!("../../../tests/fixtures/office/spreadsheet/open-render-sheets.xlsx");
        let prepared = prepare(
            OfficeKind::Spreadsheet,
            source,
            PrepareOptions {
                implemented_features: vec!["spreadsheet.open-render-sheets".to_string()],
            },
        )?;
        let package = export(
            OfficeKind::Spreadsheet,
            &prepared.editor_payload,
            Some(source),
        )?;
        let record = serde_json::json!({
            "id": "sheet_restart", "title": "Restart workbook", "filename": "restart.xlsx",
            "mime_type": OfficeKind::Spreadsheet.canonical_mime(), "status": "Imported",
            "current_version_id": "sheet_restart_v1", "source_sha256": prepared.source_sha256,
            "tags": [], "index_text": "Restart workbook", "created_at_ms": 1, "updated_at_ms": 1
        });
        let base_version = serde_json::json!({
            "id": "sheet_restart_v1", "spreadsheet_id": "sheet_restart", "version": 1,
            "blob_id": "sheet_restart_blob", "editor_blob_id": "sheet_restart_editor",
            "editor_sha256": prepared.editor_sha256,
            "ingestion_kind": "research_generated",
            "linked_records": [{"kind": "source_receipt", "id": "source-7"}],
            "source_receipt_snapshot_hashes": [format!("sha256:{}", "e".repeat(64))],
            "knowledge_version": {"version_id": "knowledge-v7"},
            "knowledge_lineage": {"domain": "bearing_design"},
            "created_at_ms": 1, "updated_at_ms": 1
        });
        {
            let conn = open_store(root.path())?;
            upsert_business_record(&conn, "spreadsheets", "sheet_restart", 1, record.clone())?;
            upsert_business_record(
                &conn,
                "spreadsheet_versions",
                "sheet_restart_v1",
                1,
                base_version.clone(),
            )?;
        }

        let command = BusinessCommand {
            origin: CommandOrigin::TrustedLocal,
            id: Some("cmd_sheet_restart_commit".into()),
            module: "spreadsheets".into(),
            command_type: "office.spreadsheet.commit".into(),
            record_id: Some("sheet_restart".into()),
            payload: Value::Null,
            client_context: Value::Null,
        };
        let outcome = commit_office_spreadsheet_version(
            root.path(),
            &command,
            &record,
            &base_version,
            "sheet_restart_v1",
            "sheet_restart_editor",
            &package,
            &prepared,
        )?;
        assert_eq!(outcome.get("ok").and_then(Value::as_bool), Some(true));
        assert_eq!(
            outcome.get("spreadsheet_id").and_then(Value::as_str),
            Some("sheet_restart")
        );
        let committed_version = outcome
            .get("version_id")
            .and_then(Value::as_str)
            .context("committed spreadsheet version")?;
        let conn = open_store(root.path())?;
        let spreadsheet: String = conn.query_row(
            "SELECT payload_json FROM business_records WHERE collection = 'spreadsheets' AND record_id = 'sheet_restart'", [], |row| row.get(0),
        )?;
        let spreadsheet: Value = serde_json::from_str(&spreadsheet)?;
        assert_eq!(
            spreadsheet
                .get("current_version_id")
                .and_then(Value::as_str),
            Some(committed_version)
        );
        let spreadsheet_versions: i64 = conn.query_row(
            "SELECT COUNT(*) FROM business_records WHERE collection = 'spreadsheet_versions' AND record_id = ?1", params![committed_version], |row| row.get(0),
        )?;
        let leaked_document_versions: i64 = conn.query_row(
            "SELECT COUNT(*) FROM business_records WHERE collection = 'document_versions' AND record_id = ?1", params![committed_version], |row| row.get(0),
        )?;
        assert_eq!(spreadsheet_versions, 1);
        assert_eq!(leaked_document_versions, 0);
        let committed_payload: String = conn.query_row(
            "SELECT payload_json FROM business_records WHERE collection = 'spreadsheet_versions' AND record_id = ?1",
            params![committed_version],
            |row| row.get(0),
        )?;
        let committed_payload: Value = serde_json::from_str(&committed_payload)?;
        assert_eq!(
            committed_payload
                .get("ingestion_kind")
                .and_then(Value::as_str),
            Some("research_generated")
        );
        assert_eq!(
            committed_payload
                .pointer("/knowledge_version/version_id")
                .and_then(Value::as_str),
            Some("knowledge-v7")
        );
        assert_eq!(
            committed_payload
                .get("source_receipt_snapshot_hashes")
                .and_then(Value::as_array)
                .map(Vec::len),
            Some(1)
        );
        drop(conn);

        let replay = commit_office_spreadsheet_version(
            root.path(),
            &command,
            &record,
            &base_version,
            "sheet_restart_v1",
            "sheet_restart_editor",
            &package,
            &prepared,
        )?;
        assert_eq!(
            replay.get("idempotent_replay").and_then(Value::as_bool),
            Some(true)
        );

        let document_source =
            include_bytes!("../../../tests/fixtures/office/document/open-render-zoom.docx");
        let document_prepared = prepare(
            OfficeKind::Document,
            document_source,
            PrepareOptions {
                implemented_features: vec!["document.open-render-zoom".to_string()],
            },
        )?;
        let document_package = export(
            OfficeKind::Document,
            &document_prepared.editor_payload,
            Some(document_source),
        )?;
        let document_record = serde_json::json!({
            "id": "doc_restart", "title": "Restart document", "filename": "restart.docx",
            "mime_type": OfficeKind::Document.canonical_mime(), "document_type": "word_document",
            "status": "Imported", "current_version_id": "doc_restart_v1",
            "source_sha256": document_prepared.source_sha256, "tags": [], "index_text": "Restart document",
            "created_at_ms": 1, "updated_at_ms": 1
        });
        let document_base = serde_json::json!({
            "id": "doc_restart_v1", "document_id": "doc_restart", "version": 1,
            "blob_id": "doc_restart_blob", "editor_blob_id": "doc_restart_editor",
            "editor_sha256": document_prepared.editor_sha256, "created_at_ms": 1, "updated_at_ms": 1
        });
        {
            let conn = open_store(root.path())?;
            upsert_business_record(
                &conn,
                "documents",
                "doc_restart",
                1,
                document_record.clone(),
            )?;
            upsert_business_record(
                &conn,
                "document_versions",
                "doc_restart_v1",
                1,
                document_base.clone(),
            )?;
        }
        let document_command = BusinessCommand {
            origin: CommandOrigin::TrustedLocal,
            id: Some("cmd_doc_restart_commit".into()),
            module: "documents".into(),
            command_type: "office.document.commit".into(),
            record_id: Some("doc_restart".into()),
            payload: Value::Null,
            client_context: Value::Null,
        };
        let document_outcome = commit_office_document_version(
            root.path(),
            &document_command,
            &document_record,
            &document_base,
            "doc_restart_v1",
            "doc_restart_editor",
            &document_package,
            &document_prepared,
        )?;
        assert_eq!(
            document_outcome.get("ok").and_then(Value::as_bool),
            Some(true)
        );
        assert_eq!(
            document_outcome.get("document_id").and_then(Value::as_str),
            Some("doc_restart")
        );
        Ok(())
    }
}
