//! Immutable release loading and production Node delivery. No PATH fallback.
use crate::{AppBundle, Result};
#[cfg(not(feature = "bundled-node"))]
use anyhow::bail;
use anyhow::{ensure, Context};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet},
    path::{Path, PathBuf},
};
pub trait ReleaseArtifacts {
    /// Only admitted immutable business_app_versions, not drafts or app input.
    fn version(&self, app: &str, version: &str) -> Result<Value>;
    fn bytes(&self, hash: &str) -> Result<Vec<u8>>;
}
#[cfg(feature = "core-store")]
impl ReleaseArtifacts for crate::core_host::CoreRuntimeHost {
    fn version(&self, app: &str, version: &str) -> Result<Value> {
        self.release(app, version)
    }
    fn bytes(&self, hash: &str) -> Result<Vec<u8>> {
        self.artifact(hash)
    }
}
pub fn load_release(
    source: &dyn ReleaseArtifacts,
    app: &str,
    version: &str,
    grants: BTreeSet<String>,
) -> Result<AppBundle> {
    let record = source.version(app, version)?;
    let manifest = record
        .get("manifest")
        .filter(|m| m.is_object())
        .unwrap_or(&record)
        .clone();
    ensure!(
        manifest["app_id"] == app && manifest["app_ver"] == version,
        "conflict: published app version identity differs"
    );
    let mut needed = BTreeSet::new();
    let files = manifest["files"]
        .as_object()
        .context("invalid: published files")?;
    // Include helper modules too; paths are mapped to verified CAS bytes.
    for path in files.keys().filter(|p| p.starts_with("handlers/")) {
        needed.insert(path.clone());
    }
    for target in manifest["handlers"]
        .as_object()
        .into_iter()
        .flat_map(|m| m.values())
        .chain(
            manifest["triggers"]
                .as_array()
                .into_iter()
                .flatten()
                .map(|t| &t["handler"]),
        )
    {
        let file = crate::reference_file(target.as_str().context("invalid: handler reference")?)?;
        ensure!(
            file.starts_with("handlers/"),
            "invalid: production handlers must be in handlers/"
        );
        needed.insert(file.to_owned());
    }
    for spec in manifest["migrations"]
        .as_object()
        .into_iter()
        .flat_map(|m| m.values())
    {
        if let Some(reference) = spec["migrate_fn"].as_str() {
            let file = crate::reference_file(reference)?;
            needed.insert(file.into());
            if let Some((directory, _)) = file.rsplit_once('/') {
                needed.extend(
                    files
                        .keys()
                        .filter(|p| p.starts_with(&format!("{directory}/")))
                        .cloned(),
                );
            }
        }
    }
    let mut sources = BTreeMap::new();
    let mut size = 0usize;
    for path in needed {
        let hash = files
            .get(&path)
            .and_then(Value::as_str)
            .context("invalid: module hash missing")?;
        ensure!(
            hash.len() == 64 && hash.bytes().all(|c| c.is_ascii_hexdigit()),
            "invalid: CAS hash"
        );
        let bytes = source.bytes(hash)?;
        size = size
            .checked_add(bytes.len())
            .context("quota: source size")?;
        ensure!(
            size <= 4 * 1024 * 1024,
            "quota: handler source exceeds 4 MiB"
        );
        ensure!(
            format!("{:x}", Sha256::digest(&bytes)) == hash,
            "conflict: CAS integrity mismatch"
        );
        sources.insert(
            path,
            String::from_utf8(bytes).context("invalid: handler is not UTF-8")?,
        );
    }
    AppBundle::new(manifest, sources, grants)
}

#[cfg(feature = "bundled-node")]
const NODE: &[u8] = include_bytes!(env!("CTOX_APP_RUNTIME_NODE_BUNDLE"));
/// Build-time packaging input, never a runtime environment override or download.
/// The package owner supplies the platform's official Node binary at compilation.
pub fn bundled_node(root: &Path) -> Result<PathBuf> {
    #[cfg(not(feature = "bundled-node"))]
    {
        let _ = root;
        bail!("unavailable: app runtime package has no embedded Node");
    }
    #[cfg(feature = "bundled-node")]
    {
        use std::{fs, io::Write};
        let hash = format!("{:x}", Sha256::digest(NODE));
        let root = root.join("app-node").join(&hash);
        fs::create_dir_all(&root)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(&root, fs::Permissions::from_mode(0o700))?;
        }
        let path = root.join(if cfg!(windows) { "node.exe" } else { "node" });
        if let Ok(metadata) = fs::symlink_metadata(&path) {
            ensure!(
                metadata.file_type().is_file(),
                "forbidden: embedded Node destination is not a regular file"
            );
            let current = fs::read(&path)?;
            ensure!(
                Sha256::digest(&current) == Sha256::digest(NODE),
                "conflict: embedded Node binary changed"
            );
            return Ok(path);
        }
        let temporary = root.join(format!(".node-{}", uuid::Uuid::new_v4()));
        let result = (|| {
            let mut file = fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&temporary)?;
            file.write_all(NODE)?;
            file.sync_all()?;
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                file.set_permissions(fs::Permissions::from_mode(0o700))?;
            }
            // Hard-link publishes without replacing another installer's file.
            match fs::hard_link(&temporary, &path) {
                Ok(()) => {}
                Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
                    ensure!(
                        fs::symlink_metadata(&path)?.file_type().is_file()
                            && Sha256::digest(fs::read(&path)?) == Sha256::digest(NODE),
                        "conflict: concurrent Node delivery mismatch"
                    );
                }
                Err(error) => return Err(error.into()),
            }
            Ok(path)
        })();
        let _ = fs::remove_file(temporary);
        result
    }
}
