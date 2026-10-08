// Origin: CTOX
// License: AGPL-3.0-only

use super::*;

fn source(root: &std::path::Path) -> Result<(tempfile::NamedTempFile, QemuMemoryState)> {
    let mut file = tempfile::NamedTempFile::new_in(root)?;
    let mut hash = Sha256::new();
    for (count, byte) in [
        (CHUNK_BYTES as usize, 0x41),
        (CHUNK_BYTES as usize, 0x42),
        (17, 0x43),
    ] {
        let bytes = vec![byte; count];
        file.write_all(&bytes)?;
        hash.update(&bytes);
    }
    file.as_file().sync_all()?;
    file.as_file()
        .set_permissions(std::fs::Permissions::from_mode(0o400))?;
    Ok((
        file,
        QemuMemoryState {
            bytes: 2 * CHUNK_BYTES + 17,
            sha256: format!("{:x}", hash.finalize()),
        },
    ))
}

#[test]
fn chunked_memory_roundtrip_preserves_exact_content_and_readonly_staging() -> Result<()> {
    let root = tempfile::tempdir()?;
    let store = CheckpointStore::open(root.path().join("store"), 64 * 1024 * 1024)?;
    let (mut source, memory) = source(root.path())?;
    let entries = store_memory_chunks(&store, source.as_file_mut(), &memory)?;
    ensure!(
        entries.len() == 4,
        "memory was not split into ordered bounded parts"
    );
    ensure!(entries
        .iter()
        .all(|entry| entry.artifact.size_bytes <= CHUNK_BYTES));
    let mut staged = tempfile::NamedTempFile::new_in(root.path())?;
    ensure!(load_memory_chunks(&store, &entries, staged.as_file_mut())? == memory);
    ensure!(staged.as_file().metadata()?.mode() & 0o777 == 0o400);
    staged.rewind()?;
    let mut hash = Sha256::new();
    let mut buffer = [0u8; 128 * 1024];
    loop {
        let count = staged.read(&mut buffer)?;
        if count == 0 {
            break;
        }
        hash.update(&buffer[..count]);
    }
    ensure!(format!("{:x}", hash.finalize()) == memory.sha256);
    Ok(())
}

#[test]
fn missing_duplicate_or_reordered_memory_cannot_complete_staging() -> Result<()> {
    let root = tempfile::tempdir()?;
    let store = CheckpointStore::open(root.path().join("store"), 64 * 1024 * 1024)?;
    let (mut source, memory) = source(root.path())?;
    let original = store_memory_chunks(&store, source.as_file_mut(), &memory)?;
    for case in ["missing", "duplicate", "unexpected", "reordered"] {
        let mut entries = original.clone();
        match case {
            "missing" => {
                entries.remove(0);
            }
            "duplicate" => entries.push(entries[0].clone()),
            "unexpected" => entries.push(entry(chunk_path(99), entries[0].artifact.clone())),
            "reordered" => {
                // The two full chunks have equal lengths. Rebind both the
                // metadata and paths so only the complete content hash catches it.
                let metadata = entries
                    .iter()
                    .find(|entry| entry.path == METADATA_PATH)
                    .unwrap();
                let mut bytes = Vec::new();
                store
                    .open_blob(&metadata.artifact)?
                    .read_to_end(&mut bytes)?;
                let mut manifest: MemoryManifest = serde_json::from_slice(&bytes)?;
                manifest.chunks.swap(0, 1);
                let metadata_artifact = artifact(&store, &serde_json::to_vec(&manifest)?)?;
                for entry in &mut entries {
                    if entry.path == METADATA_PATH {
                        entry.artifact = metadata_artifact.clone();
                    }
                    for index in 0..manifest.chunks.len() {
                        if entry.path == chunk_path(index) {
                            entry.artifact = manifest.chunks[index].clone();
                        }
                    }
                }
            }
            _ => unreachable!(),
        }
        let mut staged = tempfile::NamedTempFile::new_in(root.path())?;
        ensure!(
            load_memory_chunks(&store, &entries, staged.as_file_mut()).is_err(),
            "{case} accepted"
        );
        ensure!(
            staged.as_file().metadata()?.mode() & 0o200 != 0,
            "{case} published readonly completed state"
        );
    }
    Ok(())
}

#[test]
fn changed_cas_blob_and_wrong_source_digest_are_rejected() -> Result<()> {
    let root = tempfile::tempdir()?;
    let store_path = root.path().join("store");
    let store = CheckpointStore::open(store_path.clone(), 64 * 1024 * 1024)?;
    let (mut source, memory) = source(root.path())?;
    let entries = store_memory_chunks(&store, source.as_file_mut(), &memory)?;
    let wrong = QemuMemoryState {
        bytes: memory.bytes,
        sha256: "0".repeat(64),
    };
    ensure!(
        store_memory_chunks(&store, source.as_file_mut(), &wrong).is_err(),
        "wrong source digest accepted"
    );
    std::fs::write(
        store_path.join("blobs").join(&entries[0].artifact.sha256),
        b"corrupt chunk",
    )?;
    let mut staged = tempfile::NamedTempFile::new_in(root.path())?;
    ensure!(
        load_memory_chunks(&store, &entries, staged.as_file_mut()).is_err(),
        "corrupt CAS input completed"
    );
    ensure!(staged.as_file().metadata()?.mode() & 0o200 != 0);
    Ok(())
}
