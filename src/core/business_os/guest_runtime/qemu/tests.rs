// Origin: CTOX
// License: AGPL-3.0-only

use super::*;
use std::os::unix::fs::PermissionsExt;

fn config(root: &Path) -> Result<PreparedQemuGuest> {
    let base = root.join("base,readonly=off.raw");
    let overlay = root.join("root,backing=other.qcow2");
    let file = std::fs::File::create(&base)?;
    file.set_len(2 * 1024 * 1024)?;
    std::fs::write(&overlay, b"placeholder for pre-spawn validation")?;
    Ok(PreparedQemuGuest {
        program: PathBuf::from("/usr/bin/qemu-system-x86_64"),
        runtime_parent: root.to_path_buf(),
        base_raw: base,
        overlay_qcow2: overlay,
        memory_mib: 64,
        vcpus: 1,
        acceleration: QemuAcceleration::Tcg,
    })
}

async fn real_disk(config: &PreparedQemuGuest) -> Result<()> {
    // qemu-img must not replace an existing user's disk: this test owns the
    // complete directory and just removes its own placeholder.
    std::fs::remove_file(&config.overlay_qcow2)?;
    let mut image = Command::new("/usr/bin/qemu-img")
        .args(["create", "-f", "qcow2", "-u", "-F", "raw", "-b"])
        .arg("/does-not-exist/embedded-backing-must-not-be-followed")
        .arg(&config.overlay_qcow2)
        .arg("2M")
        .env_clear()
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .kill_on_drop(true)
        .spawn()?;
    let result = tokio::time::timeout(Duration::from_secs(10), image.wait()).await;
    if image.try_wait()?.is_none() {
        image.kill().await?;
    }
    image.wait().await?;
    ensure!(
        result.context("qemu-img deadline")??.success(),
        "qemu-img failed"
    );
    Ok(())
}

fn sleeping_program(root: &Path) -> Result<PathBuf> {
    // exec preserves the exact spawned PID. No detached child or watcher.
    let path = root.join("qemu-fixture");
    std::fs::write(&path, b"#!/bin/sh\nexec /bin/sleep 30\n")?;
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o700))?;
    Ok(path)
}

#[tokio::test]
async fn invalid_resources_and_aliasing_disks_are_rejected_before_spawn() -> Result<()> {
    let root = tempfile::tempdir()?;
    let mut input = config(root.path())?;
    input.vcpus = 0;
    ensure!(
        QemuProcess::spawn_paused(&input).is_err(),
        "zero vCPUs admitted"
    );
    input.vcpus = 3;
    ensure!(
        QemuProcess::spawn_paused(&input).is_err(),
        "vCPU limit exceeded"
    );
    input.vcpus = 1;
    input.memory_mib = 4097;
    ensure!(
        QemuProcess::spawn_paused(&input).is_err(),
        "memory limit exceeded"
    );
    input.memory_mib = 64;
    std::fs::remove_file(&input.overlay_qcow2)?;
    std::fs::hard_link(&input.base_raw, &input.overlay_qcow2)?;
    ensure!(
        QemuProcess::spawn_paused(&input).is_err(),
        "base admitted as writable overlay"
    );
    std::fs::remove_file(&input.overlay_qcow2)?;
    std::os::unix::fs::symlink(&input.base_raw, &input.overlay_qcow2)?;
    ensure!(
        QemuProcess::spawn_paused(&input).is_err(),
        "symlink overlay admitted"
    );
    ensure!(
        std::fs::read_dir(root.path())?.count() == 2,
        "failed pre-spawn validation leaked a runtime directory"
    );
    Ok(())
}

