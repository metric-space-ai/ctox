// Origin: CTOX
// License: AGPL-3.0-only

//! Native-owned, initially paused QEMU child. This is a process primitive,
//! not permission, a provisioner, a controller lease, or guest readiness.
//! The caller must retain this owner across awaits and confirm exit before
//! releasing its native write fence. Prepared disks are never removed here.

use super::channel::{GuestChannel, RemoteGuestDriver, GUEST_DESKTOP_PORT};
use super::identifier;
use super::qmp::{QemuStatus, QmpClient};
use anyhow::{anyhow, ensure, Context, Result};
use serde_json::json;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{ExitStatus, Stdio};
use std::time::Duration;
use tempfile::TempDir;
use tokio::net::{UnixListener, UnixStream};
use tokio::process::{Child, Command};

const CONNECT_TIMEOUT: Duration = Duration::from_secs(10);
const EXIT_TIMEOUT: Duration = Duration::from_secs(10);

/// Resolved by the native image/lifecycle owner, never deserialized from a
/// renderer or model request. Image provenance and host admission belong there.
pub(super) struct PreparedQemuGuest {
    pub program: PathBuf,
    pub runtime_parent: PathBuf,
    /// A verified, immutable, standalone raw base image.
    pub base_raw: PathBuf,
    /// An existing native-created qcow2 overlay, exclusively assigned to this guest.
    pub overlay_qcow2: PathBuf,
    pub memory_mib: u32,
    pub vcpus: u8,
    pub acceleration: QemuAcceleration,
}

#[derive(Clone, Copy)]
pub(super) enum QemuAcceleration {
    Kvm,
    /// Software emulation is only used by bounded CI process tests. No silent
    /// production fallback from unavailable KVM to a slow emulated desktop.
    #[cfg(test)]
    Tcg,
}

pub(super) struct QemuProcess {
    child: Child,
    pid: u32,
    monitor: Option<QmpClient<UnixStream>>,
    listener: Option<UnixListener>,
    guest_listener: Option<UnixListener>,
    guest_channel: Option<GuestChannel<UnixStream>>,
    // Dropped after the child. Only the private monitor directory is disposable.
    runtime: TempDir,
}

pub(super) fn regular_file(path: &Path) -> Result<std::fs::Metadata> {
    ensure!(path.is_absolute(), "QEMU paths must be absolute");
    let metadata = std::fs::symlink_metadata(path)
        .map_err(|_| anyhow!("prepared QEMU file is unavailable"))?;
    ensure!(
        metadata.is_file(),
        "prepared QEMU path must be a regular file"
    );
    Ok(metadata)
}

fn prepare_command(config: &PreparedQemuGuest, monitor: &Path, guest: &Path) -> Result<Command> {
    use std::os::unix::fs::MetadataExt;

    ensure!(
        (1..=2).contains(&config.vcpus),
        "QEMU vCPU budget is invalid"
    );
    ensure!(
        (32..=4096).contains(&config.memory_mib),
        "QEMU memory budget is invalid"
    );
    regular_file(&config.program)?;
    let base = regular_file(&config.base_raw)?;
    let overlay = regular_file(&config.overlay_qcow2)?;
    ensure!(
        base.len() > 0 && base.len() % 512 == 0,
        "prepared raw base size is invalid"
    );
    ensure!(overlay.len() > 0, "prepared overlay is empty");
    ensure!(
        (base.dev(), base.ino()) != (overlay.dev(), overlay.ino()),
        "base and overlay must be distinct files"
    );
    let monitor = chardev_socket_path(monitor, "monitor")?;
    let guest = chardev_socket_path(guest, "guest channel")?;
    let base_path = config
        .base_raw
        .to_str()
        .context("base path must be UTF-8")?;
    let overlay_path = config
        .overlay_qcow2
        .to_str()
        .context("overlay path must be UTF-8")?;

    let mut command = Command::new(&config.program);
    command.args([
        "-machine",
        "pc",
        "-accel",
        match config.acceleration {
            QemuAcceleration::Kvm => "kvm",
            #[cfg(test)]
            QemuAcceleration::Tcg => "tcg,thread=single",
        },
    ]);
    command.args(["-m", &config.memory_mib.to_string()]);
    command.args(["-smp", &config.vcpus.to_string()]);
    command.args([
        "-nodefaults",
        "-no-user-config",
        "-display",
        "none",
        "-serial",
        "none",
        "-monitor",
        "none",
        "-nic",
        "none",
        "-sandbox",
        "on,obsolete=deny,elevateprivileges=deny,spawn=deny,resourcecontrol=deny",
        "-S",
    ]);
    // Explicit formats and backing nodes prevent QEMU from guessing a raw
    // image's format or following a backing filename stored in the overlay.
    command.arg("-blockdev").arg(
        json!({
            "driver": "raw", "node-name": "guest-base", "read-only": true,
            "file": {
                "driver": "file", "filename": base_path, "read-only": true,
                "locking": "on"
            }
        })
        .to_string(),
    );
    command.arg("-blockdev").arg(
        json!({
            "driver": "qcow2", "node-name": "guest-root",
            "backing": "guest-base",
            "file": { "driver": "file", "filename": overlay_path, "locking": "on" }
        })
        .to_string(),
    );
    command.args([
        "-device",
        "virtio-blk-pci,drive=guest-root",
        "-device",
        "virtio-vga",
        "-device",
        "virtio-serial",
    ]);
    command
        .arg("-chardev")
        .arg(format!("socket,id=control,path={monitor},server=off"))
        .args(["-mon", "chardev=control,mode=control"]);
    // qemu-ga uses virtio-serial + virtserialport; the host binds the unix
    // socket and QEMU connects (server=off), matching the owned QMP chardev.
    // The guest process owner later invokes run_guest_desktop_effects; this
    // owner does not start that guest agent.
    command
        .arg("-chardev")
        .arg(format!("socket,id=guestctl,path={guest},server=off"))
        .arg("-device")
        .arg(format!(
            "virtserialport,chardev=guestctl,name={GUEST_DESKTOP_PORT}"
        ));
    command
        .env_clear()
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .kill_on_drop(true);
    Ok(command)
}

