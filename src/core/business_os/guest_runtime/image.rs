// Origin: CTOX
// License: AGPL-3.0-only

//! Creates a fresh per-guest qcow2 overlay without copying a verified raw base.
//! Native-only preparation: image trust, persistent registration and host
//! admission remain with the lifecycle owner. No existing disk is replaced.

use super::qemu::regular_file;
use anyhow::{anyhow, ensure, Context, Result};
use std::fs::File;
use std::os::unix::fs::{MetadataExt, OpenOptionsExt, PermissionsExt};
use std::path::{Path, PathBuf};
use std::process::{ExitStatus, Stdio};
use std::time::Duration;
use tempfile::TempDir;
use tokio::process::{Child, Command};

const PREPARE_TIMEOUT: Duration = Duration::from_secs(10);

pub(in crate::business_os) struct QemuOverlayPreparation {
    // The owner exists before any await; cancellation does not lose the PID.
    child: Child,
    directory: Option<TempDir>,
    attempted: bool,
}

impl QemuOverlayPreparation {
    /// The native owner supplies an admitted state directory and a verified,
    /// immutable raw base. This function does not download or trust an image.
    pub(in crate::business_os) fn start(
        program: &Path,
        state_parent: &Path,
        base_raw: &Path,
    ) -> Result<Self> {
        regular_file(program)?;
        let base = regular_file(base_raw)?;
        ensure!(
            base.len() > 0 && base.len() % 512 == 0,
            "prepared raw base size is invalid"
        );
        ensure!(
            state_parent.is_absolute(),
            "guest state parent must be absolute"
        );
        let directory = tempfile::Builder::new()
            .prefix("guest-disk-")
            .permissions(std::fs::Permissions::from_mode(0o700))
            .tempdir_in(state_parent)
            .map_err(|_| anyhow!("private guest disk directory is unavailable"))?;
        // The backing name is deliberately unresolved. QemuProcess must bind
        // the verified base explicitly, never follow a path from disk metadata.
        let child = Command::new(program)
            .args(["create", "-q", "-f", "qcow2", "-u", "-F", "raw", "-b"])
            .arg("ctox-native-base")
            .arg(directory.path().join("root.qcow2"))
            .arg(base.len().to_string())
            .env_clear()
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .kill_on_drop(true)
            .spawn()
            .map_err(|_| anyhow!("guest disk preparation could not be started"))?;
        Ok(Self {
            child,
            directory: Some(directory),
            attempted: false,
        })
    }

    /// Retain this owner outside any cancellable future. On failure or
    /// cancellation call abort and await confirmed exit before dropping it.
    /// Successful preparation retains the disk; dropping self cannot erase it.
    pub(in crate::business_os) async fn finish(&mut self) -> Result<PathBuf> {
        ensure!(!self.attempted, "guest disk preparation is retired");
        self.attempted = true;
        let status = self.wait_for_exit().await?;
        ensure!(status.success(), "guest disk preparation failed");
        let directory = self
            .directory
            .as_ref()
            .context("guest disk preparation is retired")?;
        let overlay = directory.path().join("root.qcow2");
        let disk = File::options()
            .read(true)
            .custom_flags(libc::O_NOFOLLOW | libc::O_CLOEXEC)
            .open(&overlay)?;
        let metadata = disk.metadata()?;
        ensure!(
            metadata.is_file()
                && metadata.len() > 0
                && metadata.uid() == unsafe { libc::geteuid() }
                && metadata.nlink() == 1,
            "prepared guest disk must be nonempty, native-owned and unaliased"
        );
        // qemu-img inherits the operator umask. Publish a private writable
        // disk even when that umask permits group/other access by default.
        disk.set_permissions(std::fs::Permissions::from_mode(0o600))?;
        disk.sync_all()?;
        File::open(directory.path())?.sync_all()?;
        File::open(
            directory
                .path()
                .parent()
                .context("guest state parent is unavailable")?,
        )?
        .sync_all()?;
        // Persistence belongs to the caller's existing native state owner.
        // keep is synchronous: cancellation cannot split publication/retention.
        let retained = self.directory.take().unwrap().keep();
        Ok(retained.join("root.qcow2"))
    }

    async fn wait_for_exit(&mut self) -> Result<ExitStatus> {
        tokio::time::timeout(PREPARE_TIMEOUT, self.child.wait())
            .await
            .map_err(|_| anyhow!("guest disk preparation has not confirmed exit"))?
            .map_err(|_| anyhow!("guest disk preparation exit could not be observed"))
    }

    /// Removes only this attempt's unpublished directory, after reaping the
    /// captured helper. A completed disk is retained even if abort is called.
    pub(in crate::business_os) async fn abort(&mut self) -> Result<()> {
        self.attempted = true;
        if self.child.try_wait()?.is_none() {
            self.child.start_kill()?;
        }
        self.wait_for_exit().await?;
        if let Some(directory) = self.directory.take() {
            directory.close()?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests;
