//! Explicit local-operator publication into the existing native file service.
//! This produces source metadata, not a transfer grant or execution authority.
use super::{
    rxdb_peer::materialize_desktop_file_from_path,
    rxdb_peer_desktop_files::{
        desktop_file_id, desktop_file_transfer_metadata, ensure_safe_desktop_file_index_path,
    },
    store,
};
use anyhow::{ensure, Context, Result};
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::{fs::File, io::Read, path::Path};

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct PublishedNativeFile {
    pub file_id: String,
    pub sha256: String,
    pub size: u64,
    pub source_instance_id: String,
    pub source_public_identity: String,
}

/// Only the local CLI calls this. Remote readers still need native device
/// admission, the existing file-read policy, and their exact-scope grant.
pub(crate) fn publish_native_file(root: &Path, source: &Path) -> Result<PublishedNativeFile> {
    let source = source
        .canonicalize()
        .context("cannot resolve native publication file")?;
    ensure!(
        source.to_str().is_some(),
        "native publication requires a UTF-8 path"
    );
    ensure_safe_desktop_file_index_path(&source, "native publication file")?;
    ensure!(
        source.is_file(),
        "native publication requires a regular file"
    );
    let mut file = File::open(&source)?;
    let mut hash = Sha256::new();
    let mut size = 0_u64;
    let mut buffer = [0_u8; 64 * 1024];
    loop {
        let count = file.read(&mut buffer)?;
        if count == 0 {
            break;
        }
        hash.update(&buffer[..count]);
        size = size
            .checked_add(count as u64)
            .context("native publication file too large")?;
    }
    let sha256 = format!("{:x}", hash.finalize());
    // Reuse the production writer, full chunk verification and path boundary.
    // Eager materialization is essential for bundles above the normal lazy limit.
    materialize_desktop_file_from_path(root, &source)?;
    let file_id = desktop_file_id(&source);
    let published = desktop_file_transfer_metadata(root, &file_id)?
        .context("native file publication did not produce transferable metadata")?;
    ensure!(
        published.size_bytes == size && published.content_hash == sha256,
        "native publication changed while materializing; retry the quiescent file"
    );
    Ok(PublishedNativeFile {
        file_id,
        sha256,
        size,
        source_instance_id: store::sync_connection_config(root)?.instance_id,
        source_public_identity: crate::sync_host::signing_identity(root)?.public_identity(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn explicit_publication_materializes_large_and_empty_git_artifacts_durably() {
        let fixture = tempfile::tempdir().unwrap();
        let root = fixture.path().join("instance");
        let artifacts = fixture.path().join("artifacts");
        std::fs::create_dir_all(&root).unwrap();
        crate::sync_host::handle_command(&root, &["init".into()]).unwrap();
        std::fs::create_dir_all(&artifacts).unwrap();
        for (name, bytes) in [
            ("bundle.gitbundle", vec![0x5a; 1024 * 1024 + 17]),
            ("index.patch", vec![]),
        ] {
            let path = artifacts.join(name);
            std::fs::write(&path, &bytes).unwrap();
            let first = publish_native_file(&root, &path).unwrap();
            assert_eq!(first.sha256, format!("{:x}", Sha256::digest(&bytes)));
            assert_eq!(first.size, bytes.len() as u64);
            // Reopen through the actual source read path after the temporary
            // native database owner has closed; no in-memory fixture table.
            let stored = desktop_file_transfer_metadata(&root, &first.file_id)
                .unwrap()
                .unwrap();
            assert_eq!(stored.content_hash, first.sha256);
            assert_eq!(stored.size_bytes, first.size);
            assert!(!stored.generation_id.is_empty());
            // Read the durable generation through the native demand-file
            // source, including the final chunk beyond the first write batch.
            let (rows, offset) =
                super::super::rxdb_peer_desktop_files::active_desktop_file_chunk_rows_from_sqlite(
                    &root,
                    &first.file_id,
                    None,
                    &mut super::super::rxdb_peer::DemandFileFetchRequestStats::default(),
                )
                .unwrap();
            assert_eq!(offset, 0);
            let mut encoded = String::new();
            for (idx, row) in rows.iter().enumerate() {
                assert_eq!(row["idx"].as_u64(), Some(idx as u64));
                let data = row["data"].as_str().unwrap();
                assert_eq!(
                    row["chunk_hash"].as_str(),
                    Some(format!("{:x}", Sha256::digest(data.as_bytes())).as_str())
                );
                encoded.push_str(data);
            }
            use base64::Engine;
            assert_eq!(
                base64::engine::general_purpose::STANDARD
                    .decode(encoded)
                    .unwrap(),
                bytes
            );
            let second = publish_native_file(&root, &path).unwrap();
            assert_eq!(first.file_id, second.file_id);
            assert_eq!(first.source_public_identity, second.source_public_identity);
            assert_eq!(first.source_instance_id, second.source_instance_id);
            assert_eq!(first.sha256, second.sha256);
        }
    }

    #[test]
    fn directory_publication_fails_before_native_store_creation() {
        let fixture = tempfile::tempdir().unwrap();
        let root = fixture.path().join("instance");
        assert!(publish_native_file(&root, fixture.path()).is_err());
        assert!(!store::rxdb_store_path(&root).exists());
    }

    #[test]
    fn failed_temporary_native_operation_closes_its_database() {
        use super::super::rxdb_peer::{with_business_os_database, TemporaryDatabaseLockScope};
        let fixture = tempfile::tempdir().unwrap();
        let observed = std::sync::Arc::new(std::sync::OnceLock::new());
        let capture = observed.clone();
        let result: Result<()> = with_business_os_database(
            fixture.path(),
            "publication error cleanup fixture",
            true,
            TemporaryDatabaseLockScope::TemporaryOnly,
            move |peer, database| async move {
                assert!(peer.is_none());
                assert!(capture.set(database).is_ok());
                anyhow::bail!("injected publication failure")
            },
        );
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("injected publication failure"));
        assert!(observed.get().unwrap().closed());
    }
}
