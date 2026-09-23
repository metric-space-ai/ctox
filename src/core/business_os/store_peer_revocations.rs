//! Current peer policy reads. Cache a bounded read-only connection, never a
//! decision or transaction; missing/corrupt/replaced stores must not grant access.
use anyhow::Context;
use rusqlite::{Connection, OpenFlags, OptionalExtension};
use std::path::Path;

fn open_reader(path: &Path) -> anyhow::Result<Connection> {
    let conn = Connection::open_with_flags(
        path,
        OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )
    .context("open peer revocation store for reading")?;
    conn.busy_timeout(crate::persistence::sqlite_busy_timeout_duration())?;
    Ok(conn)
}

fn query(conn: &Connection, peer_id: &str) -> anyhow::Result<bool> {
    Ok(conn
        .prepare_cached("SELECT 1 FROM business_peer_revocations WHERE peer_id = ?1")?
        .query_row([peer_id], |_| Ok(()))
        .optional()?
        .is_some())
}

#[cfg(unix)]
type FileIdentity = (std::path::PathBuf, u64, u64);

#[cfg(unix)]
fn identity(path: &Path) -> anyhow::Result<FileIdentity> {
    use std::os::unix::fs::MetadataExt;
    let canonical = std::fs::canonicalize(path).context("locate peer revocation store")?;
    let metadata = std::fs::metadata(&canonical).context("inspect peer revocation store")?;
    anyhow::ensure!(metadata.is_file(), "peer revocation store is not a file");
    Ok((canonical, metadata.dev(), metadata.ino()))
}

#[cfg(unix)]
thread_local! {
    // One slot per calling thread; switching instances replaces the handle.
    static READER: std::cell::RefCell<Option<(FileIdentity, Connection)>> = const {
        std::cell::RefCell::new(None)
    };
}

/// Returns an error when policy cannot be read. Callers may admit a peer only
/// on Ok(false). This read never initializes an absent database or schema.
pub fn is_business_peer_revoked(root: &Path, peer_id: &str) -> anyhow::Result<bool> {
    let peer_id = peer_id.trim();
    anyhow::ensure!(!peer_id.is_empty(), "peer_id is required");
    let path = super::business_os_store_path(root);
    #[cfg(unix)]
    {
        READER.with(|slot| {
            let mut cached = slot.borrow_mut();
            let result = (|| {
                let current = identity(&path)?;
                if cached.as_ref().is_none_or(|(key, _)| key != &current) {
                    *cached = None;
                    let conn = open_reader(&current.0)?;
                    anyhow::ensure!(
                        identity(&path)? == current,
                        "peer revocation store changed while opening"
                    );
                    *cached = Some((current, conn));
                }
                let (key, conn) = cached.as_ref().expect("reader initialized above");
                let revoked = query(conn, peer_id)?;
                anyhow::ensure!(
                    identity(&path)? == *key,
                    "peer revocation store changed while reading"
                );
                Ok(revoked)
            })();
            if result.is_err() {
                *cached = None;
            }
            result
        })
    }
    #[cfg(not(unix))]
    {
        // No cached Windows handle until file-replacement identity is certified.
        query(&open_reader(&path)?, peer_id)
    }
}

#[cfg(test)]
#[path = "store_peer_revocations_tests.rs"]
mod tests;
