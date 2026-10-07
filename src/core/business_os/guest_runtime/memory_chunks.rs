// Origin: CTOX
// License: AGPL-3.0-only

//! Bounded memory artifacts in the existing protected checkpoint CAS.
//! The native lifecycle caller owns source quiescence, the completed guest
//! import/manifest binding, current authority and the disk/profile contract.
//! These helpers only materialize bytes; they never activate or grant a guest.

use super::QemuMemoryState;
use anyhow::{ensure, Context, Result};
use ctox_sync::{
    checkpoint::CheckpointStore,
    contracts::{ArtifactRef, WorkspaceEntry, WorkspaceEntryKind},
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    fs::File,
    io::{Read, Seek, Write},
    os::unix::fs::{MetadataExt, PermissionsExt},
};

const CHUNK_BYTES: u64 = 8 * 1024 * 1024;
const MAX_MEMORY_BYTES: u64 = 1024 * 1024 * 1024;
const MAX_METADATA_BYTES: u64 = 64 * 1024;
const METADATA_PATH: &str = "native-guest-memory.json";
const CHUNK_PREFIX: &str = "native-guest-memory/";

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
struct MemoryManifest {
    version: u32,
    bytes: u64,
    sha256: String,
    chunks: Vec<ArtifactRef>,
}

fn valid_hash(hash: &str) -> bool {
    hash.len() == 64
        && hash
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}

fn private_file(file: &File) -> Result<std::fs::Metadata> {
    let metadata = file.metadata()?;
    // SAFETY: geteuid only reads this native process's effective user ID.
    ensure!(
        metadata.is_file()
            && metadata.uid() == unsafe { libc::geteuid() }
            && metadata.mode() & 0o077 == 0
            && metadata.nlink() == 1,
        "guest memory file must be private, native-owned and unaliased"
    );
    Ok(metadata)
}

fn artifact(store: &CheckpointStore, bytes: &[u8]) -> Result<ArtifactRef> {
    let artifact = ArtifactRef {
        sha256: format!("{:x}", Sha256::digest(bytes)),
        size_bytes: bytes.len() as u64,
    };
    store.ingest_blob(&artifact, bytes)?;
    Ok(artifact)
}

fn entry(path: String, artifact: ArtifactRef) -> WorkspaceEntry {
    WorkspaceEntry {
        path,
        kind: WorkspaceEntryKind::File,
        artifact,
        executable: false,
    }
}

fn chunk_path(index: usize) -> String {
    format!("{CHUNK_PREFIX}{index:04}.bin")
}

impl MemoryManifest {
    fn validate(&self) -> Result<()> {
        ensure!(
            self.version == 1
                && (1..=MAX_MEMORY_BYTES).contains(&self.bytes)
                && valid_hash(&self.sha256),
            "guest memory metadata version, size or digest is invalid"
        );
        let count = ((self.bytes - 1) / CHUNK_BYTES + 1) as usize;
        ensure!(
            self.chunks.len() == count,
            "guest memory chunk count is incomplete"
        );
        for (index, chunk) in self.chunks.iter().enumerate() {
            let expected = (self.bytes - index as u64 * CHUNK_BYTES).min(CHUNK_BYTES);
            ensure!(
                chunk.size_bytes == expected && valid_hash(&chunk.sha256),
                "guest memory chunk length or digest is invalid"
            );
        }
        Ok(())
    }
}

