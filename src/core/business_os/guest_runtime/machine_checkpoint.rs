// Origin: CTOX
// License: AGPL-3.0-only

//! RAM, writable disk and original service identity in one protected checkpoint.
//! A stopped-source witness is issued only by the retained real QEMU owner.
//! No helper here grants ownership, reconciles effects or activates a guest.
//! Chunk/hash I/O runs outside issuer/SQLite decision fences; the native caller
//! rechecks current ownership and completed import authority before activation.

use super::{identifier, memory_chunks, PreparedQemuGuest, QemuMemoryState};
use anyhow::{ensure, Context, Result};
use ctox_sync::{
    checkpoint::CheckpointStore,
    contracts::{ArtifactRef, WorkspaceEntry, WorkspaceEntryKind},
    guest_restore::GuestLiveEndpoint,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    fs::{File, OpenOptions},
    io::{Read, Seek},
    os::unix::fs::{MetadataExt, OpenOptionsExt, PermissionsExt},
    path::Path,
};

#[cfg(test)]
mod tests;

const MACHINE_PATH: &str = "native-guest-machine.json";
const PROFILE: &str = "ctox.pc-i440fx-5.1.qemu64-v1.v1";
const MAX_METADATA: u64 = 64 * 1024;
const MAX_STATE: u64 = 1024 * 1024 * 1024 - MAX_METADATA;
const MAX_BASE: u64 = 64 * 1024 * 1024 * 1024;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
struct FileIdentity {
    bytes: u64,
    sha256: String,
}

impl From<QemuMemoryState> for FileIdentity {
    fn from(value: QemuMemoryState) -> Self {
        Self {
            bytes: value.bytes,
            sha256: value.sha256,
        }
    }
}

impl FileIdentity {
    fn memory(&self) -> QemuMemoryState {
        QemuMemoryState {
            bytes: self.bytes,
            sha256: self.sha256.clone(),
        }
    }

    fn validate(&self, maximum: u64) -> Result<()> {
        ensure!(
            (1..=maximum).contains(&self.bytes)
                && self.sha256.len() == 64
                && self
                    .sha256
                    .bytes()
                    .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b)),
            "guest machine file identity is invalid"
        );
        Ok(())
    }
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
struct MachineManifest {
    version: u32,
    profile: String,
    guest_id: String,
    source_process_instance_id: String,
    source_endpoint_id: String,
    guest_service_session: String,
    memory_mib: u32,
    vcpus: u8,
    base: FileIdentity,
    memory: FileIdentity,
    disk: FileIdentity,
}

impl MachineManifest {
    fn validate(&self, guest: &str, session: &str, config: &PreparedQemuGuest) -> Result<()> {
        ensure!(
            self.version == 1
                && self.profile == PROFILE
                && self.guest_id == guest
                && self.guest_service_session == session
                && identifier(guest)
                && identifier(session)
                && self.memory_mib == config.memory_mib
                && self.vcpus == config.vcpus,
            "guest machine assignment, service or hardware profile differs"
        );
        ensure!(
            !self.source_process_instance_id.is_empty()
                && self.source_process_instance_id.len() <= 256
                && !self.source_endpoint_id.is_empty()
                && self.source_endpoint_id.len() <= 256,
            "guest source endpoint witness is invalid"
        );
        self.base.validate(MAX_BASE)?;
        self.memory.validate(MAX_STATE)?;
        self.disk.validate(MAX_STATE)?;
        ensure!(
            self.memory
                .bytes
                .checked_add(self.disk.bytes)
                .is_some_and(|n| n <= MAX_STATE),
            "combined guest RAM and disk exceed protected checkpoint budget"
        );
        Ok(())
    }
}

fn private_file(file: &File) -> Result<std::fs::Metadata> {
    let m = file.metadata()?;
    ensure!(
        m.is_file()
            && m.uid() == unsafe { libc::geteuid() }
            && m.mode() & 0o077 == 0
            && m.nlink() == 1,
        "guest machine file must be private, native-owned and unaliased"
    );
    Ok(m)
}

fn open_private(path: &Path) -> Result<File> {
    ensure!(path.is_absolute(), "guest machine path must be absolute");
    let file = OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NOFOLLOW)
        .open(path)?;
    private_file(&file)?;
    Ok(file)
}

