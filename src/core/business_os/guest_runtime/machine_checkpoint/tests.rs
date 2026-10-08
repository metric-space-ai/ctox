// Origin: CTOX
// License: AGPL-3.0-only

use super::*;
use std::io::Write;

fn file(path: &Path, bytes: &[u8]) -> Result<File> {
    let mut f = OpenOptions::new()
        .read(true)
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(path)?;
    f.write_all(bytes)?;
    f.sync_all()?;
    Ok(f)
}

fn fixture(root: &Path) -> Result<(CheckpointStore, Vec<WorkspaceEntry>, PreparedQemuGuest)> {
    let store = CheckpointStore::open(root.join("store"), 64 * 1024 * 1024)?;
    let base = file(&root.join("base.raw"), &[0x51; 512])?;
    base.set_permissions(std::fs::Permissions::from_mode(0o400))?;
    let disk = file(&root.join("source.qcow2"), &vec![0x52; 8 * 1024 * 1024 + 7])?;
    let mut ram = file(&root.join("source.ram"), &[0x53; 1024])?;
    ram.set_permissions(std::fs::Permissions::from_mode(0o400))?;
    let config = PreparedQemuGuest {
        program: "/usr/bin/qemu-system-x86_64".into(),
        runtime_parent: root.into(),
        base_raw: root.join("base.raw"),
        overlay_qcow2: root.join("source.qcow2"),
        memory_mib: 128,
        vcpus: 1,
        acceleration: super::super::QemuAcceleration::Tcg,
    };
    // A byte-codec fixture only; no claim of QEMU/source-effect acceptance.
    let witness = QuiescedQemuCheckpoint::after_clean_exit(
        config.clone(),
        "guest-o04".into(),
        GuestLiveEndpoint {
            process_instance_id: "qemu:123:fixture".into(),
            guest_session_id: "service-o04".into(),
            endpoint_id: "endpoint-o04".into(),
        },
        QemuMemoryState {
            bytes: 1024,
            sha256: format!("{:x}", Sha256::digest([0x53; 1024])),
        },
    )?;
    drop(disk);
    let entries = witness.store(&store, &mut ram)?;
    Ok((store, entries, config))
}

fn stage(
    root: &Path,
    name: &str,
    store: &CheckpointStore,
    entries: &[WorkspaceEntry],
    config: &PreparedQemuGuest,
    guest: &str,
    service: &str,
) -> Result<StagedQemuCheckpoint> {
    let mut ram = file(&root.join(format!("{name}.ram")), &[])?;
    let mut disk = file(&root.join(format!("{name}.qcow2")), &[])?;
    let mut target = config.clone();
    target.overlay_qcow2 = root.join(format!("{name}.qcow2"));
    StagedQemuCheckpoint::stage(store, entries, target, guest, service, &mut ram, &mut disk)
}

#[tokio::test]
async fn real_incoming_machine_load_is_bound_to_its_exact_retained_child() -> Result<()> {
    use super::super::{image::QemuOverlayPreparation, qemu::QemuProcess, RetainedQemuDesktop};
    let root = tempfile::tempdir()?;
    let base = file(&root.path().join("base.raw"), &[0; 512])?;
    base.set_len(1024 * 1024)?;
    base.set_permissions(std::fs::Permissions::from_mode(0o400))?;
    let mut preparation = QemuOverlayPreparation::start(
        Path::new("/usr/bin/qemu-img"),
        root.path(),
        &root.path().join("base.raw"),
    )?;
    let overlay = preparation.finish().await?;
    let config = PreparedQemuGuest {
        program: "/usr/bin/qemu-system-x86_64".into(),
        runtime_parent: root.path().into(),
        base_raw: root.path().join("base.raw"),
        overlay_qcow2: overlay,
        memory_mib: 64,
        vcpus: 1,
        acceleration: super::super::QemuAcceleration::Tcg,
    };
    let mut source = QemuProcess::spawn_paused(&config, "guest-o04")?;
    source.connect_monitor().await?;
    let initial = source.status().await?;
    ensure!(
        !initial.running && initial.status == "prelaunch",
        "machine checkpoint fixture must preserve a never-started source"
    );
    let mut ram = tokio::fs::File::from_std(file(&root.path().join("source.ram"), &[])?);
    let state = source.save_memory(&mut ram).await?;
    source.finish_memory_export().await?;
    let mut ram = ram.into_std().await;
    let store = CheckpointStore::open(root.path().join("store"), 64 * 1024 * 1024)?;
    // Actual paused-QEMU/CAS/incoming connection. Service identity here is a
    // fixture: this test never activates or claims live guest readiness.
    let witness = QuiescedQemuCheckpoint::after_clean_exit(
        config.clone(),
        "guest-o04".into(),
        GuestLiveEndpoint {
            process_instance_id: "paused-source-fixture".into(),
            guest_session_id: "service-o04".into(),
            endpoint_id: "unactivated-fixture".into(),
        },
        state,
    )?;
    let entries = witness.store(&store, &mut ram)?;
    let mut staged = stage(
        root.path(),
        "actual-target",
        &store,
        &entries,
        &config,
        "guest-o04",
        "service-o04",
    )?;
    let mut other = stage(
        root.path(),
        "other-target",
        &store,
        &entries,
        &config,
        "guest-o04",
        "service-o04",
    )?;
    let mut target = RetainedQemuDesktop::spawn_checkpoint(&mut staged)?;
    let foreign_rejected = target.load_checkpoint(&mut other).await.is_err();
    let loaded = target.load_checkpoint(&mut staged).await;
    let stopped = target.stop().await?;
    ensure!(
        foreign_rejected,
        "a different staging attempt entered this retained child"
    );
    loaded?;
    // stop is forced cleanup, never a clean source-checkpoint witness.
    ensure!(
        std::os::unix::process::ExitStatusExt::signal(&stopped) == Some(libc::SIGKILL),
        "actual target was not retained until forced cleanup and reap"
    );
    Ok(())
}

