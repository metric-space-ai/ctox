// Origin: CTOX
// License: AGPL-3.0-only

//! Read physical SQLite bytes through its retained VFS file, never a second OS
//! descriptor. Closing an independently opened descriptor on Unix would cancel
//! every POSIX lock this process holds on that inode, including other connections.
//! https://www.sqlite.org/howtocorrupt.html (section 2.2)

use anyhow::{ensure, Context, Result};
use rusqlite::{ffi, Connection, OpenFlags};
use sha2::{Digest, Sha256};
use std::path::Path;

/// Offline takeover uses the writer's stable lock inode, never the database
/// inode which restore replaces. Hold this across SQLite close and rename.
pub(super) struct MaintenanceOwner(std::fs::File);

impl MaintenanceOwner {
    pub(super) fn acquire(path: &Path) -> Result<Self> {
        let db = ctox_store::DbId::new(path)?;
        let mut name = db
            .path()
            .file_name()
            .context("database filename")?
            .to_os_string();
        name.push(".ctox-store.lock");
        let file = std::fs::OpenOptions::new()
            .create(true)
            .truncate(false)
            .read(true)
            .write(true)
            .open(db.path().with_file_name(name))?;
        file.try_lock().with_context(|| {
            format!(
                "offline maintenance requires stopped daemon and exclusive store ownership: {}",
                db.path().display()
            )
        })?;
        ensure!(
            ctox_store::DbId::new(path)? == db,
            "database identity changed during maintenance takeover"
        );
        Ok(Self(file))
    }
}

impl Drop for MaintenanceOwner {
    fn drop(&mut self) {
        let _ = self.0.unlock();
    }
}

struct MainFile<'a> {
    file: *mut ffi::sqlite3_file,
    mutex: *mut ffi::sqlite3_mutex,
    _connection: &'a Connection,
}
impl<'a> MainFile<'a> {
    fn new(connection: &'a Connection) -> Result<Self> {
        // SAFETY: the borrowed Connection remains alive and is not shared across
        // threads. SQLite's recursive connection mutex also covers VFS access;
        // enter/leave on a null mutex is a supported no-op for NOMUTEX connections.
        let mutex = unsafe { ffi::sqlite3_db_mutex(connection.handle()) };
        unsafe { ffi::sqlite3_mutex_enter(mutex) };
        let mut owner = Self {
            file: std::ptr::null_mut(),
            mutex,
            _connection: connection,
        };
        // SQLITE_FCNTL_FILE_POINTER lends the main file; it does not transfer
        // ownership. We never close, unlock, mutate or retain it past Connection.
        let code = unsafe {
            ffi::sqlite3_file_control(
                connection.handle(),
                b"main\0".as_ptr().cast(),
                ffi::SQLITE_FCNTL_FILE_POINTER,
                (&mut owner.file as *mut *mut ffi::sqlite3_file).cast(),
            )
        };
        ensure!(
            code == ffi::SQLITE_OK && !owner.file.is_null(),
            "SQLite main file is unavailable ({code})"
        );
        ensure!(
            !unsafe { (*owner.file).pMethods }.is_null(),
            "SQLite main VFS is unavailable"
        );
        Ok(owner)
    }
    fn len(&self) -> Result<i64> {
        // SAFETY: the checked file/method pointers belong to the still-borrowed
        // Connection, with its mutex held throughout this object's lifetime.
        let size = unsafe { (*(*self.file).pMethods).xFileSize }
            .context("SQLite VFS does not support file size")?;
        let mut bytes = 0;
        let code = unsafe { size(self.file, &mut bytes) };
        ensure!(
            code == ffi::SQLITE_OK && bytes >= 0,
            "SQLite file size failed ({code})"
        );
        Ok(bytes)
    }
    fn read(&self, offset: i64, bytes: &mut [u8]) -> Result<()> {
        let length = i32::try_from(bytes.len())?;
        let read = unsafe { (*(*self.file).pMethods).xRead }
            .context("SQLite VFS does not support file reads")?;
        // SAFETY: bytes is writable for length bytes; offset is nonnegative and
        // bounded by the previously checked file size at every call site.
        let code = unsafe { read(self.file, bytes.as_mut_ptr().cast(), length, offset) };
        ensure!(code == ffi::SQLITE_OK, "SQLite file read failed ({code})");
        Ok(())
    }
}
impl Drop for MainFile<'_> {
    fn drop(&mut self) {
        // SAFETY: paired with new's enter; the connection borrow still lives.
        unsafe { ffi::sqlite3_mutex_leave(self.mutex) };
    }
}

pub(super) fn sqlite_file_sha256(connection: &Connection) -> Result<(u64, String)> {
    let file = MainFile::new(connection)?;
    let size = file.len()?;
    let mut digest = Sha256::new();
    let mut buffer = [0u8; 64 * 1024];
    let mut offset = 0;
    while offset < size {
        let count = (size - offset).min(buffer.len() as i64) as usize;
        file.read(offset, &mut buffer[..count])?;
        digest.update(&buffer[..count]);
        offset += count as i64;
    }
    ensure!(
        file.len()? == size,
        "SQLite file changed size during digest"
    );
    Ok((size as u64, format!("{:x}", digest.finalize())))
}

/// Backups are caller-selected paths and can alias an already-open live inode.
/// Even the rejected-backup hash must use SQLite's inode/descriptor bookkeeping.
pub(super) fn sqlite_path_sha256(path: &Path) -> Result<(u64, String)> {
    let _owner = MaintenanceOwner::acquire(path)?;
    let connection = Connection::open_with_flags(
        path,
        OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )
    .with_context(|| format!("open SQLite physical digest {}", path.display()))?;
    sqlite_file_sha256(&connection)
}

#[cfg(test)]
mod maintenance_tests {
    use super::*;

    #[tokio::test]
    async fn live_owner_excludes_digest_and_maintenance_until_shutdown() -> Result<()> {
        let root = tempfile::tempdir()?;
        let path = root.path().join("owned.sqlite3");
        let db = ctox_store::DbId::new(&path)?;
        let writer = ctox_store::registry::Registry.open(db.clone(), Default::default())?;
        writer.ready().await?;
        assert!(MaintenanceOwner::acquire(&path).is_err());
        assert!(sqlite_path_sha256(&path).is_err());
        writer.shutdown().await?;
        let maintenance = MaintenanceOwner::acquire(&path)?;
        let competitor = ctox_store::registry::Registry.open(db.clone(), Default::default())?;
        assert!(competitor.ready().await.is_err());
        assert!(MaintenanceOwner::acquire(&path).is_err());
        drop(maintenance);
        assert!(sqlite_path_sha256(&path)?.0 > 0);
        let reopened = ctox_store::registry::Registry.open(db, Default::default())?;
        reopened.ready().await?;
        reopened.shutdown().await?;
        Ok(())
    }

    #[test]
    #[cfg(unix)]
    fn maintenance_canonicalizes_symlinks_and_refuses_hardlinks() -> Result<()> {
        let root = tempfile::tempdir()?;
        let path = root.path().join("store.sqlite3");
        drop(Connection::open(&path)?);
        let alias = root.path().join("alias.sqlite3");
        std::os::unix::fs::symlink(&path, &alias)?;
        let guard = MaintenanceOwner::acquire(&path)?;
        assert!(MaintenanceOwner::acquire(&alias).is_err());
        drop(guard);
        std::fs::hard_link(&path, root.path().join("hardlink.sqlite3"))?;
        assert!(MaintenanceOwner::acquire(&path).is_err());
        Ok(())
    }
}