#[tokio::test]
async fn real_prepared_guest_starts_paused_and_owned_exit_preserves_disks() -> Result<()> {
    let root = tempfile::tempdir()?;
    let input = config(root.path())?;
    real_disk(&input).await?;
    let original_base = std::fs::read(&input.base_raw)?;
    let mut guest = QemuProcess::spawn_paused(&input)?;
    let runtime = guest.runtime_directory().to_owned();
    eprintln!(
        "owned prepared QEMU test: pid={}, stop=explicit owned stop; no guest OS",
        guest.pid()
    );
    let mut duplicate: Option<QemuProcess> = None;
    let result = tokio::time::timeout(Duration::from_secs(35), async {
        ensure!(
            std::fs::metadata(&runtime)?.permissions().mode() & 0o777 == 0o700,
            "monitor directory is not private"
        );
        ensure!(
            !guest.connect_monitor().await?.running,
            "guest was not paused"
        );
        guest.resume().await?;
        ensure!(guest.status().await?.running, "guest did not resume");
        guest.pause().await?;
        ensure!(!guest.status().await?.running, "guest did not pause");

        // The writable overlay must not admit another QEMU owner on this
        // host. This is a local file-lock check, not a distributed fence.
        duplicate = Some(QemuProcess::spawn_paused(&input)?);
        let duplicate_result = duplicate.as_mut().unwrap().wait_for_exit().await;
        ensure!(
            !duplicate_result?.success(),
            "second writer acquired the overlay"
        );
        ensure!(
            !guest.status().await?.running,
            "duplicate disturbed original owner"
        );
        Ok::<_, anyhow::Error>(())
    })
    .await;
    let duplicate_stopped = match duplicate.as_mut() {
        Some(duplicate) => duplicate.stop().await.map(|_| ()),
        None => Ok(()),
    };
    let stopped = guest.stop().await;
    duplicate_stopped?;
    result.context("prepared QEMU test deadline")??;
    stopped?;
    ensure!(
        guest.child.try_wait()?.is_some(),
        "owned process was not reaped"
    );
    guest.stop().await?; // Already-observed exit is safe and does not target another PID.
    drop(guest);
    ensure!(!runtime.exists(), "private runtime directory remained");
    ensure!(
        input.overlay_qcow2.is_file(),
        "persistent overlay was removed"
    );
    ensure!(
        std::fs::read(&input.base_raw)? == original_base,
        "base image was modified"
    );
    Ok(())
}

#[tokio::test]
async fn cancellation_retires_handshake_but_keeps_child_owned_until_stop() -> Result<()> {
    let root = tempfile::tempdir()?;
    let mut input = config(root.path())?;
    input.program = sleeping_program(root.path())?;
    let mut guest = QemuProcess::spawn_paused(&input)?;
    let result = async {
        let mut connecting = Box::pin(guest.connect_monitor());
        ensure!(
            futures_util::poll!(connecting.as_mut()).is_pending(),
            "fixture connected"
        );
        drop(connecting);
        ensure!(
            guest.connect_monitor().await.is_err(),
            "cancelled handshake was retried"
        );
        ensure!(
            guest.child.try_wait()?.is_none(),
            "live fixture lost its owner"
        );
        Ok::<_, anyhow::Error>(())
    }
    .await;
    let stopped = guest.stop().await;
    result?;
    stopped?;
    ensure!(
        guest.child.try_wait()?.is_some(),
        "cancelled child was not reaped"
    );
    Ok(())
}

#[tokio::test]
async fn monitor_rejects_a_peer_other_than_the_spawned_child() -> Result<()> {
    let root = tempfile::tempdir()?;
    let mut input = config(root.path())?;
    input.program = sleeping_program(root.path())?;
    let mut guest = QemuProcess::spawn_paused(&input)?;
    let result = async {
        let impostor = UnixStream::connect(guest.runtime_directory().join("qmp.sock")).await?;
        ensure!(
            guest.connect_monitor().await.is_err(),
            "foreign monitor peer was accepted"
        );
        drop(impostor);
        ensure!(guest.monitor.is_none(), "foreign monitor was retained");
        Ok::<_, anyhow::Error>(())
    }
    .await;
    let stopped = guest.stop().await;
    result?;
    stopped?;
    Ok(())
}

#[tokio::test]
async fn startup_exit_is_observed_without_waiting_for_monitor_timeout() -> Result<()> {
    let root = tempfile::tempdir()?;
    let mut input = config(root.path())?;
    input.program = PathBuf::from("/bin/false");
    let mut guest = QemuProcess::spawn_paused(&input)?;
    let result = tokio::time::timeout(Duration::from_secs(2), guest.connect_monitor()).await;
    let stopped = guest.stop().await;
    ensure!(
        result
            .context("startup failure waited for monitor deadline")?
            .is_err(),
        "failed process appeared connected"
    );
    stopped?;
    ensure!(
        guest.child.try_wait()?.is_some(),
        "failed startup child was not reaped"
    );
    Ok(())
}
