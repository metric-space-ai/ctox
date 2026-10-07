// Origin: CTOX
// License: AGPL-3.0-only

//! Paused, owned-child memory streams. This grants no execution authority and
//! never resumes either side. The lifecycle caller must retain the child,
//! reconcile unknown effects and verify the protected disk/profile manifest.
//! Protocol: https://www.qemu.org/docs/master/interop/qemu-qmp-ref.html#migrate
//! Uses private Unix sockets; no shell command, TCP endpoint or exec URI.

use super::*;
use sha2::{Digest, Sha256};
use std::os::unix::fs::MetadataExt;
use tokio::io::{AsyncReadExt, AsyncSeekExt, AsyncWriteExt};

const MAX_STREAM_BYTES: u64 = 1024 * 1024 * 1024;
const MIGRATION_TIMEOUT: Duration = Duration::from_secs(120);

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum MigrationPhase {
    Fresh,
    Exporting,
    Exported,
    FinishingExport,
    FinishedExport,
    Incoming,
    Restoring,
    Restored,
}

/// Actual bytes saved, not an import receipt, clean-effect assertion or grant.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::business_os) struct QemuMemoryState {
    pub bytes: u64,
    pub sha256: String,
}

fn private_file(metadata: &std::fs::Metadata) -> Result<()> {
    ensure!(
        metadata.is_file()
            && metadata.nlink() == 1
            && metadata.uid() == unsafe { libc::geteuid() }
            && metadata.mode() & 0o077 == 0,
        "QEMU memory file is not private, native-owned and unaliased"
    );
    Ok(())
}

async fn digest_file(file: &mut tokio::fs::File) -> Result<QemuMemoryState> {
    file.rewind().await?;
    let mut hash = Sha256::new();
    let mut bytes = 0u64;
    let mut buffer = [0u8; 128 * 1024];
    loop {
        let count = file.read(&mut buffer).await?;
        if count == 0 {
            break;
        }
        bytes += count as u64;
        ensure!(
            bytes <= MAX_STREAM_BYTES,
            "QEMU memory exceeds protected transport limit"
        );
        hash.update(&buffer[..count]);
    }
    Ok(QemuMemoryState {
        bytes,
        sha256: format!("{:x}", hash.finalize()),
    })
}

impl QemuProcess {
    /// The caller supplies its newly created private file descriptor, never a
    /// renderer-selected filename. Cancellation retires migration/resume while
    /// preserving this exact child for explicit stop and effect reconciliation.
    pub(in crate::business_os::guest_runtime) async fn save_memory(
        &mut self,
        output: &mut tokio::fs::File,
    ) -> Result<QemuMemoryState> {
        ensure!(
            self.migration == MigrationPhase::Fresh,
            "QEMU memory export already attempted"
        );
        let metadata = output.metadata().await?;
        private_file(&metadata)?;
        ensure!(metadata.len() == 0, "QEMU memory output must be empty");
        self.migration = MigrationPhase::Exporting;
        let result = tokio::time::timeout(MIGRATION_TIMEOUT, async {
            self.ensure_alive()?;
            let status = self.status().await?;
            ensure!(
                !status.running && matches!(status.status.as_str(), "paused" | "prelaunch"),
                "QEMU memory capture requires a confirmed paused child"
            );
            output.rewind().await?;
            let socket = self.runtime.path().join("memory-out.sock");
            let listener = UnixListener::bind(&socket)?;
            self.monitor()?.migrate_local(&socket, false).await?;
            let (mut stream, _) = listener.accept().await?;
            ensure!(
                stream.peer_cred()?.pid() == Some(self.pid as i32),
                "QEMU memory stream peer is not the owned child"
            );
            let mut hash = Sha256::new();
            let mut bytes = 0u64;
            let mut buffer = [0u8; 128 * 1024];
            loop {
                let count = stream.read(&mut buffer).await?;
                if count == 0 {
                    break;
                }
                bytes += count as u64;
                ensure!(
                    bytes <= MAX_STREAM_BYTES,
                    "QEMU memory exceeds protected transport limit"
                );
                output.write_all(&buffer[..count]).await?;
                hash.update(&buffer[..count]);
            }
            ensure!(bytes > 0, "QEMU memory stream is empty");
            loop {
                self.ensure_alive()?;
                match self.monitor()?.migration_status().await?.as_str() {
                    "completed" => break,
                    "setup" | "active" | "device" | "wait-unplug" => {
                        tokio::time::sleep(Duration::from_millis(50)).await;
                    }
                    _ => anyhow::bail!("QEMU memory export did not complete"),
                }
            }
            let status = self.status().await?;
            ensure!(
                !status.running && status.status == "postmigrate",
                "source QEMU is not stopped after memory export"
            );
            output.flush().await?;
            output.sync_all().await?;
            output
                .set_permissions(std::fs::Permissions::from_mode(0o400))
                .await?;
            Ok::<_, anyhow::Error>(QemuMemoryState {
                bytes,
                sha256: format!("{:x}", hash.finalize()),
            })
        })
        .await
        .context("QEMU memory export exceeded its deadline")??;
        self.migration = MigrationPhase::Exported;
        Ok(result)
    }