#[test]
fn machine_checkpoint_binds_original_service_base_ram_and_multichunk_disk() -> Result<()> {
    let root = tempfile::tempdir()?;
    let (store, entries, config) = fixture(root.path())?;
    ensure!(
        entries
            .iter()
            .filter(|e| e.path.starts_with("native-guest-disk/"))
            .count()
            == 2
    );
    ensure!(entries
        .iter()
        .any(|e| e.path == "native-guest-machine.json"));
    let staged = stage(
        root.path(),
        "complete",
        &store,
        &entries,
        &config,
        "guest-o04",
        "service-o04",
    )?;
    ensure!(staged.guest_id == "guest-o04" && staged.service_session == "service-o04");
    ensure!(staged.disk.metadata()?.mode() & 0o777 == 0o400);
    ensure!(staged.memory.metadata()?.mode() & 0o777 == 0o400);
    ensure!(staged.disk_identity.bytes == 8 * 1024 * 1024 + 7);
    Ok(())
}

#[test]
fn incomplete_machine_or_foreign_assignment_cannot_complete_staging() -> Result<()> {
    let root = tempfile::tempdir()?;
    let (store, original, config) = fixture(root.path())?;
    for case in [
        "disk-missing",
        "disk-duplicate",
        "ram-missing",
        "metadata-duplicate",
        "wrong-guest",
        "wrong-session",
        "wrong-profile",
        "wrong-base",
    ] {
        let mut entries = original.clone();
        let mut target = config.clone();
        let mut guest = "guest-o04";
        let mut service = "service-o04";
        match case {
            "disk-missing" => entries.retain(|e| e.path != "native-guest-disk/0001.bin"),
            "disk-duplicate" => entries.push(
                entries
                    .iter()
                    .find(|e| e.path == "native-guest-disk/0000.bin")
                    .unwrap()
                    .clone(),
            ),
            "ram-missing" => entries.retain(|e| e.path != "native-guest-memory/0000.bin"),
            "metadata-duplicate" => entries.push(
                entries
                    .iter()
                    .find(|e| e.path == MACHINE_PATH)
                    .unwrap()
                    .clone(),
            ),
            "wrong-guest" => guest = "foreign-guest",
            "wrong-session" => service = "foreign-session",
            "wrong-profile" => target.memory_mib += 1,
            "wrong-base" => {
                let other = file(&root.path().join("other-base.raw"), &[0x54; 512])?;
                other.set_permissions(std::fs::Permissions::from_mode(0o400))?;
                target.base_raw = root.path().join("other-base.raw");
            }
            _ => unreachable!(),
        }
        ensure!(
            stage(root.path(), case, &store, &entries, &target, guest, service).is_err(),
            "{case} accepted"
        );
    }
    Ok(())
}

#[test]
fn changed_staged_disk_or_base_retires_the_incoming_attempt() -> Result<()> {
    let root = tempfile::tempdir()?;
    let (store, entries, config) = fixture(root.path())?;
    for kind in ["disk", "base"] {
        let mut staged = stage(
            root.path(),
            kind,
            &store,
            &entries,
            &config,
            "guest-o04",
            "service-o04",
        )?;
        let path = if kind == "disk" {
            &staged.config.overlay_qcow2
        } else {
            &staged.config.base_raw
        };
        // Native staging retains writable descriptors: chmod alone is not immutability.
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600))?;
        let mut writer = OpenOptions::new().write(true).open(path)?;
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o400))?;
        writer.write_all(&[0x55])?;
        writer.sync_all()?;
        ensure!(
            staged.prepare_spawn().is_err(),
            "changed {kind} was admitted"
        );
        ensure!(staged.prepare_spawn().is_err(), "failed attempt replayed");
    }
    Ok(())
}