fn chardev_socket_path<'a>(path: &'a Path, what: &str) -> Result<&'a str> {
    let path = path
        .to_str()
        .with_context(|| format!("{what} path must be UTF-8"))?;
    // The local chardev uses QEMU keyval syntax, unlike JSON blockdev. Refuse
    // delimiters rather than interpreting a native filesystem path as options.
    ensure!(
        !path.contains(',') && !path.chars().any(char::is_control),
        "{what} path contains unsupported characters"
    );
    Ok(path)
}

impl QemuProcess {
    /// Creates the owner synchronously, before the first cancellable await.
    /// A successful return proves only process creation; call connect_monitor.
    pub(super) fn spawn_paused(config: &PreparedQemuGuest) -> Result<Self> {
        ensure!(
            config.runtime_parent.is_absolute(),
            "runtime parent must be absolute"
        );
        let runtime = tempfile::Builder::new()
            .prefix("qemu-")
            .permissions(std::fs::Permissions::from_mode(0o700))
            .tempdir_in(&config.runtime_parent)
            .map_err(|_| anyhow!("private QEMU runtime directory is unavailable"))?;
        // Restrict access at creation, before binding the monitor or spawning.
        let socket = runtime.path().join("qmp.sock");
        let guest_socket = runtime.path().join("guest.sock");
        let mut command = prepare_command(config, &socket, &guest_socket)?;
        let listener = UnixListener::bind(&socket)
            .map_err(|_| anyhow!("private QEMU monitor could not be bound"))?;
        let guest_listener = UnixListener::bind(&guest_socket)
            .map_err(|_| anyhow!("private guest channel could not be bound"))?;
        let child = command
            .spawn()
            .map_err(|_| anyhow!("QEMU could not be started"))?;
        let pid = child.id().context("QEMU child identity is unavailable")?;
        Ok(Self {
            child,
            pid,
            monitor: None,
            listener: Some(listener),
            guest_listener: Some(guest_listener),
            guest_channel: None,
            runtime,
        })
    }

    pub(super) fn pid(&self) -> u32 {
        self.pid
    }

    pub(super) fn runtime_directory(&self) -> &Path {
        self.runtime.path()
    }