/// Source bytes must come from the actual stopped QEMU owner. The returned
/// provider entries must join that same protected manifest before publication.
/// A failed full hash can leave verified private CAS chunks, never a receipt.
pub(in crate::business_os) fn store_memory_chunks(
    store: &CheckpointStore,
    input: &mut File,
    expected: &QemuMemoryState,
) -> Result<Vec<WorkspaceEntry>> {
    let metadata = private_file(input)?;
    ensure!(
        metadata.mode() & 0o222 == 0
            && metadata.len() == expected.bytes
            && (1..=MAX_MEMORY_BYTES).contains(&expected.bytes)
            && valid_hash(&expected.sha256),
        "source guest memory is mutable, incomplete or oversized"
    );
    input.rewind()?;
    let mut hash = Sha256::new();
    let mut remaining = expected.bytes;
    let mut buffer = vec![0u8; CHUNK_BYTES as usize];
    let mut manifest = MemoryManifest {
        version: 1,
        bytes: expected.bytes,
        sha256: expected.sha256.clone(),
        chunks: Vec::new(),
    };
    let mut entries = Vec::new();
    while remaining > 0 {
        let count = remaining.min(CHUNK_BYTES) as usize;
        input.read_exact(&mut buffer[..count])?;
        hash.update(&buffer[..count]);
        let part = artifact(store, &buffer[..count])?;
        entries.push(entry(chunk_path(manifest.chunks.len()), part.clone()));
        manifest.chunks.push(part);
        remaining -= count as u64;
    }
    ensure!(
        input.read(&mut [0u8; 1])? == 0,
        "source guest memory grew during capture"
    );
    ensure!(
        format!("{:x}", hash.finalize()) == expected.sha256,
        "source guest memory changed during chunking"
    );
    manifest.validate()?;
    let bytes = serde_json::to_vec(&manifest)?;
    ensure!(
        bytes.len() as u64 <= MAX_METADATA_BYTES,
        "guest memory metadata is oversized"
    );
    entries.push(entry(METADATA_PATH.into(), artifact(store, &bytes)?));
    Ok(entries)
}

/// The caller passes provider entries from its current completed protected
/// import, not a renderer list. Reconstruct only in a fresh private native
/// staging descriptor. No readiness, process effect or execution is created.
pub(in crate::business_os) fn load_memory_chunks(
    store: &CheckpointStore,
    protected_provider_state: &[WorkspaceEntry],
    output: &mut File,
) -> Result<QemuMemoryState> {
    let metadata = private_file(output)?;
    ensure!(
        metadata.len() == 0 && metadata.mode() & 0o200 != 0,
        "guest memory output must be a fresh private writable staging file"
    );
    let mut entries = BTreeMap::new();
    for entry in protected_provider_state
        .iter()
        .filter(|entry| entry.path == METADATA_PATH || entry.path.starts_with(CHUNK_PREFIX))
    {
        ensure!(
            entry.kind == WorkspaceEntryKind::File && !entry.executable,
            "guest memory entry is not a plain immutable file"
        );
        ensure!(
            entries.insert(entry.path.as_str(), entry).is_none(),
            "guest memory provider path is duplicated"
        );
    }
    let metadata = entries
        .get(METADATA_PATH)
        .context("protected guest memory metadata is missing")?;
    ensure!(
        (1..=MAX_METADATA_BYTES).contains(&metadata.artifact.size_bytes),
        "protected guest memory metadata is oversized"
    );
    let mut bytes = Vec::new();
    store
        .open_blob(&metadata.artifact)?
        .take(MAX_METADATA_BYTES + 1)
        .read_to_end(&mut bytes)?;
    ensure!(
        bytes.len() as u64 <= MAX_METADATA_BYTES,
        "guest memory metadata grew"
    );
    let manifest: MemoryManifest = serde_json::from_slice(&bytes)?;
    manifest.validate()?;
    ensure!(
        entries.len() == manifest.chunks.len() + 1,
        "protected guest memory has missing or unexpected chunks"
    );
    for (index, artifact) in manifest.chunks.iter().enumerate() {
        let path = chunk_path(index);
        let entry = entries
            .get(path.as_str())
            .context("protected guest memory chunk is missing")?;
        ensure!(
            entry.artifact == *artifact,
            "protected guest memory chunk order/identity differs"
        );
    }
    output.rewind()?;
    let mut hash = Sha256::new();
    let mut buffer = [0u8; 128 * 1024];
    for artifact in &manifest.chunks {
        let mut input = store.open_blob(artifact)?;
        let mut remaining = artifact.size_bytes;
        while remaining > 0 {
            let count = remaining.min(buffer.len() as u64) as usize;
            input.read_exact(&mut buffer[..count])?;
            hash.update(&buffer[..count]);
            output.write_all(&buffer[..count])?;
            remaining -= count as u64;
        }
        ensure!(
            input.read(&mut [0u8; 1])? == 0,
            "guest memory chunk grew during reconstruction"
        );
    }
    ensure!(
        format!("{:x}", hash.finalize()) == manifest.sha256,
        "reconstructed guest memory full digest differs"
    );
    output.flush()?;
    output.sync_all()?;
    output.set_permissions(std::fs::Permissions::from_mode(0o400))?;
    output.sync_all()?;
    Ok(QemuMemoryState {
        bytes: manifest.bytes,
        sha256: manifest.sha256,
    })
}

#[cfg(test)]
mod tests;
