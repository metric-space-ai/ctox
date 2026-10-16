//! Pure shell modules, pinned once per invocation to a verified shell manifest.
use anyhow::{ensure, Context, Result};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeMap;

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct SharedModules {
    pub version: String,
    pub sources: BTreeMap<String, String>,
}
impl SharedModules {
    pub fn load(manifest: &Value, mut bytes: impl FnMut(&str) -> Result<Vec<u8>>) -> Result<Self> {
        let version = crate::string(manifest, "app_ver")?.to_owned();
        let exports = manifest["shared_exports"]
            .as_array()
            .context("invalid: shell shared_exports")?;
        let mut sources = BTreeMap::new();
        let mut size = 0usize;
        for export in exports {
            let path = export.as_str().context("invalid: shared export path")?;
            ensure!(
                crate::safe_path(path) && path.starts_with("shared/"),
                "forbidden: shell shared export path"
            );
            let hash = manifest["files"][path]
                .as_str()
                .context("invalid: shared export hash")?;
            let body = bytes(hash)?;
            size = size
                .checked_add(body.len())
                .context("quota: shared source size")?;
            ensure!(
                size <= 1024 * 1024,
                "quota: shared shell source exceeds 1 MiB"
            );
            use sha2::Digest;
            ensure!(
                format!("{:x}", sha2::Sha256::digest(&body)) == hash,
                "conflict: shell shared CAS integrity"
            );
            sources.insert(
                path.to_owned(),
                String::from_utf8(body).context("invalid: shared UTF-8")?,
            );
        }
        Ok(Self { version, sources })
    }
}