    /// Flush/close the source disk through QEMU's normal quit path, then
    /// confirm this exact child exited successfully before copying its disk.
    /// Uncertain quit/exit cannot be retried or promoted to a clean checkpoint.
    pub(in crate::business_os::guest_runtime) async fn finish_memory_export(
        &mut self,
    ) -> Result<ExitStatus> {
        ensure!(
            self.migration == MigrationPhase::Exported,
            "QEMU export is not complete"
        );
        self.migration = MigrationPhase::FinishingExport;
        self.monitor()?.quit().await?;
        let status = self.wait_for_exit().await?;
        ensure!(status.success(), "QEMU source did not exit cleanly");
        self.migration = MigrationPhase::FinishedExport;
        Ok(status)
    }

    /// The descriptor must come from the native protected staging owner.
    /// Verify every byte before feeding QEMU, then hash the exact fed bytes
    /// again. Any mismatch/cancellation leaves this attempt unable to resume.
    /// Successful restore remains paused; caller must freshly authorize cont.
    pub(in crate::business_os::guest_runtime) async fn restore_memory(
        &mut self,
        input: &mut tokio::fs::File,
        expected: &QemuMemoryState,
    ) -> Result<()> {
        ensure!(
            self.migration == MigrationPhase::Incoming,
            "QEMU incoming attempt is retired"
        );
        self.migration = MigrationPhase::Restoring;
        tokio::time::timeout(MIGRATION_TIMEOUT, async {
            let metadata = input.metadata().await?;
            private_file(&metadata)?;
            ensure!(
                metadata.mode() & 0o222 == 0
                    && expected.bytes > 0
                    && expected.bytes <= MAX_STREAM_BYTES
                    && metadata.len() == expected.bytes
                    && expected.sha256.len() == 64
                    && expected.sha256.bytes().all(|b| b.is_ascii_hexdigit()),
                "QEMU memory input is incomplete or mutable"
            );
            ensure!(
                digest_file(input).await? == *expected,
                "QEMU memory digest differs"
            );
            self.ensure_alive()?;
            let status = self.status().await?;
            ensure!(
                !status.running && status.status == "inmigrate",
                "QEMU incoming child changed"
            );
            let socket = self.runtime.path().join("memory-in.sock");
            self.monitor()?.migrate_local(&socket, true).await?;
            let mut stream = UnixStream::connect(&socket).await?;
            ensure!(
                stream.peer_cred()?.pid() == Some(self.pid as i32),
                "QEMU incoming stream peer is not the owned child"
            );
            input.rewind().await?;
            let mut hash = Sha256::new();
            let mut bytes = 0u64;
            let mut buffer = [0u8; 128 * 1024];
            loop {
                let count = input.read(&mut buffer).await?;
                if count == 0 {
                    break;
                }
                bytes += count as u64;
                ensure!(
                    bytes <= expected.bytes,
                    "QEMU memory input grew during restore"
                );
                hash.update(&buffer[..count]);
                stream.write_all(&buffer[..count]).await?;
            }
            ensure!(
                bytes == expected.bytes && format!("{:x}", hash.finalize()) == expected.sha256,
                "QEMU memory changed while restoring"
            );
            stream.shutdown().await?;
            loop {
                self.ensure_alive()?;
                let status = self.status().await?;
                ensure!(!status.running, "restored QEMU ran before authorization");
                match status.status.as_str() {
                    "paused" => break,
                    "inmigrate" => tokio::time::sleep(Duration::from_millis(50)).await,
                    _ => anyhow::bail!("QEMU incoming restore did not complete"),
                }
            }
            Ok::<_, anyhow::Error>(())
        })
        .await
        .context("QEMU incoming restore exceeded its deadline")??;
        self.migration = MigrationPhase::Restored;
        Ok(())
    }
}
