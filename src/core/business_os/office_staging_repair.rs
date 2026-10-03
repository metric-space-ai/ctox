// Origin: CTOX
// License: AGPL-3.0-only

//! Explicit, offline removal of corrupted Office upload staging records.
//! Canonical version files and historical command evidence are never deleted.
use super::office_engine::sha256_hex;
use super::store::{business_os_store_path, rxdb_store_path, BusinessProjectionWriter};
use super::store_office_commands::load_rxdb_office_blob;
use anyhow::Context;
use rusqlite::{Connection, OpenFlags};
use serde_json::{json, Value};
use std::{collections::BTreeSet, fs, io::Write, path::Path};

fn records(conn: &Connection, collection: &str) -> anyhow::Result<Vec<Value>> {
    let table = match collection {
        "documents" => "ctox_business_os__documents__v0",
        "document_versions" => "ctox_business_os__document_versions__v0",
        "document_blob_chunks" => "ctox_business_os__document_blob_chunks__v0",

        _ => anyhow::bail!("unsupported staging repair collection"),
    };
    // Include invalid deleted chunks: the normal replay validates tombstones.
    let selection = if collection == "document_blob_chunks" {
        ""
    } else {
        " WHERE deleted=0"
    };
    let mut stmt = conn.prepare(&format!("SELECT data FROM {table}{selection} ORDER BY id"))?;
    let rows = stmt.query_map([], |r| r.get::<_, String>(0))?;
    rows.map(|r| Ok(serde_json::from_str(&r?)?)).collect()
}

fn required<'a>(record: &'a Value, field: &str) -> anyhow::Result<&'a str> {
    record
        .get(field)
        .and_then(Value::as_str)
        .filter(|s| !s.is_empty())
        .with_context(|| format!("missing {field} in Office staging audit"))
}

fn audit(root: &Path) -> anyhow::Result<(Vec<Value>, Value)> {
    let conn =
        Connection::open_with_flags(rxdb_store_path(root), OpenFlags::SQLITE_OPEN_READ_ONLY)?;
    conn.execute_batch("BEGIN")?;
    let chunks = records(&conn, "document_blob_chunks")?;
    let candidates: Vec<Value> = chunks
        .into_iter()
        .filter(|r| {
            r.get("data")
                .and_then(Value::as_object)
                .is_some_and(|d| d.get("_omitted") == Some(&Value::Bool(true)))
        })
        .collect();
    let mut blobs = BTreeSet::new();
    let mut documents = BTreeSet::new();
    for c in &candidates {
        let blob = required(c, "blob_id")?;
        anyhow::ensure!(
            blob.starts_with("office_document_")
                && c["idx"] == 0
                && c["total"] == 1
                && required(c, "id")? == format!("{blob}_0000"),
            "refusing unsupported damaged record shape; no records removed"
        );
        anyhow::ensure!(
            c["created_at_ms"]
                .as_i64()
                .is_some_and(|t| t >= 0 && t < chrono::Utc::now().timestamp_millis() - 3_600_000),
            "refusing recent or undated staging data"
        );
        blobs.insert(blob.to_owned());
        documents.insert(required(c, "document_id")?.to_owned());
    }
    let versions = records(&conn, "document_versions")?;
    let docs = records(&conn, "documents")?;
    let mut verified = Vec::new();
    for id in &documents {
        let doc = docs
            .iter()
            .find(|d| d["id"].as_str() == Some(id))
            .context("damaged staging record has no document")?;
        let current = required(doc, "current_version_id")?;
        anyhow::ensure!(versions.iter().any(|v| v["id"].as_str() == Some(current)
            && v["document_id"].as_str() == Some(id)), "current document version missing");
    }
    for v in &versions {
        for key in ["blob_id", "editor_blob_id"] {
            anyhow::ensure!(
                !v.get(key)
                    .and_then(Value::as_str)
                    .is_some_and(|s| blobs.contains(s)),
                "damaged blob is still a canonical version file"
            );
        }
        if !v
            .get("document_id")
            .and_then(Value::as_str)
            .is_some_and(|s| documents.contains(s))
        {
            continue;
        }
        for (key, hash_key) in [
            ("blob_id", "source_sha256"),
            ("editor_blob_id", "editor_sha256"),
        ] {
            let blob = required(v, key)?;
            let bytes = load_rxdb_office_blob(root, "document_blob_chunks", blob)?;
            let digest = sha256_hex(&bytes);
            anyhow::ensure!(
                digest == required(v, hash_key)?,
                "canonical Office file hash mismatch: {blob}"
            );
            verified.push(json!({"version": required(v, "id")?, "field": key,
                "blob_id": blob, "sha256": digest, "bytes": bytes.len()}));
        }
    }
    let commands = Connection::open_with_flags(
        business_os_store_path(root),
        OpenFlags::SQLITE_OPEN_READ_ONLY,
    )?;
    let mut stmt =
        commands.prepare("SELECT command_id,status,payload_json FROM business_commands")?;
    let rows = stmt.query_map([], |r| {
        Ok((
            r.get::<_, String>(0)?,
            r.get::<_, String>(1)?,
            r.get::<_, String>(2)?,
        ))
    })?;
    for row in rows {
        let (command_id, status, encoded) = row?;
        if blobs.iter().any(|b| encoded.contains(b)) {
            anyhow::ensure!(
                matches!(
                    status.as_str(),
                    "completed" | "failed" | "cancelled" | "canceled" | "rejected" | "expired"
                ),
                "nonterminal command still references damaged staging: {command_id}"
            );
        }
    }
    let digest = sha256_hex(&serde_json::to_vec(&candidates)?);
    let report = json!({"schema": "ctox.office_staging_repair.v1",
        "candidate_count": candidates.len(), "candidate_sha256": digest,
        "chunk_ids": candidates.iter().map(|v| v["id"].clone()).collect::<Vec<_>>(),
        "verified_canonical_files": verified, "applied": false});
    Ok((candidates, report))
}