fn same_file(file: &File, path: &Path) -> Result<()> {
    let actual = private_file(file)?;
    let current = std::fs::symlink_metadata(path)?;
    ensure!(
        current.is_file() && (actual.dev(), actual.ino()) == (current.dev(), current.ino()),
        "guest machine staging path no longer names its retained file"
    );
    Ok(())
}

fn digest(file: &mut File, maximum: u64) -> Result<FileIdentity> {
    let before = private_file(file)?;
    ensure!(
        before.len() > 0 && before.len() <= maximum && before.mode() & 0o222 == 0,
        "guest machine input is mutable, empty or oversized"
    );
    file.rewind()?;
    let mut hash = Sha256::new();
    let mut total = 0u64;
    let mut buffer = vec![0u8; 128 * 1024];
    loop {
        let count = file.read(&mut buffer)?;
        if count == 0 {
            break;
        }
        total = total
            .checked_add(count as u64)
            .context("guest machine length overflow")?;
        ensure!(total <= before.len(), "guest machine input grew");
        hash.update(&buffer[..count]);
    }
    let after = private_file(file)?;
    ensure!(
        total == before.len()
            && before.len() == after.len()
            && before.mtime() == after.mtime()
            && before.mtime_nsec() == after.mtime_nsec()
            && after.mode() & 0o222 == 0,
        "guest machine input changed"
    );
    Ok(FileIdentity {
        bytes: total,
        sha256: format!("{:x}", hash.finalize()),
    })
}

fn disk_entries(entries: Vec<WorkspaceEntry>, to_disk: bool) -> Vec<WorkspaceEntry> {
    entries
        .into_iter()
        .map(|mut e| {
            let (from, to) = if to_disk {
                ("native-guest-memory", "native-guest-disk")
            } else {
                ("native-guest-disk", "native-guest-memory")
            };
            e.path = e.path.replacen(from, to, 1);
            e
        })
        .collect()
}

/// This opaque value is created only after save_memory_live has successfully
/// quit and reaped the real source. It is not a clean external-effect receipt.
pub(in crate::business_os) struct QuiescedQemuCheckpoint {
    config: PreparedQemuGuest,
    guest_id: String,
    endpoint: GuestLiveEndpoint,
    memory: QemuMemoryState,
    base: File,
    disk: File,
}

impl QuiescedQemuCheckpoint {
    pub(super) fn after_clean_exit(
        config: PreparedQemuGuest,
        guest_id: String,
        endpoint: GuestLiveEndpoint,
        memory: QemuMemoryState,
    ) -> Result<Self> {
        let base = open_private(&config.base_raw)?;
        ensure!(
            base.metadata()?.mode() & 0o222 == 0,
            "guest base must remain immutable"
        );
        let disk = open_private(&config.overlay_qcow2)?;
        // The retained source has already exited successfully; retire its writable disk.
        disk.sync_all()?;
        disk.set_permissions(std::fs::Permissions::from_mode(0o400))?;
        disk.sync_all()?;
        Ok(Self {
            config,
            guest_id,
            endpoint,
            memory,
            base,
            disk,
        })
    }

    /// Consume the witness once. All returned entries must join the same native
    /// protected manifest/history/effect snapshot before transport/publication.
    pub(in crate::business_os) fn store(
        mut self,
        store: &CheckpointStore,
        memory: &mut File,
    ) -> Result<Vec<WorkspaceEntry>> {
        same_file(&self.base, &self.config.base_raw)?;
        same_file(&self.disk, &self.config.overlay_qcow2)?;
        let base = digest(&mut self.base, MAX_BASE)?;
        let disk = digest(&mut self.disk, MAX_STATE)?;
        let manifest = MachineManifest {
            version: 1,
            profile: PROFILE.into(),
            guest_id: self.guest_id,
            source_process_instance_id: self.endpoint.process_instance_id,
            source_endpoint_id: self.endpoint.endpoint_id,
            guest_service_session: self.endpoint.guest_session_id,
            memory_mib: self.config.memory_mib,
            vcpus: self.config.vcpus,
            base,
            memory: self.memory.clone().into(),
            disk,
        };
        manifest.validate(
            &manifest.guest_id,
            &manifest.guest_service_session,
            &self.config,
        )?;
        let mut entries = memory_chunks::store_memory_chunks(store, memory, &self.memory)?;
        entries.extend(disk_entries(
            memory_chunks::store_memory_chunks(store, &mut self.disk, &manifest.disk.memory())?,
            true,
        ));
        let bytes = serde_json::to_vec(&manifest)?;
        ensure!(
            bytes.len() as u64 <= MAX_METADATA,
            "guest machine metadata is oversized"
        );
        let artifact = ArtifactRef {
            sha256: format!("{:x}", Sha256::digest(&bytes)),
            size_bytes: bytes.len() as u64,
        };
        store.ingest_blob(&artifact, bytes.as_slice())?;
        entries.push(WorkspaceEntry {
            path: MACHINE_PATH.into(),
            kind: WorkspaceEntryKind::File,
            artifact,
            executable: false,
        });
        Ok(entries)
    }
}

