// Origin: CTOX
// License: AGPL-3.0-only

//! Native image-startup entry for the isolated Linux guest desktop endpoint.
//! It opens only the existing fixed virtio device and local X11 backend. It
//! never resolves a host workspace, opens CTOX state, or grants input authority.

#[cfg(any(target_os = "linux", all(test, unix)))]
use anyhow::Context;
use anyhow::{ensure, Result};
use std::path::Path;

fn config_path(args: &[String]) -> Result<&Path> {
    let [flag, value] = args else {
        anyhow::bail!("native guest desktop requires --config <absolute private JSON path>");
    };
    ensure!(flag == "--config", "unknown native guest startup argument");
    let path = Path::new(value);
    ensure!(
        path.is_absolute(),
        "guest startup config must be an absolute path"
    );
    Ok(path)
}

/// Called before normal CTOX root resolution. Provisioning supplies this
/// private typed configuration; model/Business OS payloads cannot start it.
pub(crate) fn run_native_guest_desktop(args: &[String]) -> Result<()> {
    let path = config_path(args)?;
    #[cfg(not(target_os = "linux"))]
    {
        let _ = path;
        anyhow::bail!("native guest desktop requires Linux");
    }
    #[cfg(target_os = "linux")]
    {
        let config = load_config(path)?;
        let driver = super::X11GuestDriver::new(super::X11GuestConfig {
            guest_id: config.guest_id,
            display: config.display,
            xauthority: config.xauthority,
        })?;
        // One runtime thread; actual process lifetime belongs to the guest
        // image supervisor. A closed/failed endpoint exits without reconnect.
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .context("could not start the guest desktop runtime")?;
        runtime
            .block_on(super::run_guest_desktop_effects(&driver))
            .context("guest desktop endpoint stopped")
    }
}

#[cfg(any(target_os = "linux", all(test, unix)))]
#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct GuestStartupConfig {
    guest_id: String,
    display: String,
    xauthority: std::path::PathBuf,
}

#[cfg(any(target_os = "linux", all(test, unix)))]
fn load_config(path: &Path) -> Result<GuestStartupConfig> {
    use std::io::Read;
    use std::os::unix::fs::{MetadataExt, OpenOptionsExt};

    const LIMIT: u64 = 4096;
    let file = std::fs::OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NOFOLLOW | libc::O_CLOEXEC | libc::O_NONBLOCK)
        .open(path)
        .context("could not open native guest startup config")?;
    let metadata = file.metadata()?;
    // SAFETY: geteuid only reads this process's native effective user ID.
    let native_uid = unsafe { libc::geteuid() };
    ensure!(
        metadata.is_file() && metadata.uid() == native_uid && metadata.mode() & 0o7177 == 0,
        "guest startup config must be a private regular file owned by the native user"
    );
    ensure!(metadata.len() <= LIMIT, "guest startup config is oversized");
    let mut bytes = Vec::new();
    file.take(LIMIT + 1).read_to_end(&mut bytes)?;
    ensure!(
        bytes.len() as u64 <= LIMIT,
        "guest startup config is oversized"
    );
    serde_json::from_slice(&bytes).context("invalid native guest startup config")
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt;

    fn write_config(directory: &Path, bytes: &[u8]) -> std::path::PathBuf {
        let path = directory.join("guest-startup.json");
        std::fs::write(&path, bytes).unwrap();
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600)).unwrap();
        path
    }

    #[test]
    fn startup_rejects_untrusted_config_before_opening_the_endpoint() {
        let directory = tempfile::tempdir().unwrap();
        let valid =
            br#"{"guest_id":"guest-1","display":":0","xauthority":"/home/guest/.Xauthority"}"#;
        let path = write_config(directory.path(), valid);
        let config = load_config(&path).unwrap();
        assert_eq!(config.guest_id, "guest-1");
        assert_eq!(config.display, ":0");
        assert_eq!(config.xauthority, Path::new("/home/guest/.Xauthority"));
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o644)).unwrap();
        assert!(load_config(&path).is_err());
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600)).unwrap();
        let alias = directory.path().join("alias.json");
        std::os::unix::fs::symlink(&path, &alias).unwrap();
        assert!(load_config(&alias).is_err());
        let fifo = directory.path().join("config.fifo");
        use std::os::unix::ffi::OsStrExt;
        let fifo_name = std::ffi::CString::new(fifo.as_os_str().as_bytes()).unwrap();
        // SAFETY: the NUL-terminated path points into this owned test directory.
        assert_eq!(unsafe { libc::mkfifo(fifo_name.as_ptr(), 0o600) }, 0);
        assert!(load_config(&fifo).is_err());
        write_config(directory.path(), br#"{"guest_id":"guest-1","display":":0","xauthority":"/home/guest/.Xauthority","controller_id":"forged"}"#);
        assert!(load_config(&path).is_err());
        write_config(directory.path(), &vec![b' '; 4097]);
        assert!(load_config(&path).is_err());
    }

    #[test]
    fn startup_arguments_cannot_select_a_port_or_host_workspace() {
        let valid = ["--config".into(), "/native/guest.json".into()];
        assert_eq!(
            config_path(&valid).unwrap(),
            Path::new("/native/guest.json")
        );
        for args in [
            vec![],
            vec!["--config".into(), "relative.json".into()],
            vec!["--port".into(), "/dev/other".into()],
            vec![
                "--config".into(),
                "/native/guest.json".into(),
                "--root".into(),
                "/host".into(),
            ],
        ] {
            assert!(config_path(&args).is_err());
        }
    }
}
