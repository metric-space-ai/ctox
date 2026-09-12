//! Content-addressed session checkpoints. No source tree, credentials or live database is mutated.
use crate::contracts::{ArtifactRef, CheckpointManifest, WorkspaceEntry, WorkspaceEntryKind};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeSet,
    fs::{self, File, OpenOptions},
    io::{self, Read, Write},
    path::{Path, PathBuf},
};

const MAX_MANIFEST_BYTES: u64 = 8 * 1024 * 1024;
const MAX_SAFE_INTEGER: u64 = 9_007_199_254_740_991;
fn invalid(message: impl Into<String>) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, message.into())
}
fn hash_valid(hash: &str) -> bool {
    hash.len() == 64
        && hash
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}

pub struct CheckpointStore {
    root: PathBuf,
    max_blob_bytes: u64,
}
impl CheckpointStore {
    pub fn open(root: PathBuf, max_blob_bytes: u64) -> io::Result<Self> {
        if max_blob_bytes == 0 || max_blob_bytes > MAX_SAFE_INTEGER {
            return Err(invalid("invalid checkpoint blob budget"));
        }
        fs::create_dir_all(root.join("blobs"))?;
        fs::create_dir_all(root.join("manifests"))?;
        let root = fs::canonicalize(root)?;
        Ok(Self {
            root,
            max_blob_bytes,
        })
    }
    pub(crate) fn max_blob_bytes(&self) -> u64 {
        self.max_blob_bytes
    }
    fn blob_path(&self, artifact: &ArtifactRef) -> io::Result<PathBuf> {
        if !hash_valid(&artifact.sha256) || artifact.size_bytes > self.max_blob_bytes {
            return Err(invalid("invalid checkpoint artifact identity or size"));
        }
        Ok(self.root.join("blobs").join(&artifact.sha256))
    }
    /// A receiver acknowledges a copy only after streaming verification and durable publication.
    pub fn ingest_blob(&self, expected: &ArtifactRef, mut input: impl Read) -> io::Result<()> {
        let target = self.blob_path(expected)?;
        let mut staged = tempfile::NamedTempFile::new_in(self.root.join("blobs"))?;
        let mut hash = Sha256::new();
        let mut size = 0u64;
        let mut buffer = [0u8; 65536];
        loop {
            let n = input.read(&mut buffer)?;
            if n == 0 {
                break;
            }
            size = size
                .checked_add(n as u64)
                .ok_or_else(|| invalid("artifact size overflow"))?;
            if size > expected.size_bytes {
                return Err(invalid("artifact exceeds declared length"));
            }
            hash.update(&buffer[..n]);
            staged.write_all(&buffer[..n])?;
        }
        if size != expected.size_bytes || format!("{:x}", hash.finalize()) != expected.sha256 {
            return Err(invalid("artifact hash or length mismatch"));
        }
        staged.as_file().sync_all()?;
        match staged.persist_noclobber(&target) {
            Ok(_) => sync_directory(&self.root.join("blobs"))?,
            Err(e) if e.error.kind() == io::ErrorKind::AlreadyExists => {
                self.verify_blob(expected)?
            }
            Err(e) => return Err(e.error),
        }
        Ok(())
    }
    pub fn verify_blob(&self, expected: &ArtifactRef) -> io::Result<()> {
        let path = self.blob_path(expected)?;
        if fs::symlink_metadata(&path)?.file_type().is_symlink() {
            return Err(invalid("artifact store contains a symlink"));
        }
        let mut input = File::open(path)?;
        let mut hash = Sha256::new();
        let mut size = 0u64;
        let mut buffer = [0u8; 65536];
        loop {
            let n = input.read(&mut buffer)?;
            if n == 0 {
                break;
            }
            size += n as u64;
            if size > expected.size_bytes {
                return Err(invalid("stored artifact exceeds declared length"));
            }
            hash.update(&buffer[..n]);
        }
        if size != expected.size_bytes || format!("{:x}", hash.finalize()) != expected.sha256 {
            return Err(invalid("stored artifact hash or length mismatch"));
        }
        Ok(())
    }
    pub fn open_blob(&self, expected: &ArtifactRef) -> io::Result<File> {
        self.verify_blob(expected)?;
        File::open(self.blob_path(expected)?)
    }
    fn verify_contents(&self, manifest: &CheckpointManifest) -> io::Result<()> {
        for artifact in artifacts(manifest) {
            self.verify_blob(artifact)?;
        }
        for entry in manifest
            .workspace
            .iter()
            .chain(&manifest.workspace_state.required_untracked)
            .chain(&manifest.provider_state)
            .filter(|e| e.kind == WorkspaceEntryKind::Symlink)
        {
            if entry.artifact.size_bytes > 4096 {
                return Err(invalid("oversized session symlink"));
            }
            let mut target = String::new();
            self.open_blob(&entry.artifact)?
                .read_to_string(&mut target)?;
            validate_link(&entry.path, &target)?;
        }
        Ok(())
    }
    pub fn publish(&self, manifest: &CheckpointManifest) -> io::Result<String> {
        validate_manifest(manifest)?;
        self.verify_contents(manifest)?;
        let bytes = serde_json::to_vec(manifest).map_err(io::Error::other)?;
        if bytes.len() as u64 > MAX_MANIFEST_BYTES {
            return Err(invalid("checkpoint manifest exceeds its budget"));
        }
        let digest = format!("{:x}", Sha256::digest(&bytes));
        let mut staged = tempfile::NamedTempFile::new_in(self.root.join("manifests"))?;
        staged.write_all(&bytes)?;
        staged.as_file().sync_all()?;
        match staged.persist_noclobber(self.root.join("manifests").join(&digest)) {
            Ok(_) => sync_directory(&self.root.join("manifests"))?,
            Err(e) if e.error.kind() == io::ErrorKind::AlreadyExists => {
                self.load(&digest)?;
            }
            Err(e) => return Err(e.error),
        }
        Ok(digest)
    }
    /// Reverify content before issuing a durable-copy receipt or restoring a session.
    pub fn load(&self, digest: &str) -> io::Result<CheckpointManifest> {
        if !hash_valid(digest) {
            return Err(invalid("invalid checkpoint digest"));
        }
        let path = self.root.join("manifests").join(digest);
        let meta = fs::symlink_metadata(&path)?;
        if !meta.is_file() || meta.len() > MAX_MANIFEST_BYTES {
            return Err(invalid("invalid stored checkpoint manifest"));
        }
        let bytes = fs::read(path)?;
        if format!("{:x}", Sha256::digest(&bytes)) != digest {
            return Err(invalid("checkpoint manifest hash mismatch"));
        }
        let manifest: CheckpointManifest =
            serde_json::from_slice(&bytes).map_err(io::Error::other)?;
        validate_manifest(&manifest)?;
        self.verify_contents(&manifest)?;
        Ok(manifest)
    }
    /// Flush verified artifacts and their directory entries before issuing a signed receipt.
    pub(crate) fn verify_durable_copy(&self, digest: &str) -> io::Result<CheckpointManifest> {
        if !cfg!(unix) {
            return Err(io::Error::new(
                io::ErrorKind::Unsupported,
                "durable checkpoint receipts require a certified directory-flush implementation",
            ));
        }
        let manifest = self.load(digest)?;
        for artifact in artifacts(&manifest) {
            File::open(self.blob_path(artifact)?)?.sync_all()?;
        }
        File::open(self.root.join("manifests").join(digest))?.sync_all()?;
        sync_directory(&self.root.join("blobs"))?;
        sync_directory(&self.root.join("manifests"))?;
        // Include newly created ancestor directories, not just the blob entries.
        for directory in self.root.ancestors() {
            sync_directory(directory)?;
        }
        Ok(manifest)
    }
    /// Restore into a newly created directory only. Existing user files are never overwritten.
    pub fn restore(&self, digest: &str, target: &Path) -> io::Result<CheckpointManifest> {
        let manifest = self.load(digest)?;
        if !manifest.pending_effects.is_empty() {
            return Err(invalid("session requires external-effect reconciliation"));
        }
        fs::create_dir(target)?;
        let result = (|| {
            let mut workspace_entries = manifest.workspace.clone();
            workspace_entries.extend(manifest.workspace_state.required_untracked.iter().cloned());
            self.restore_entries(&workspace_entries, &target.join("workspace"))?;
            self.restore_entries(&manifest.provider_state, &target.join("provider"))?;
            self.restore_artifacts(&manifest.history, &target.join("history"))?;
            self.restore_artifacts(&manifest.attachments, &target.join("attachments"))?;
            let git_root = target.join("git");
            fs::create_dir(&git_root)?;
            self.restore_named_artifact(
                &manifest.workspace_state.index_patch,
                &git_root.join("index.patch"),
            )?;
            self.restore_named_artifact(
                &manifest.workspace_state.worktree_patch,
                &git_root.join("worktree.patch"),
            )?;
            let mut base = OpenOptions::new();
            base.write(true).create_new(true);
            #[cfg(unix)]
            {
                use std::os::unix::fs::OpenOptionsExt;
                base.mode(0o600);
            }
            let mut base_file = base.open(git_root.join("base-commit"))?;
            base_file.write_all(manifest.workspace_state.base_commit.as_bytes())?;
            base_file.write_all(b"\n")?;
            base_file.sync_all()?;
            sync_directory(&git_root)?;
            let manifest_file = target.join("checkpoint.json");
            let mut f = OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(manifest_file)?;
            f.write_all(&serde_json::to_vec(&manifest).map_err(io::Error::other)?)?;
            f.sync_all()?;
            sync_directory(target)?;
            Ok(())
        })();
        if let Err(error) = result {
            // This directory was created by this call. It has never been exposed as ready.
            let _ = fs::remove_dir_all(target);
            return Err(error);
        }
        Ok(manifest)
    }
    fn restore_named_artifact(&self, artifact: &ArtifactRef, path: &Path) -> io::Result<()> {
        let mut options = OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        let mut output = options.open(path)?;
        io::copy(&mut self.open_blob(artifact)?, &mut output)?;
        output.sync_all()
    }
    fn restore_artifacts(&self, artifacts: &[ArtifactRef], root: &Path) -> io::Result<()> {
        fs::create_dir(root)?;
        let mut copied = BTreeSet::new();
        for artifact in artifacts {
            if !copied.insert(&artifact.sha256) {
                continue;
            }
            let mut options = OpenOptions::new();
            options.write(true).create_new(true);
            #[cfg(unix)]
            {
                use std::os::unix::fs::OpenOptionsExt;
                options.mode(0o600);
            }
            let mut output = options.open(root.join(&artifact.sha256))?;
            io::copy(&mut self.open_blob(artifact)?, &mut output)?;
            output.sync_all()?;
        }
        sync_directory(root)
    }
    fn restore_entries(&self, entries: &[WorkspaceEntry], root: &Path) -> io::Result<()> {
        fs::create_dir(root)?;
        // Install links last, so writing a regular file never traverses a checkpoint symlink.
        for entry in entries
            .iter()
            .filter(|e| e.kind == WorkspaceEntryKind::File)
        {
            let path = root.join(&entry.path);
            if let Some(parent) = path.parent() {
                fs::create_dir_all(parent)?;
            }
            let mut options = OpenOptions::new();
            options.write(true).create_new(true);
            #[cfg(unix)]
            {
                use std::os::unix::fs::OpenOptionsExt;
                options.mode(if entry.executable { 0o700 } else { 0o600 });
            }
            let mut output = options.open(path)?;
            io::copy(&mut self.open_blob(&entry.artifact)?, &mut output)?;
            output.sync_all()?;
        }
        for entry in entries
            .iter()
            .filter(|e| e.kind == WorkspaceEntryKind::Symlink)
        {
            let path = root.join(&entry.path);
            if let Some(parent) = path.parent() {
                fs::create_dir_all(parent)?;
            }
            let mut text = String::new();
            self.open_blob(&entry.artifact)?
                .take(4097)
                .read_to_string(&mut text)?;
            validate_link(&entry.path, &text)?;
            #[cfg(unix)]
            std::os::unix::fs::symlink(&text, &path)?;
            #[cfg(not(unix))]
            {
                let _ = path;
                return Err(invalid("this host has not certified symlink restoration"));
            }
        }
        sync_directory(root)
    }
}
fn sync_directory(path: &Path) -> io::Result<()> {
    #[cfg(unix)]
    {
        File::open(path)?.sync_all()?;
    }
    #[cfg(not(unix))]
    {
        let _ = path;
    }
    Ok(())
}
fn artifacts(manifest: &CheckpointManifest) -> impl Iterator<Item = &ArtifactRef> {
    manifest
        .history
        .iter()
        .chain(&manifest.attachments)
        .chain(std::iter::once(&manifest.workspace_state.index_patch))
        .chain(std::iter::once(&manifest.workspace_state.worktree_patch))
        .chain(manifest.workspace.iter().map(|e| &e.artifact))
        .chain(
            manifest
                .workspace_state
                .required_untracked
                .iter()
                .map(|e| &e.artifact),
        )
        .chain(manifest.provider_state.iter().map(|e| &e.artifact))
}
pub fn validate_manifest(manifest: &CheckpointManifest) -> io::Result<()> {
    let session = &manifest.session;
    if manifest.version != 2
        || session.version != 1
        || manifest.sequence > MAX_SAFE_INTEGER
        || session.scope_id.is_empty()
        || session.session_id.is_empty()
        || session.harness.is_empty()
        || session.harness_version.is_empty()
        || session.gateway_account_id.is_empty()
        || session.model_route_id.is_empty()
        || session.model_id.is_empty()
        || manifest.history.is_empty()
        || manifest.provider_state.is_empty()
    {
        return Err(invalid(
            "incomplete or incompatible portable session manifest",
        ));
    }
    if !commit_valid(&manifest.workspace_state.base_commit) {
        return Err(invalid("portable session requires a full Git base commit"));
    }
    let mut workspace_entries = manifest.workspace.clone();
    workspace_entries.extend(manifest.workspace_state.required_untracked.iter().cloned());
    let workspace_names = validate_entries(&workspace_entries)?;
    validate_entries(&manifest.provider_state)?;
    let mut deleted_names = BTreeSet::new();
    for path in &manifest.workspace_state.deleted_paths {
        validate_path(path)?;
        let name = path.to_lowercase();
        if !deleted_names.insert(name.clone())
            || deleted_names.iter().any(|other| {
                other != &name
                    && (name.starts_with(&format!("{other}/"))
                        || other.starts_with(&format!("{name}/")))
            })
            || workspace_names.iter().any(|entry| {
                entry == &name
                    || entry.starts_with(&format!("{name}/"))
                    || name.starts_with(&format!("{entry}/"))
            })
        {
            return Err(invalid("deleted path is also present in the workspace"));
        }
    }
    for artifact in artifacts(manifest) {
        if !hash_valid(&artifact.sha256) || artifact.size_bytes > MAX_SAFE_INTEGER {
            return Err(invalid("invalid artifact reference"));
        }
    }
    Ok(())
}
fn validate_entries(entries: &[WorkspaceEntry]) -> io::Result<BTreeSet<String>> {
    let mut names = BTreeSet::new();
    for entry in entries {
        validate_path(&entry.path)?;
        let name = entry.path.to_lowercase();
        if !names.insert(name) {
            return Err(invalid("duplicate or case-colliding session path"));
        }
    }
    for name in &names {
        let mut prefix = String::new();
        for component in name
            .split('/')
            .take(name.split('/').count().saturating_sub(1))
        {
            if !prefix.is_empty() {
                prefix.push('/');
            }
            prefix.push_str(component);
            if names.contains(&prefix) {
                return Err(invalid(
                    "session file or symlink is also used as a directory",
                ));
            }
        }
    }
    Ok(names)
}
fn commit_valid(commit: &str) -> bool {
    (commit.len() == 40 || commit.len() == 64)
        && commit
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}
pub(crate) fn validate_path(path: &str) -> io::Result<()> {
    if path.is_empty()
        || path.len() > 4096
        || path.starts_with('/')
        || path.contains(['\\', ':', '\0', '*', '?', '"', '<', '>', '|'])
    {
        return Err(invalid("unsafe portable session path"));
    }
    for part in path.split('/') {
        if part.is_empty() || part == "." || part == ".." || part.ends_with([' ', '.']) {
            return Err(invalid("unsafe portable session path component"));
        }
        let stem = part
            .split('.')
            .next()
            .unwrap_or_default()
            .to_ascii_uppercase();
        if ["CON", "PRN", "AUX", "NUL"].contains(&stem.as_str())
            || (stem.len() == 4
                && (stem.starts_with("COM") || stem.starts_with("LPT"))
                && stem.as_bytes()[3].is_ascii_digit())
        {
            return Err(invalid("reserved portable session path"));
        }
    }
    Ok(())
}
pub(crate) fn validate_link(path: &str, target: &str) -> io::Result<()> {
    if target.is_empty()
        || target.len() > 4096
        || target.starts_with('/')
        || target.contains(['\\', ':', '\0'])
    {
        return Err(invalid("unsafe session symlink"));
    }
    let mut depth = path.split('/').count() - 1;
    for part in target.split('/') {
        match part {
            "" | "." => {}
            ".." => {
                depth = depth
                    .checked_sub(1)
                    .ok_or_else(|| invalid("session symlink escapes workspace"))?;
            }
            _ => depth += 1,
        }
    }
    Ok(())
}
