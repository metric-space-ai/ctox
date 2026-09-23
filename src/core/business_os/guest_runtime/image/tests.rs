// Origin: CTOX
// License: AGPL-3.0-only

use super::super::qemu::{PreparedQemuGuest, QemuAcceleration, QemuProcess};
use super::*;
use std::future::{poll_fn, Future};
use std::task::Poll;

fn base(root: &Path) -> Result<PathBuf> {
    let path = root.join("base,readonly=off.raw");
    File::create(&path)?.set_len(16 * 1024 * 1024)?;
    Ok(path)
}

#[tokio::test]
async fn invalid_base_does_not_create_state_or_replace_an_existing_disk() -> Result<()> {
    let root = tempfile::tempdir()?;
    let original = root.path().join("root.qcow2");
    std::fs::write(&original, b"existing worker state")?;
    let invalid = root.path().join("invalid.raw");
    std::fs::write(&invalid, b"not sector aligned")?;
    let linked = root.path().join("linked.raw");
    std::os::unix::fs::symlink(&invalid, &linked)?;
    for image in [&invalid, &linked] {
        ensure!(
            QemuOverlayPreparation::start(Path::new("/bin/false"), root.path(), image).is_err(),
            "invalid base admitted"
        );
    }
    ensure!(
        std::fs::read(&original)? == b"existing worker state",
        "existing disk changed"
    );
    ensure!(
        std::fs::read_dir(root.path())?.count() == 3,
        "failed validation created state"
    );
    Ok(())
}

#[tokio::test]
async fn failed_preparation_is_retired_and_only_its_unpublished_directory_is_removed() -> Result<()>
{
    let root = tempfile::tempdir()?;
    let image = base(root.path())?;
    let mut preparation =
        QemuOverlayPreparation::start(Path::new("/bin/false"), root.path(), &image)?;
    let private = preparation.directory.as_ref().unwrap().path().to_path_buf();
    let result = preparation.finish().await;
    let aborted = preparation.abort().await;
    ensure!(result.is_err(), "unsuccessful helper published a disk");
    aborted?;
    ensure!(
        preparation.child.try_wait()?.is_some(),
        "helper was not reaped"
    );
    ensure!(
        !private.exists() && image.exists(),
        "abort removed wrong state"
    );
    ensure!(
        preparation.finish().await.is_err(),
        "failed preparation was retried"
    );
    Ok(())
}

#[tokio::test]
async fn cancelled_preparation_retains_child_until_explicit_abort() -> Result<()> {
    let root = tempfile::tempdir()?;
    let image = base(root.path())?;
    let sleeper = root.path().join("image-fixture");
    std::fs::write(&sleeper, b"#!/bin/sh\nexec /bin/sleep 30\n")?;
    std::fs::set_permissions(&sleeper, std::fs::Permissions::from_mode(0o700))?;
    let mut preparation = QemuOverlayPreparation::start(&sleeper, root.path(), &image)?;
    let private = preparation.directory.as_ref().unwrap().path().to_path_buf();
    let pending = {
        let mut finishing = Box::pin(preparation.finish());
        poll_fn(|cx| Poll::Ready(finishing.as_mut().poll(cx).is_pending())).await
    };
    let result = async {
        ensure!(pending, "fixture did not suspend in preparation");
        ensure!(
            preparation.child.try_wait()?.is_none(),
            "cancelled helper ownership was lost"
        );
        ensure!(
            preparation.finish().await.is_err(),
            "cancelled preparation was retried"
        );
        Ok::<_, anyhow::Error>(())
    }
    .await;
    let aborted = preparation.abort().await;
    result?;
    aborted?;
    ensure!(
        preparation.child.try_wait()?.is_some(),
        "cancelled helper was not reaped"
    );
    ensure!(
        !private.exists() && image.exists(),
        "cancelled preparation cleanup failed"
    );
    Ok(())
}

#[tokio::test]
async fn real_overlay_is_small_retained_and_used_by_the_owned_qemu_process() -> Result<()> {
    let root = tempfile::tempdir()?;
    let image = base(root.path())?;
    let existing = root.path().join("root.qcow2");
    std::fs::write(&existing, b"existing worker state")?;
    let mut preparation =
        QemuOverlayPreparation::start(Path::new("/usr/bin/qemu-img"), root.path(), &image)?;
    let mut second =
        QemuOverlayPreparation::start(Path::new("/usr/bin/qemu-img"), root.path(), &image)?;
    let mut guest: Option<QemuProcess> = None;
    let result = tokio::time::timeout(Duration::from_secs(30), async {
        let overlay = preparation.finish().await?;
        let other_overlay = second.finish().await?;
        ensure!(overlay != other_overlay, "workers shared a writable disk");
        ensure!(
            std::fs::metadata(overlay.parent().unwrap())?
                .permissions()
                .mode()
                & 0o777
                == 0o700,
            "guest disk directory is not private"
        );
        ensure!(
            std::fs::metadata(&overlay)?.len() < std::fs::metadata(&image)?.len(),
            "base was copied"
        );
        let config = PreparedQemuGuest {
            program: PathBuf::from("/usr/bin/qemu-system-x86_64"),
            runtime_parent: root.path().to_owned(),
            base_raw: image.clone(),
            overlay_qcow2: overlay.clone(),
            memory_mib: 64,
            vcpus: 1,
            acceleration: QemuAcceleration::Tcg,
        };
        guest = Some(QemuProcess::spawn_paused(&config)?);
        ensure!(
            !guest.as_mut().unwrap().connect_monitor().await?.running,
            "prepared guest was not paused"
        );
        Ok::<_, anyhow::Error>((overlay, other_overlay))
    })
    .await;
    let stopped = match guest.as_mut() {
        Some(guest) => guest.stop().await.map(|_| ()),
        None => Ok(()),
    };
    let aborted = preparation.abort().await;
    let second_aborted = second.abort().await;
    stopped?;
    aborted?;
    second_aborted?;
    let (overlay, other_overlay) = result.context("overlay integration deadline")??;
    drop(guest);
    drop(preparation);
    drop(second);
    ensure!(
        overlay.is_file() && other_overlay.is_file(),
        "completed disks were removed"
    );
    ensure!(
        std::fs::read(&existing)? == b"existing worker state",
        "existing disk was overwritten"
    );
    ensure!(
        std::fs::read(&image)?.iter().all(|byte| *byte == 0),
        "base image changed"
    );
    Ok(())
}
