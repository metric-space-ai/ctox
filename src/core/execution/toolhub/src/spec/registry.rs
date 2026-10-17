//! Per-instance spec store: immutable versions, an active pointer, history,
//! rollback. Plain files under the instance runtime directory; the caller
//! passes the instance root (no environment switches).
//!
//! Layout below `<root>/runtime/toolhub/adapter-specs/<slug>/`:
//! - `versions/<version>.json`  immutable spec text (re-install must be byte-identical)
//! - `active.json`              `{version, sha256, activated_at, actor, previous}`
//! - `history.jsonl`            append-only activation log
use anyhow::{anyhow, bail, Context, Result};
use serde::{Deserialize, Serialize};
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

use super::adapter::{sha256_hex, SpecAdapter};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ActivePointer {
    pub version: String,
    pub sha256: String,
    pub activated_at: u64,
    pub actor: String,
    #[serde(default)]
    pub previous: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SpecStatus {
    pub slug: String,
    pub versions: Vec<String>,
    pub active: Option<ActivePointer>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct InstalledSpec {
    pub slug: String,
    pub version: String,
    pub sha256: String,
    /// false when the identical version was already installed.
    pub new: bool,
}

pub struct SpecStore {
    dir: PathBuf,
}

fn now() -> u64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0)
}

fn version_key(v: &str) -> Vec<u64> {
    v.split('.').map(|p| p.parse().unwrap_or(0)).collect()
}

fn safe_name(v: &str) -> Result<&str> {
    if v.is_empty() || v.len() > 64 || !v.bytes().all(|b| b.is_ascii_alphanumeric() || b"._-".contains(&b)) || v.starts_with('.') {
        bail!("invalid name `{v}`");
    }
    Ok(v)
}

fn write_atomic(path: &Path, bytes: &[u8]) -> Result<()> {
    let tmp = path.with_extension(format!("tmp-{}", std::process::id()));
    {
        let mut f = fs::File::create(&tmp)?;
        f.write_all(bytes)?;
        f.sync_all()?;
    }
    fs::rename(&tmp, path)?;
    Ok(())
}

impl SpecStore {
    pub fn new(dir: impl Into<PathBuf>) -> Self {
        Self { dir: dir.into() }
    }

    /// Store of the CTOX instance at `root` (same `runtime/` directory as the secret store).
    pub fn for_instance_root(root: &Path) -> Self {
        Self::new(root.join("runtime").join("toolhub").join("adapter-specs"))
    }

    pub fn dir(&self) -> &Path {
        &self.dir
    }

    fn slug_dir(&self, slug: &str) -> Result<PathBuf> {
        Ok(self.dir.join(safe_name(slug)?))
    }

    /// Validate and store a spec version (inactive). Versions are immutable.
    pub fn install(&self, text: &str) -> Result<InstalledSpec> {
        let adapter = SpecAdapter::from_json(text)?;
        let slug = crate::toolkits::ToolkitAdapter::spec(&adapter).slug.clone();
        let version = adapter.version().to_string();
        let dir = self.slug_dir(&slug)?.join("versions");
        fs::create_dir_all(&dir)?;
        let path = dir.join(format!("{}.json", safe_name(&version)?));
        if path.exists() {
            let existing = fs::read(&path)?;
            if sha256_hex(&existing) != adapter.sha256() {
                bail!("{slug} {version} is already installed with different content; bump the version");
            }
            return Ok(InstalledSpec { slug, version, sha256: adapter.sha256().into(), new: false });
        }
        write_atomic(&path, text.as_bytes())?;
        Ok(InstalledSpec { slug, version, sha256: adapter.sha256().into(), new: true })
    }

    /// Directory of one adapter (`<dir>/<slug>`), for review/approval records.
    pub fn slug_path(&self, slug: &str) -> Result<PathBuf> {
        self.slug_dir(slug)
    }

    /// Installed spec text of `slug@version`.
    pub fn version_text(&self, slug: &str, version: &str) -> Result<String> {
        self.read_version(slug, version)
    }

    /// Append an audit event to `<slug>/history.jsonl` (append-only).
    pub fn append_history(&self, slug: &str, event: serde_json::Value) -> Result<()> {
        fs::create_dir_all(self.slug_dir(slug)?)?;
        self.log(slug, event)
    }

    fn read_version(&self, slug: &str, version: &str) -> Result<String> {
        let p = self.slug_dir(slug)?.join("versions").join(format!("{}.json", safe_name(version)?));
        fs::read_to_string(&p).with_context(|| format!("{slug} {version} is not installed"))
    }

    pub fn active(&self, slug: &str) -> Result<Option<ActivePointer>> {
        let p = self.slug_dir(slug)?.join("active.json");
        if !p.exists() {
            return Ok(None);
        }
        Ok(Some(serde_json::from_slice(&fs::read(&p)?).context("active.json is corrupt")?))
    }