/// Apply requires the native peer to be stopped and an exact dry-run digest.
/// The normal projection writer publishes tombstones to both durable stores.
pub fn repair(root: &Path, apply: bool, expected_sha256: Option<&str>) -> anyhow::Result<Value> {
    let _peer_lock = if apply {
        Some(
            super::rxdb_peer::acquire_native_peer_process_lock(root)?
                .context("stop the native peer before applying Office staging repair")?,
        )
    } else {
        None
    };
    let (candidates, mut report) = audit(root)?;
    if !apply {
        return Ok(report);
    }
    anyhow::ensure!(
        expected_sha256 == report["candidate_sha256"].as_str(),
        "candidate digest changed or missing; run --dry-run again"
    );
    if candidates.is_empty() {
        report["applied"] = json!(true);
        return Ok(report);
    }
    let dir = root.join("runtime/office-staging-repair");
    fs::create_dir_all(&dir)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&dir, fs::Permissions::from_mode(0o700))?;
    }
    let backup = dir.join(format!("{}.json", required(&report, "candidate_sha256")?));
    let backup_bytes =
        serde_json::to_vec_pretty(&json!({"report": report, "records": candidates}))?;
    match fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&backup)
    {
        Ok(mut file) => {
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                file.set_permissions(fs::Permissions::from_mode(0o600))?;
            }
            file.write_all(&backup_bytes)?;
            file.sync_all()?;
        }
        Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {
            anyhow::ensure!(
                fs::read(&backup)? == backup_bytes,
                "existing repair backup differs"
            );
        }
        Err(e) => return Err(e.into()),
    }
    #[cfg(unix)]
    fs::File::open(&dir)?.sync_all()?;
    let mut writer = BusinessProjectionWriter::open(root)?;
    let now = chrono::Utc::now().timestamp_millis();
    for c in &candidates {
        writer.tombstone_source_projection("document_blob_chunks", required(c, "id")?, now)?;
        anyhow::ensure!(
            writer.delivered_to_rxdb("document_blob_chunks"),
            "tombstone delivery deferred"
        );
    }
    let (remaining, _) = audit(root)?;
    anyhow::ensure!(
        remaining.is_empty(),
        "damaged active staging remains after repair"
    );
    report["applied"] = json!(true);
    report["backup"] = json!(backup);
    Ok(report)
}

