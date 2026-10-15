//! Content-addressed files in the existing Transfer-CAS layout. No database or
//! transfer identifiers are exposed to apps. The daemon owns this private root.
use crate::{writer::WriterHandle, JobOptions, StoreError, WriteJob};
use rusqlite::Transaction;
use sha2::{Digest, Sha256};
use std::{
    fs::{self, File, OpenOptions},
    io::{Read, Seek, SeekFrom, Write},
    path::{Path, PathBuf},
};

// Logical chunks amortize durable IO; physical frames/credit remain owned by
// the scheduler (16 KiB / 128 KiB). Never allocate a chunk on the async stack.
pub const CHUNK_BYTES: usize = 512 * 1024;
const IO_BUFFER_BYTES: usize = 64 * 1024;
fn invalid(message: &str) -> StoreError {
    StoreError::Invalid(message.into())
}
fn digest_ok(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}
fn regular(path: &Path) -> Result<(), StoreError> {
    let m = fs::symlink_metadata(path)?;
    if !m.is_file() || m.file_type().is_symlink() {
        return Err(invalid("CAS requires regular file"));
    }
    Ok(())
}
fn directory(path: &Path) -> Result<(), StoreError> {
    match fs::symlink_metadata(path) {
        Ok(m) if !m.is_dir() || m.file_type().is_symlink() => {
            return Err(invalid("CAS requires private directory"))
        }
        Ok(_) => (),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => fs::create_dir_all(path)?,
        Err(e) => return Err(e.into()),
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(path, fs::Permissions::from_mode(0o700))?;
    }
    Ok(())
}
fn verify(path: &Path, reference: &str, size: u64) -> Result<(), StoreError> {
    regular(path)?;
    let mut f = File::open(path)?;
    if f.metadata()?.len() != size {
        return Err(invalid("CAS size mismatch"));
    }
    let mut hash = Sha256::new();
    let mut buf = [0; IO_BUFFER_BYTES];
    loop {
        let n = f.read(&mut buf)?;
        if n == 0 {
            break;
        }
        hash.update(&buf[..n]);
    }
    if format!("{:x}", hash.finalize()) != reference {
        return Err(invalid("CAS sha256 mismatch"));
    }
    Ok(())
}

