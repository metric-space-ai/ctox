//! CTOX adapter for the native Sync host and its provisioned signing identity.
#[cfg(unix)]
#[path = "sync_host/unix.rs"]
mod unix;
#[cfg(unix)]
pub use unix::{handle_command, start_if_configured};

/// Reuse the provisioned native Sync identity. Reading never creates or rotates
/// a key; enrollment must distribute its public identity through a trusted path.
pub(crate) fn signing_identity(
    root: &std::path::Path,
) -> anyhow::Result<std::sync::Arc<ctox_sync::authority::auth::SigningIdentity>> {
    #[cfg(unix)]
    {
        unix::key(root)
    }
    #[cfg(not(unix))]
    {
        let _ = root;
        anyhow::bail!("native Sync identity is unavailable on this platform")
    }
}

#[cfg(not(unix))]
pub fn handle_command(_: &std::path::Path, _: &[String]) -> anyhow::Result<()> {
    anyhow::bail!("native Sync hosting requires a certified local listener on this platform")
}
#[cfg(not(unix))]
pub fn start_if_configured(root: &std::path::Path) -> anyhow::Result<Option<()>> {
    let path = crate::inference::runtime_env::runtime_config_path(root);
    if path.exists() {
        let connection = rusqlite::Connection::open_with_flags(
            path,
            rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
        )?;
        anyhow::ensure!(
            ctox_sync::host_config::load(&connection)?.is_none(),
            "configured native Sync host requires a certified local listener on this platform"
        );
    }
    Ok(None)
}
