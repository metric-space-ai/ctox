//! CTOX adapter for the native Sync host and its provisioned signing identity.
pub(crate) const SIGNING_IDENTITY_SECRET_KEY: (&str, &str) = ("ctox-sync-host", "identity-pkcs8");
/// Decode only bytes borrowed from the caller's current encrypted-store fence.
pub(crate) fn signing_identity_from_record(
    encoded: &[u8],
) -> anyhow::Result<ctox_sync::authority::auth::SigningIdentity> {
    #[cfg(unix)]
    {
        unix::decode_key(encoded)
    }
    #[cfg(not(unix))]
    {
        let _ = encoded;
        anyhow::bail!("native Sync identity is unavailable on this platform")
    }
}
#[cfg(unix)]
#[path = "sync_host/control_channel.rs"]
mod control_channel;
#[cfg(unix)]
pub(crate) use control_channel::{
    native_control_channel, NativeControlChannel, NativeControlPeer, NativeControlReplyVerifier,
};
#[cfg(unix)]
#[path = "sync_host/unix.rs"]
mod unix;
#[cfg(all(unix, test))]
pub(crate) use unix::guests::serve_connection as serve_native_guest_enrollment;
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

/// Executes one synchronous authority decision using the current provisioned
/// native Sync identity. The encrypted issuer is held until the callback
/// returns; no key/store initialization, await or secret API reentry is allowed.
pub(crate) fn with_current_signing_identity<T>(
    root: &std::path::Path,
    apply: impl FnOnce(&ctox_sync::authority::auth::SigningIdentity) -> anyhow::Result<T>,
) -> anyhow::Result<T> {
    #[cfg(unix)]
    {
        unix::with_current_key(root, apply)
    }
    #[cfg(not(unix))]
    {
        let _ = (root, apply);
        anyhow::bail!("native Sync identity is unavailable on this platform")
    }
}

#[cfg(unix)]
pub(crate) fn handoff_configuration(
    root: &std::path::Path,
) -> anyhow::Result<ctox_sync::host_config::HostConfiguration> {
    unix::configuration(root)
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