#[cfg(test)]
mod tests {
    use super::*;
    use base64::Engine;
    use rusqlite::params;
    use tempfile::TempDir;

    fn fixture() -> anyhow::Result<TempDir> {
        let root = tempfile::tempdir()?;
        fs::create_dir_all(root.path().join("runtime"))?;
        let conn = Connection::open(rxdb_store_path(root.path()))?;
        for collection in ["documents", "document_versions", "document_blob_chunks"] {
            conn.execute_batch(&format!(
                "CREATE TABLE ctox_business_os__{collection}__v0 (
                id TEXT PRIMARY KEY NOT NULL, revision TEXT, deleted INTEGER NOT NULL DEFAULT 0,
                lastWriteTime REAL NOT NULL DEFAULT 0, data TEXT NOT NULL)"
            ))?;
        }
        let mut writer = BusinessProjectionWriter::open(root.path())?;
        for (id, blob, data) in [
            (
                "canonical_0000",
                "canonical",
                json!(base64::engine::general_purpose::STANDARD.encode(b"canonical-docx")),
            ),
            (
                "editor_0000",
                "editor",
                json!(base64::engine::general_purpose::STANDARD.encode(b"canonical-editor")),
            ),
            (
                "office_document_old_0000",
                "office_document_old",
                json!({"_omitted":true,"_omitted_bytes":324210}),
            ),
        ] {
            writer.upsert_source_projection(
                "document_blob_chunks",
                id,
                1000,
                json!({"id":id,"blob_id":blob,"document_id":"doc","version_id":"v1",
                    "idx":0,"total":1,"encoding":"base64","mime_type":"application/octet-stream","data":data,"created_at_ms":1000}),
            )?;
        }
        writer.upsert_source_projection(
            "documents",
            "doc",
            1000,
            json!({"id":"doc","current_version_id":"v1","title":"Preserve me"}),
        )?;
        writer.upsert_source_projection(
            "document_versions",
            "v1",
            1000,
            json!({"id":"v1","document_id":"doc","blob_id":"canonical","editor_blob_id":"editor",
                "staged_editor_blob_id":"office_document_old",
                "source_sha256":sha256_hex(b"canonical-docx"),
                "editor_sha256":sha256_hex(b"canonical-editor")}),
        )?;
        Ok(root)
    }

    #[test]
    fn repair_tombstones_only_confirmed_staging_and_preserves_canonical_files() -> anyhow::Result<()>
    {
        let root = fixture()?;
        let before = audit(root.path())?.0;
        let dry = repair(root.path(), false, None)?;
        assert_eq!(dry["candidate_count"], 1);
        assert_eq!(dry["verified_canonical_files"].as_array().unwrap().len(), 2);
        assert_eq!(audit(root.path())?.0, before, "dry run must not mutate");
        let result = repair(root.path(), true, dry["candidate_sha256"].as_str())?;
        assert_eq!(result["applied"], true);
        let saved: Value = serde_json::from_slice(&fs::read(result["backup"].as_str().unwrap())?)?;
        assert_eq!(saved["records"], json!(before));
        let conn = Connection::open(rxdb_store_path(root.path()))?;
        let deleted: i64 = conn.query_row(
            "SELECT deleted FROM ctox_business_os__document_blob_chunks__v0 WHERE id='office_document_old_0000'",
            [], |r| r.get(0))?;
        assert_eq!(deleted, 1);
        assert_eq!(
            load_rxdb_office_blob(root.path(), "document_blob_chunks", "canonical")?,
            b"canonical-docx"
        );
        assert_eq!(
            load_rxdb_office_blob(root.path(), "document_blob_chunks", "editor")?,
            b"canonical-editor"
        );
        let empty = repair(root.path(), false, None)?;
        assert_eq!(empty["candidate_count"], 0);
        assert_eq!(
            repair(root.path(), true, empty["candidate_sha256"].as_str())?["applied"],
            true
        );
        Ok(())
    }

    #[test]
    fn repair_refuses_live_peer_and_changed_candidate_digest() -> anyhow::Result<()> {
        let root = fixture()?;
        let dry = repair(root.path(), false, None)?;
        let guard =
            super::super::rxdb_peer::acquire_native_peer_process_lock(root.path())?.unwrap();
        assert!(repair(root.path(), true, dry["candidate_sha256"].as_str())
            .unwrap_err()
            .to_string()
            .contains("stop the native peer"));
        drop(guard);
        assert!(repair(root.path(), true, Some("stale")).is_err());
        assert_eq!(audit(root.path())?.0.len(), 1);
        Ok(())
    }

    #[test]
    fn repair_refuses_missing_or_corrupt_canonical_data() -> anyhow::Result<()> {
        let root = fixture()?;
        let mut writer = BusinessProjectionWriter::open(root.path())?;
        writer.tombstone_source_projection("document_blob_chunks", "canonical_0000", 2000)?;
        assert!(repair(root.path(), false, None).is_err());
        let conn = Connection::open(rxdb_store_path(root.path()))?;
        assert_eq!(conn.query_row(
            "SELECT deleted FROM ctox_business_os__document_blob_chunks__v0 WHERE id='office_document_old_0000'",
            [], |r| r.get::<_,i64>(0))?, 0);
        Ok(())
    }

    #[tokio::test]
    async fn repaired_tombstone_passes_native_projection_replay() -> anyhow::Result<()> {
        let root = fixture()?;
        let dry = repair(root.path(), false, None)?;
        repair(root.path(), true, dry["candidate_sha256"].as_str())?;
        let conn = Connection::open(rxdb_store_path(root.path()))?;
        let raw: String = conn.query_row(
            "SELECT data FROM ctox_business_os__document_blob_chunks__v0 WHERE id='office_document_old_0000'",
            [], |r| r.get(0))?;
        let tombstone: Value = serde_json::from_str(&raw)?;
        assert_eq!(tombstone["data"], "");
        let schema = super::super::rxdb_peer::business_os_schema("document_blob_chunks", "id");
        rxdb::rx_schema::validate_write_document(&schema, "id", &tombstone)
            .map_err(anyhow::Error::msg)?;
        let db = super::super::rxdb_peer::open_database(rxdb_store_path(root.path())).await?;
        let creator = rxdb::rx_database::RxCollectionCreator {
            schema,
            conflict_handler: None,
            options: std::collections::HashMap::new(),
        };
        db.add_collections(std::collections::HashMap::from([(
            "document_blob_chunks".to_string(),
            creator,
        )]))
        .await?;
        let collection = db.collection("document_blob_chunks").unwrap();
        super::super::rxdb_peer::upsert_business_record_projection_tombstone(
            &collection,
            json!({"id":"office_document_old_0000","_deleted":true}),
        )
        .await?;
        db.close().await?;
        Ok(())
    }

    #[test]
    fn repair_refuses_nonterminal_command_references() -> anyhow::Result<()> {
        let root = fixture()?;
        let conn = super::super::store::open_store(root.path())?;
        conn.execute("INSERT INTO business_commands
            (command_id,module,command_type,record_id,status,payload_json,client_context_json,observed_at_ms)
            VALUES (?1,'documents','office.document.apply_changes','doc',?2,?3,'{}',1000)",
            params!["active","running",json!({"blob_id":"office_document_old"}).to_string()])?;
        assert!(repair(root.path(), false, None)
            .unwrap_err()
            .to_string()
            .contains("nonterminal"));
        conn.execute(
            "UPDATE business_commands SET status='failed' WHERE command_id='active'",
            [],
        )?;
        assert_eq!(repair(root.path(), false, None)?["candidate_count"], 1);
        Ok(())
    }
}