    fn log(&self, slug: &str, event: serde_json::Value) -> Result<()> {
        let mut f = fs::OpenOptions::new().create(true).append(true).open(self.slug_dir(slug)?.join("history.jsonl"))?;
        writeln!(f, "{event}")?;
        Ok(())
    }

    /// Point `slug` at an installed version. The caller is responsible for the
    /// approval (user activation card); `actor` is recorded for audit.
    pub fn activate(&self, slug: &str, version: &str, actor: &str) -> Result<ActivePointer> {
        let text = self.read_version(slug, version)?;
        let adapter = SpecAdapter::from_json(&text).context("stored spec no longer validates")?;
        let previous = self.active(slug)?.map(|a| a.version).filter(|v| v != version);
        let ptr = ActivePointer { version: version.into(), sha256: adapter.sha256().into(), activated_at: now(), actor: actor.into(), previous };
        write_atomic(&self.slug_dir(slug)?.join("active.json"), &serde_json::to_vec_pretty(&ptr)?)?;
        self.log(slug, serde_json::json!({"event": "activate", "version": version, "sha256": ptr.sha256, "actor": actor, "at": ptr.activated_at}))?;
        Ok(ptr)
    }

    /// Re-activate the previously active version.
    pub fn rollback(&self, slug: &str, actor: &str) -> Result<ActivePointer> {
        let cur = self.active(slug)?.ok_or_else(|| anyhow!("{slug} has no active version"))?;
        let prev = cur.previous.clone().ok_or_else(|| anyhow!("{slug} has no previous version to roll back to"))?;
        let mut ptr = self.activate(slug, &prev, actor)?;
        // The rolled-back-from version stays reachable for a roll-forward.
        ptr.previous = Some(cur.version.clone());
        write_atomic(&self.slug_dir(slug)?.join("active.json"), &serde_json::to_vec_pretty(&ptr)?)?;
        self.log(slug, serde_json::json!({"event": "rollback", "from": cur.version, "to": prev, "actor": actor, "at": now()}))?;
        Ok(ptr)
    }

    pub fn deactivate(&self, slug: &str, actor: &str) -> Result<bool> {
        let p = self.slug_dir(slug)?.join("active.json");
        if !p.exists() {
            return Ok(false);
        }
        fs::remove_file(&p)?;
        self.log(slug, serde_json::json!({"event": "deactivate", "actor": actor, "at": now()}))?;
        Ok(true)
    }

    pub fn list(&self) -> Result<Vec<SpecStatus>> {
        let mut out = Vec::new();
        let Ok(rd) = fs::read_dir(&self.dir) else { return Ok(out) };
        for e in rd.flatten() {
            let slug = e.file_name().to_string_lossy().to_string();
            if safe_name(&slug).is_err() || !e.path().is_dir() {
                continue;
            }
            let mut versions: Vec<String> = fs::read_dir(e.path().join("versions"))
                .into_iter()
                .flatten()
                .flatten()
                .filter_map(|v| v.file_name().to_string_lossy().strip_suffix(".json").map(String::from))
                .collect();
            versions.sort_by_key(|v| version_key(v));
            let active = self.active(&slug).unwrap_or(None);
            if versions.is_empty() && active.is_none() {
                // Audit-only directory (e.g. executions of a built-in toolkit).
                continue;
            }
            out.push(SpecStatus { active, slug, versions });
        }
        out.sort_by(|a, b| a.slug.cmp(&b.slug));
        Ok(out)
    }

    /// Cheap change marker over all active pointers (for reload-if-changed).
    pub fn fingerprint(&self) -> String {
        let mut acc = String::new();
        for s in self.list().unwrap_or_default() {
            if let Some(a) = s.active {
                acc.push_str(&format!("{}={}:{};", s.slug, a.version, a.sha256));
            }
        }
        sha256_hex(acc.as_bytes())
    }

    /// Compile every active spec. A spec whose file no longer matches its
    /// pointer hash or no longer validates is reported, not loaded.
    pub fn load_active(&self) -> Result<(Vec<SpecAdapter>, Vec<(String, String)>)> {
        let mut ok = Vec::new();
        let mut failed = Vec::new();
        for s in self.list()? {
            let Some(a) = s.active else { continue };
            let res = self.read_version(&s.slug, &a.version).and_then(|text| {
                let ad = SpecAdapter::from_json(&text)?;
                if ad.sha256() != a.sha256 {
                    bail!("stored spec changed after activation (hash mismatch)");
                }
                if crate::toolkits::ToolkitAdapter::spec(&ad).slug != s.slug {
                    bail!("spec slug does not match its directory");
                }
                Ok(ad)
            });
            match res {
                Ok(ad) => ok.push(ad),
                Err(e) => failed.push((s.slug, format!("{e:#}"))),
            }
        }
        Ok((ok, failed))
    }
}
