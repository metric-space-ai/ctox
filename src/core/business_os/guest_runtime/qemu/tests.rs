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
    // Establish valid admission without requiring an installed QEMU binary.
    // Otherwise missing program validation can mask disk-alias rejection.
    input.program = sleeping_program(root.path())?;
    let mut admitted = QemuProcess::spawn_paused(&input, "isolated-ci-guest")?;
    admitted.stop().await?;
    drop(admitted);
    input.vcpus = 0;
    ensure!(
        QemuProcess::spawn_paused(&input, "isolated-ci-guest").is_err(),
        "zero vCPUs admitted"
    );
    input.vcpus = 3;
    ensure!(
        QemuProcess::spawn_paused(&input, "isolated-ci-guest").is_err(),
        "vCPU limit exceeded"
    );
    input.vcpus = 1;
    input.memory_mib = 4097;
    ensure!(
        QemuProcess::spawn_paused(&input, "isolated-ci-guest").is_err(),
        "memory limit exceeded"
    );
    input.memory_mib = 64;
    std::fs::remove_file(&input.overlay_qcow2)?;
    std::fs::hard_link(&input.base_raw, &input.overlay_qcow2)?;
    ensure!(
        QemuProcess::spawn_paused(&input, "isolated-ci-guest").is_err(),
        "base admitted as writable overlay"
    );
    std::fs::remove_file(&input.overlay_qcow2)?;
    std::os::unix::fs::symlink(&input.base_raw, &input.overlay_qcow2)?;
    ensure!(
        QemuProcess::spawn_paused(&input, "isolated-ci-guest").is_err(),
        "symlink overlay admitted"
    );
    ensure!(
        std::fs::read_dir(root.path())?.count() == 3,
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
    let mut guest = QemuProcess::spawn_paused(&input, "isolated-ci-guest")?;
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
        guest.connect_guest_channel().await?;
        ensure!(
            guest.guest_channel().is_ok(),
            "owned QEMU did not connect the guest channel"
        );
        ensure!(
            guest.bind_guest_driver("".into()).is_err(),
            "empty guest identity bound a driver"
        );
        ensure!(
            guest.guest_channel().is_ok(),
            "invalid bind consumed the guest channel"
        );
        let driver = guest.bind_guest_driver("isolated-ci-guest".into())?;
        ensure!(driver.guest_id() == "isolated-ci-guest");
        ensure!(
            guest.guest_channel().is_err(),
            "guest channel remained after driver bind"
        );
        guest.resume().await?;
        ensure!(guest.status().await?.running, "guest did not resume");
        guest.pause().await?;
        ensure!(!guest.status().await?.running, "guest did not pause");

        // The writable overlay must not admit another QEMU owner on this
        // host. This is a local file-lock check, not a distributed fence.
        duplicate = Some(QemuProcess::spawn_paused(&input, "isolated-ci-guest")?);
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
    let mut guest = QemuProcess::spawn_paused(&input, "isolated-ci-guest")?;
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
    let mut guest = QemuProcess::spawn_paused(&input, "isolated-ci-guest")?;
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
    let mut guest = QemuProcess::spawn_paused(&input, "isolated-ci-guest")?;
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

#[tokio::test]
async fn spawn_binds_exact_assignment_config_and_retires_it_with_owned_child() -> Result<()> {
    use std::os::unix::fs::MetadataExt;

    let root = tempfile::tempdir()?;
    let mut input = config(root.path())?;
    input.program = sleeping_program(root.path())?;
    ensure!(QemuProcess::spawn_paused(&input, "").is_err());
    ensure!(QemuProcess::spawn_paused(&input, "guest\nother").is_err());
    let guest_id = "enrolled-guest,\"quoted\"";
    let mut guest = QemuProcess::spawn_paused(&input, guest_id)?;
    let runtime = guest.runtime_directory().to_owned();
    let path = runtime.join("guest-startup.json");
    let result = (|| -> Result<()> {
        let metadata = std::fs::symlink_metadata(&path)?;
        ensure!(metadata.is_file() && metadata.mode() & 0o7777 == 0o600);
        // SAFETY: geteuid only reads the test process's effective user ID.
        let native_uid = unsafe { libc::geteuid() };
        ensure!(metadata.uid() == native_uid && metadata.nlink() == 1);
        let value: serde_json::Value = serde_json::from_slice(&std::fs::read(&path)?)?;
        ensure!(
            value
                == json!({
                    "guest_id": guest_id,
                    "display": ":0",
                    "xauthority": "/run/ctox-desktop/Xauthority"
                })
        );
        let command = prepare_command(
            &input,
            &runtime.join("qmp.sock"),
            &runtime.join("guest.sock"),
            &path,
        )?;
        let args: Vec<_> = command.as_std().get_args().collect();
        ensure!(args.windows(2).any(|pair| pair[0] == "-fw_cfg"
            && pair[1]
                == std::ffi::OsStr::new(&format!(
                    "name=opt/org.ctox/guest-startup,file={}",
                    path.display()
                ))));
        Ok(())
    })();
    let stopped = guest.stop().await;
    result?;
    stopped?;
    drop(guest);
    ensure!(
        !runtime.exists(),
        "stopped guest startup config was retained"
    );
    ensure!(input.overlay_qcow2.is_file() && input.base_raw.is_file());
    Ok(())
}

#[tokio::test]
async fn spawn_binds_a_private_guest_channel_socket() -> Result<()> {
    let root = tempfile::tempdir()?;
    let mut input = config(root.path())?;
    input.program = sleeping_program(root.path())?;
    let mut guest = QemuProcess::spawn_paused(&input, "isolated-ci-guest")?;
    let result = async {
        let runtime = guest.runtime_directory();
        ensure!(
            runtime.join("guest.sock").exists(),
            "guest channel socket missing"
        );
        ensure!(
            std::fs::metadata(runtime)?.permissions().mode() & 0o777 == 0o700,
            "guest channel directory is not private"
        );
        ensure!(
            guest.guest_listener.is_some() && guest.guest_channel.is_none(),
            "guest channel handshake started before connect"
        );
        Ok::<_, anyhow::Error>(())
    }
    .await;
    let stopped = guest.stop().await;
    result?;
    stopped?;
    Ok(())
}

#[tokio::test]
async fn guest_channel_rejects_a_peer_other_than_the_spawned_child() -> Result<()> {
    let root = tempfile::tempdir()?;
    let mut input = config(root.path())?;
    input.program = sleeping_program(root.path())?;
    let mut guest = QemuProcess::spawn_paused(&input, "isolated-ci-guest")?;
    let result = async {
        let impostor = UnixStream::connect(guest.runtime_directory().join("guest.sock")).await?;
        ensure!(
            guest.connect_guest_channel().await.is_err(),
            "foreign guest-channel peer was accepted"
        );
        drop(impostor);
        ensure!(
            guest.guest_channel.is_none(),
            "foreign guest channel was retained"
        );
        Ok::<_, anyhow::Error>(())
    }
    .await;
    let stopped = guest.stop().await;
    result?;
    stopped?;
    Ok(())
}

#[tokio::test]
async fn cancellation_retires_guest_channel_handshake_but_keeps_child_owned() -> Result<()> {
    let root = tempfile::tempdir()?;
    let mut input = config(root.path())?;
    input.program = sleeping_program(root.path())?;
    let mut guest = QemuProcess::spawn_paused(&input, "isolated-ci-guest")?;
    let result = async {
        let mut connecting = Box::pin(guest.connect_guest_channel());
        ensure!(
            futures_util::poll!(connecting.as_mut()).is_pending(),
            "fixture connected guest channel"
        );
        drop(connecting);
        ensure!(
            guest.connect_guest_channel().await.is_err(),
            "cancelled guest-channel handshake was retried"
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
    Ok(())
}

#[tokio::test]
async fn real_memory_export_and_incoming_restore_stay_paused_until_explicit_resume() -> Result<()> {
    use std::os::unix::fs::OpenOptionsExt;
    let root = tempfile::tempdir()?;
    let input = config(root.path())?;
    real_disk(&input).await?;
    let path = root.path().join("memory.state");
    let file = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(&path)?;
    let mut file = tokio::fs::File::from_std(file);
    let mut source = QemuProcess::spawn_paused(&input, "isolated-memory-guest")?;
    let mut target: Option<QemuProcess> = None;
    eprintln!(
        "owned memory source pid={} stop=test-finally; no guest OS",
        source.pid()
    );
    let result = tokio::time::timeout(Duration::from_secs(60), async {
        source.connect_monitor().await?;
        source.resume().await?;
        tokio::time::sleep(Duration::from_millis(20)).await;
        source.pause().await?;
        let memory = source.save_memory(&mut file).await?;
        ensure!(memory.bytes > 0 && memory.sha256.len() == 64);
        ensure!(std::fs::metadata(&path)?.permissions().mode() & 0o777 == 0o400);
        ensure!(
            source.resume().await.is_err(),
            "exported source could execute again"
        );
        ensure!(source.status().await?.status == "postmigrate");
        ensure!(
            source.finish_memory_export().await?.success(),
            "source quit was not clean"
        );
        ensure!(
            source.child.try_wait()?.is_some(),
            "source disk writer was not reaped"
        );

        target = Some(QemuProcess::spawn_incoming(
            &input,
            "isolated-memory-guest",
        )?);
        let target = target.as_mut().unwrap();
        eprintln!(
            "owned memory target pid={} stop=test-finally; no guest OS",
            target.pid()
        );
        target.connect_monitor().await?;
        ensure!(
            target.resume().await.is_err(),
            "incomplete incoming guest executed"
        );
        let mut saved = tokio::fs::File::open(&path).await?;
        target.restore_memory(&mut saved, &memory).await?;
        let status = target.status().await?;
        ensure!(
            !status.running && status.status == "paused",
            "restore automatically executed"
        );
        ensure!(
            target.restore_memory(&mut saved, &memory).await.is_err(),
            "restore replayed"
        );
        target.resume().await?;
        ensure!(
            target.status().await?.running,
            "explicit native resume failed"
        );
        Ok::<_, anyhow::Error>(())
    })
    .await;
    let target_stopped = match target.as_mut() {
        Some(target) => target.stop().await.map(|_| ()),
        None => Ok(()),
    };
    let source_stopped = source.stop().await;
    target_stopped?;
    source_stopped?;
    result.context("actual memory roundtrip deadline")??;
    Ok(())
}

#[tokio::test]
async fn corrupt_memory_retires_incoming_attempt_without_executing_or_releasing_child() -> Result<()>
{
    use std::os::unix::fs::OpenOptionsExt;
    let root = tempfile::tempdir()?;
    let input = config(root.path())?;
    real_disk(&input).await?;
    let path = root.path().join("corrupt.state");
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(&path)?;
    std::io::Write::write_all(&mut file, b"corrupted stream")?;
    file.sync_all()?;
    file.set_permissions(std::fs::Permissions::from_mode(0o400))?;
    drop(file);
    let mut saved = tokio::fs::File::open(&path).await?;
    let expected = QemuMemoryState {
        bytes: 16,
        sha256: "0".repeat(64),
    };
    let mut target = QemuProcess::spawn_incoming(&input, "isolated-corrupt-memory")?;
    eprintln!(
        "owned corrupt-memory target pid={} stop=test-finally",
        target.pid()
    );
    let result = tokio::time::timeout(Duration::from_secs(20), async {
        target.connect_monitor().await?;
        ensure!(
            target.restore_memory(&mut saved, &expected).await.is_err(),
            "corrupt input accepted"
        );
        ensure!(
            target.restore_memory(&mut saved, &expected).await.is_err(),
            "failed incoming attempt retried"
        );
        ensure!(target.resume().await.is_err(), "failed restore executed");
        let status = target.status().await?;
        ensure!(
            !status.running && status.status == "inmigrate",
            "corrupt stream reached QEMU"
        );
        ensure!(
            target.child.try_wait()?.is_none(),
            "failed restore lost child ownership"
        );
        Ok::<_, anyhow::Error>(())
    })
    .await;
    let stopped = target.stop().await;
    stopped?;
    result.context("corrupt-memory deadline")??;
    ensure!(
        target.child.try_wait()?.is_some(),
        "failed target was not reaped"
    );
    Ok(())
}

#[tokio::test]
async fn real_qemu_survives_retirement_of_its_calling_thread() -> Result<()> {
    let root = tempfile::tempdir()?;
    let input = config(root.path())?;
    real_disk(&input).await?;
    let runtime = tokio::runtime::Handle::current();
    let caller = std::thread::spawn(move || {
        let _runtime = runtime.enter();
        QemuProcess::spawn_paused(&input, "isolated-retired-caller")
    });
    // The calling thread has actually exited before the retained owner is used.
    let mut guest = caller
        .join()
        .map_err(|_| anyhow!("native QEMU caller fixture panicked"))??;
    let result = tokio::time::timeout(Duration::from_secs(20), async {
        guest.connect_monitor().await?;
        ensure!(
            !guest.status().await?.running,
            "retained guest executed before authorization"
        );
        guest.ensure_alive()?;
        guest.monitor()?.quit().await?;
        ensure!(
            guest.wait_for_exit().await?.success(),
            "retained guest did not confirm a clean QMP quit"
        );
        Ok::<_, anyhow::Error>(())
    })
    .await;
    let stopped = guest.stop().await;
    result.context("retired-caller QEMU deadline")??;
    ensure!(
        stopped?.success(),
        "retained guest did not stop successfully"
    );
    ensure!(
        guest.child.try_wait()?.is_some(),
        "retained child was not reaped"
    );
    Ok(())
}

#[tokio::test]
async fn real_qemu_cannot_survive_abrupt_native_parent_exit() -> Result<()> {
    use std::os::fd::{AsRawFd, FromRawFd, OwnedFd};
    const CHILD_ROOT: &str = "CTOX_QEMU_PARENT_EXIT_TEST_ROOT";
    const TEST: &str =
        "business_os::guest_runtime::qemu::tests::real_qemu_cannot_survive_abrupt_native_parent_exit";
    if let Some(root) = std::env::var_os(CHILD_ROOT) {
        let root = PathBuf::from(root);
        let input = config(&root)?;
        real_disk(&input).await?;
        let mut guest = QemuProcess::spawn_paused(&input, "isolated-parent-exit-guest")?;
        guest.connect_monitor().await?;
        ensure!(!guest.status().await?.running);
        // A private witness avoids libtest's inline progress prefix on stdout.
        std::fs::write(root.join("parent-exit-qemu.pid"), guest.pid().to_string())?;
        // Deliberately bypass every Rust destructor, as SIGABRT does.
        std::process::exit(0);
    }
    let root = tempfile::tempdir()?;
    let child = Command::new(std::env::current_exe()?)
        .args(["--exact", TEST, "--nocapture", "--test-threads=1"])
        .env(CHILD_ROOT, root.path())
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true)
        .spawn()?;
    let output = tokio::time::timeout(Duration::from_secs(20), child.wait_with_output())
        .await
        .context("native parent fixture deadline")??;
    ensure!(
        output.status.success(),
        "native parent fixture failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let pid: i32 = std::fs::read_to_string(root.path().join("parent-exit-qemu.pid"))
        .context("actual QEMU child identity missing")?
        .parse()?;
    // SAFETY: pidfd pins this exact process; an already-reaped PID is absent.
    // It avoids signaling another process if a numeric PID is subsequently reused.
    let raw = unsafe { libc::syscall(libc::SYS_pidfd_open, pid, 0) };
    if raw < 0 {
        ensure!(
            std::io::Error::last_os_error().raw_os_error() == Some(libc::ESRCH),
            "cannot observe QEMU parent-exit result"
        );
        return Ok(());
    }
    // SAFETY: successful pidfd_open returns a fresh descriptor owned here.
    let fd = unsafe { OwnedFd::from_raw_fd(raw as i32) };
    let mut event = libc::pollfd {
        fd: fd.as_raw_fd(),
        events: libc::POLLIN,
        revents: 0,
    };
    // SAFETY: event is a valid single pollfd; this wait is bounded.
    let observed_exit =
        unsafe { libc::poll(&mut event, 1, 5000) } > 0 && event.revents & libc::POLLIN != 0;
    if !observed_exit {
        let argv = std::fs::read(format!("/proc/{pid}/cmdline"))?;
        let assigned = root.path().as_os_str().as_encoded_bytes();
        ensure!(
            argv.windows(assigned.len()).any(|part| part == assigned),
            "numeric QEMU PID no longer belongs to this test; no signal sent"
        );
        // A failed regression must still retire only its exact owned child.
        // SAFETY: this syscall targets our retained pidfd, with no siginfo.
        let stopped = unsafe {
            libc::syscall(
                libc::SYS_pidfd_send_signal,
                fd.as_raw_fd(),
                libc::SIGKILL,
                std::ptr::null::<libc::siginfo_t>(),
                0,
            )
        };
        ensure!(stopped == 0, "cannot retire failed owned parent-exit child");
        // SAFETY: same live descriptor and bounded wait as above.
        ensure!(
            unsafe { libc::poll(&mut event, 1, 5000) } > 0,
            "failed owned child did not exit"
        );
    }
    ensure!(observed_exit, "QEMU survived abrupt native parent exit");
    Ok(())
}