/// App-facing reference. Never contains a transfer/session identifier.
#[derive(Clone, Debug, Eq, PartialEq, serde::Serialize)]
pub struct BlobRef {
    pub r#ref: String,
}
#[derive(Clone, Debug)]
pub struct FileRecord {
    pub blob: BlobRef,
    pub name: String,
    pub mime: String,
    pub size: u64,
    pub owner_app: String,
}
pub struct FileMetadata {
    pub name: String,
    pub mime: String,
    pub owner_app: String,
}
/// Constructible only after hashing complete bytes, before publishing metadata.
#[derive(Clone, Debug)]
pub struct VerifiedBlob {
    reference: String,
    size: u64,
}
impl VerifiedBlob {
    pub fn reference(&self) -> &str {
        &self.reference
    }
    pub fn size(&self) -> u64 {
        self.size
    }
    /// Both primitive effects and its files record run in ONE canonical writer
    /// transaction. The schema owner supplies its normal files insert, not a new
    /// private SQL table. Failing either closure rolls back both effects.
    pub async fn commit_with_file<T: Send + 'static>(
        &self,
        writer: &WriterHandle,
        name: String,
        mime: String,
        owner_app: String,
        insert_file: impl FnOnce(&Transaction<'_>, &FileRecord) -> Result<(), StoreError>
            + Send
            + 'static,
        effect: impl FnOnce(&Transaction<'_>, &BlobRef) -> Result<T, StoreError> + Send + 'static,
    ) -> Result<T, StoreError> {
        let record = FileRecord {
            blob: BlobRef {
                r#ref: self.reference.clone(),
            },
            name,
            mime,
            size: self.size,
            owner_app,
        };
        if record.owner_app.is_empty() {
            return Err(invalid("CAS publication requires owner_app"));
        }
        writer
            .submit(WriteJob::new(JobOptions::default(), move |tx| {
                insert_file(tx, &record)?;
                effect(tx, &record.blob)
            }))?
            .await
    }
}
#[derive(Clone)]
pub struct Cas {
    root: PathBuf,
}
impl Cas {
    /// Primitive entry point: return the app's `{ref}` only after the files row
    /// and primitive's database effect have committed together. Callers supply
    /// their platform schema inserter; it and the effect must enforce the current
    /// actor/epoch policy inside the committing transaction.
    pub async fn produce_file<T: Send + 'static>(
        &self,
        source: PathBuf,
        scope: String,
        writer: &WriterHandle,
        metadata: FileMetadata,
        insert_file: impl FnOnce(&Transaction<'_>, &FileRecord) -> Result<(), StoreError>
            + Send
            + 'static,
        effect: impl FnOnce(&Transaction<'_>, &BlobRef) -> Result<T, StoreError> + Send + 'static,
    ) -> Result<(BlobRef, T), StoreError> {
        let blob = self.import(source, scope).await?;
        let reference = BlobRef {
            r#ref: blob.reference.clone(),
        };
        let result = blob
            .commit_with_file(
                writer,
                metadata.name,
                metadata.mime,
                metadata.owner_app,
                insert_file,
                effect,
            )
            .await?;
        Ok((reference, result))
    }
    /// Import a primitive's output before its single publication transaction.
    /// Full hashing and bounded copies run off the canonical writer. On a
    /// publication failure bytes may remain unreferenced for normal CAS GC.
    pub async fn import(&self, source: PathBuf, scope: String) -> Result<VerifiedBlob, StoreError> {
        let cas = self.clone();
        tokio::task::spawn_blocking(move || {
            regular(&source)?;
            let mut file = File::open(&source)?;
            let size = file.metadata()?.len();
            let mut hash = Sha256::new();
            let mut buffer = [0; IO_BUFFER_BYTES];
            loop {
                let n = file.read(&mut buffer)?;
                if n == 0 {
                    break;
                }
                hash.update(&buffer[..n]);
            }
            let reference = format!("{:x}", hash.finalize());
            if let Ok(blob) = cas.verified(&reference, size) {
                return Ok(blob);
            }
            let mut receiver = cas.receive(&scope, &reference, size)?;
            let mut offset = receiver.offset()?;
            file.seek(SeekFrom::Start(offset))?;
            let mut buffer = vec![0; CHUNK_BYTES];
            loop {
                let n = file.read(&mut buffer)?;
                if n == 0 {
                    break;
                }
                receiver.append(offset, &buffer[..n])?;
                offset += n as u64;
            }
            receiver.finish()
        })
        .await
        .map_err(|e| StoreError::Io(e.to_string()))?
    }
    /// Use the daemon's existing Transfer-CAS artifact root, not a second store.
    pub fn open(root: impl AsRef<Path>) -> Result<Self, StoreError> {
        directory(root.as_ref())?;
        let root = root.as_ref().canonicalize()?;
        directory(&root.join("objects"))?;
        directory(&root.join("staging"))?;
        Ok(Self { root })
    }
    pub fn verified(&self, reference: &str, size: u64) -> Result<VerifiedBlob, StoreError> {
        if !digest_ok(reference) {
            return Err(invalid("invalid sha256 ref"));
        }
        verify(&self.root.join("objects").join(reference), reference, size)?;
        Ok(VerifiedBlob {
            reference: reference.into(),
            size,
        })
    }
    pub fn reader(&self, blob: &VerifiedBlob) -> Result<File, StoreError> {
        let path = self.root.join("objects").join(&blob.reference);
        verify(&path, &blob.reference, blob.size)?;
        Ok(File::open(path)?)
    }
    /// Scope is an internal digest of authenticated instance/device/epoch and
    /// transfer identity. Never accept an app-selected staging path. One owner
    /// must serialize each scope/ref; rebind only after the old owner drains.
    pub fn receive(&self, scope: &str, reference: &str, size: u64) -> Result<Receiver, StoreError> {
        if !digest_ok(scope) || !digest_ok(reference) {
            return Err(invalid("invalid CAS scope/ref"));
        }
        let path = self
            .root
            .join("staging")
            .join(format!("v4-{scope}-{reference}"));
        let file = match OpenOptions::new()
            .read(true)
            .write(true)
            .create_new(true)
            .open(&path)
        {
            Ok(file) => {
                #[cfg(unix)]
                {
                    use std::os::unix::fs::PermissionsExt;
                    file.set_permissions(fs::Permissions::from_mode(0o600))?;
                }
                file
            }
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {
                regular(&path)?;
                OpenOptions::new().read(true).write(true).open(&path)?
            }
            Err(e) => return Err(e.into()),
        };
        file.try_lock()
            .map_err(|e| StoreError::Io(format!("CAS receiver owned: {e}")))?;
        file.sync_all()?;
        File::open(self.root.join("staging"))?.sync_all()?;
        if file.metadata()?.len() > size {
            return Err(invalid("CAS resume beyond size"));
        }
        Ok(Receiver {
            cas: self.clone(),
            path,
            file,
            reference: reference.into(),
            size,
        })
    }
}
pub struct Receiver {
    cas: Cas,
    path: PathBuf,
    file: File,
    reference: String,
    size: u64,
}
impl Receiver {
    pub fn offset(&self) -> Result<u64, StoreError> {
        Ok(self.file.metadata()?.len())
    }
    /// Persist before session ACK. A replayed chunk is accepted ONLY when its
    /// bytes match the existing prefix; gaps/overlaps cannot advance the offset.
    pub fn append(&mut self, offset: u64, bytes: &[u8]) -> Result<(), StoreError> {
        if bytes.len() > CHUNK_BYTES
            || bytes.is_empty()
            || offset
                .checked_add(bytes.len() as u64)
                .is_none_or(|end| end > self.size)
        {
            return Err(invalid("CAS chunk bounds"));
        }
        let current = self.offset()?;
        if offset > current {
            return Err(invalid("CAS resume gap"));
        }
        if offset < current {
            if offset + bytes.len() as u64 > current {
                return Err(invalid("CAS partial overlap"));
            }
            self.file.seek(SeekFrom::Start(offset))?;
            let mut existing = vec![0; bytes.len()];
            self.file.read_exact(&mut existing)?;
            if existing != bytes {
                return Err(invalid("CAS replay content mismatch"));
            }
            return Ok(());
        }
        self.file.seek(SeekFrom::Start(offset))?;
        self.file.write_all(bytes)?;
        self.file.sync_data()?;
        Ok(())
    }
    pub fn finish(self) -> Result<VerifiedBlob, StoreError> {
        verify(&self.path, &self.reference, self.size)?;
        let target = self.cas.root.join("objects").join(&self.reference);
        match fs::hard_link(&self.path, &target) {
            Ok(()) => (),
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {
                verify(&target, &self.reference, self.size)?
            }
            Err(e) => return Err(e.into()),
        }
        File::open(self.cas.root.join("objects"))?.sync_all()?;
        fs::remove_file(&self.path)?;
        File::open(self.cas.root.join("staging"))?.sync_all()?;
        Ok(VerifiedBlob {
            reference: self.reference,
            size: self.size,
        })
    }
}

#[cfg(test)]
mod tests;