/// Verified bytes only, never a GuestImportReceipt or permission to activate.
pub(in crate::business_os) struct StagedQemuCheckpoint {
    pub(super) config: PreparedQemuGuest,
    pub(super) guest_id: String,
    pub(super) service_session: String,
    pub(super) memory: File,
    pub(super) memory_state: QemuMemoryState,
    disk: File,
    disk_identity: FileIdentity,
    base: File,
    base_identity: FileIdentity,
    spawn_attempted: bool,
    pub(super) target_instance_id: Option<String>,
}

impl StagedQemuCheckpoint {
    /// Inputs come from the completed protected import and registered native
    /// assignment. Independent base provisioning is verified by full hash.
    pub(in crate::business_os) fn stage(
        store: &CheckpointStore,
        entries: &[WorkspaceEntry],
        config: PreparedQemuGuest,
        expected_guest: &str,
        expected_service: &str,
        memory: &mut File,
        disk: &mut File,
    ) -> Result<Self> {
        let found: Vec<_> = entries.iter().filter(|e| e.path == MACHINE_PATH).collect();
        ensure!(
            found.len() == 1,
            "protected guest machine metadata missing or duplicated"
        );
        let entry = found[0];
        ensure!(
            entry.kind == WorkspaceEntryKind::File
                && !entry.executable
                && (1..=MAX_METADATA).contains(&entry.artifact.size_bytes),
            "protected guest machine metadata is invalid"
        );
        let mut bytes = Vec::new();
        store
            .open_blob(&entry.artifact)?
            .take(MAX_METADATA + 1)
            .read_to_end(&mut bytes)?;
        ensure!(
            bytes.len() as u64 <= MAX_METADATA,
            "guest machine metadata grew"
        );
        let manifest: MachineManifest = serde_json::from_slice(&bytes)?;
        manifest.validate(expected_guest, expected_service, &config)?;
        let mut base = open_private(&config.base_raw)?;
        ensure!(
            digest(&mut base, MAX_BASE)? == manifest.base,
            "independent guest base differs"
        );
        same_file(disk, &config.overlay_qcow2)?;
        let disk_source = disk_entries(
            entries
                .iter()
                .filter(|e| {
                    e.path == "native-guest-disk.json" || e.path.starts_with("native-guest-disk/")
                })
                .cloned()
                .collect(),
            false,
        );
        let disk_state = memory_chunks::load_memory_chunks(store, &disk_source, disk)?;
        ensure!(
            FileIdentity::from(disk_state) == manifest.disk,
            "staged guest disk differs"
        );
        let memory_state = memory_chunks::load_memory_chunks(store, entries, memory)?;
        ensure!(
            FileIdentity::from(memory_state.clone()) == manifest.memory,
            "staged guest memory differs"
        );
        Ok(Self {
            config,
            guest_id: manifest.guest_id,
            service_session: manifest.guest_service_session,
            memory: memory.try_clone()?,
            memory_state,
            disk: disk.try_clone()?,
            disk_identity: manifest.disk,
            base,
            base_identity: manifest.base,
            spawn_attempted: false,
            target_instance_id: None,
        })
    }

    pub(super) fn prepare_spawn(&mut self) -> Result<()> {
        ensure!(
            !self.spawn_attempted,
            "staged incoming machine attempt is retired"
        );
        self.spawn_attempted = true;
        same_file(&self.base, &self.config.base_raw)?;
        ensure!(
            digest(&mut self.base, MAX_BASE)? == self.base_identity,
            "staged base changed"
        );
        same_file(&self.disk, &self.config.overlay_qcow2)?;
        ensure!(
            digest(&mut self.disk, MAX_STATE)? == self.disk_identity,
            "staged disk changed"
        );
        // Only this target's completed private overlay becomes writable.
        self.disk
            .set_permissions(std::fs::Permissions::from_mode(0o600))?;
        self.disk.sync_all()?;
        Ok(())
    }
}