    /// A cancelled or failed handshake is not retried. The caller still owns
    /// the paused child and must stop/wait it before abandoning the attempt.
    pub(super) async fn connect_monitor(&mut self) -> Result<QemuStatus> {
        let listener = self
            .listener
            .take()
            .context("QEMU monitor handshake is retired")?;
        let connected = tokio::time::timeout(CONNECT_TIMEOUT, async {
            let (stream, _) = tokio::select! {
                accepted = listener.accept() => accepted.context("QEMU monitor accept failed")?,
                exited = self.child.wait() => {
                    exited.map_err(|_| anyhow!("QEMU exit could not be observed"))?;
                    return Err(anyhow!("QEMU exited before its monitor became available"));
                }
            };
            ensure!(
                stream.peer_cred()?.pid() == Some(self.pid as i32),
                "QEMU monitor peer does not match the owned child"
            );
            let mut monitor = QmpClient::negotiate(stream, Duration::from_secs(5)).await?;
            let status = monitor.query_status().await?;
            ensure!(
                !status.running && matches!(status.status.as_str(), "prelaunch" | "paused"),
                "QEMU did not start paused"
            );
            Ok::<_, anyhow::Error>((monitor, status))
        })
        .await
        .map_err(|_| anyhow!("QEMU monitor handshake exceeded its deadline"))??;
        self.monitor = Some(connected.0);
        Ok(connected.1)
    }

    fn monitor(&mut self) -> Result<&mut QmpClient<UnixStream>> {
        self.monitor.as_mut().context("QEMU monitor is unavailable")
    }

    /// A cancelled or failed handshake is not retried. The caller still owns
    /// the paused child and must stop/wait it before abandoning the attempt.
    pub(super) async fn connect_guest_channel(&mut self) -> Result<()> {
        let listener = self
            .guest_listener
            .take()
            .context("guest channel handshake is retired")?;
        let stream = tokio::time::timeout(CONNECT_TIMEOUT, async {
            let (stream, _) = tokio::select! {
                accepted = listener.accept() => accepted.context("guest channel accept failed")?,
                exited = self.child.wait() => {
                    exited.map_err(|_| anyhow!("QEMU exit could not be observed"))?;
                    return Err(anyhow!("QEMU exited before its guest channel became available"));
                }
            };
            ensure!(
                stream.peer_cred()?.pid() == Some(self.pid as i32),
                "guest channel peer does not match the owned child"
            );
            Ok::<_, anyhow::Error>(stream)
        })
        .await
        .map_err(|_| anyhow!("guest channel handshake exceeded its deadline"))??;
        self.guest_channel = Some(GuestChannel::new(stream));
        Ok(())
    }

    pub(super) fn guest_channel(&mut self) -> Result<&mut GuestChannel<UnixStream>> {
        self.guest_channel
            .as_mut()
            .context("guest channel is unavailable")
    }

    pub(super) fn bind_guest_driver(
        &mut self,
        guest_id: String,
    ) -> Result<RemoteGuestDriver<UnixStream>> {
        ensure!(identifier(&guest_id), "guest identity is invalid");
        let channel = self
            .guest_channel
            .take()
            .context("guest channel is unavailable")?;
        RemoteGuestDriver::bind(guest_id, channel)
    }

    /// Each input/effect call must remain inside the native authority fence.
    pub(super) async fn resume(&mut self) -> Result<()> {
        Ok(self.monitor()?.resume().await?)
    }

    pub(super) async fn pause(&mut self) -> Result<()> {
        Ok(self.monitor()?.pause().await?)
    }

    pub(super) async fn status(&mut self) -> Result<QemuStatus> {
        Ok(self.monitor()?.query_status().await?)
    }

    /// A request only. Use wait_for_exit to establish actual termination.
    pub(super) async fn request_shutdown(&mut self) -> Result<()> {
        Ok(self.monitor()?.request_powerdown().await?)
    }

    pub(super) async fn wait_for_exit(&mut self) -> Result<ExitStatus> {
        let status = tokio::time::timeout(EXIT_TIMEOUT, self.child.wait())
            .await
            .map_err(|_| anyhow!("QEMU has not confirmed exit; retain its ownership fence"))?
            .map_err(|_| anyhow!("QEMU exit could not be observed"))?;
        self.monitor = None;
        self.listener = None;
        self.guest_channel = None;
        self.guest_listener = None;
        Ok(status)
    }

    /// Forced termination is not an application-consistent checkpoint.
    /// On timeout/error self retains the child; do not release its fence.
    pub(super) async fn stop(&mut self) -> Result<ExitStatus> {
        self.monitor = None;
        self.listener = None;
        self.guest_channel = None;
        self.guest_listener = None;
        if self
            .child
            .try_wait()
            .map_err(|_| anyhow!("QEMU status is unavailable"))?
            .is_none()
        {
            self.child
                .start_kill()
                .map_err(|_| anyhow!("QEMU termination failed"))?;
        }
        self.wait_for_exit().await
    }
}

#[cfg(test)]
mod tests;
