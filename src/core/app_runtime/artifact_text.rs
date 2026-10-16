//! Text, skills and prompts are resolved from immutable installed CAS releases.
//! This module never consults the host filesystem or a second skill registry.
use crate::delivery::ReleaseArtifacts;
use anyhow::{ensure, Context, Result};
use serde::Serialize;
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;

pub const TEXT_LIMIT: usize = 1024 * 1024;

/// A reference is an app-package path plus its immutable CAS hash. Binary
/// references are not decoded as text; callers can use the files primitive.
#[derive(Debug, Serialize)]
pub struct AppSkill {
    pub app_id: String,
    pub app_ver: String,
    pub name: String,
    pub path: String,
    pub instruction: String,
    pub references: BTreeMap<String, String>,
}

pub fn text(source: &dyn ReleaseArtifacts, manifest: &Value, path: &str) -> Result<String> {
    ensure!(crate::safe_path(path), "invalid: app-relative asset path");
    let hash = manifest["files"]
        .get(path)
        .and_then(Value::as_str)
        .context("not_found: asset is not in this app version")?;
    ensure!(
        hash.len() == 64 && hash.bytes().all(|b| b.is_ascii_hexdigit()),
        "invalid: asset CAS hash"
    );
    let bytes = source.bytes(hash)?;
    ensure!(bytes.len() <= TEXT_LIMIT, "quota: text asset exceeds 1 MiB");
    ensure!(
        format!("{:x}", Sha256::digest(&bytes)) == hash,
        "conflict: asset CAS integrity"
    );
    String::from_utf8(bytes).context("invalid: asset is not UTF-8")
}

pub fn skill(source: &dyn ReleaseArtifacts, manifest: &Value, name: &str) -> Result<AppSkill> {
    ensure!(
        !name.is_empty()
            && name
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-'),
        "invalid: skill name"
    );
    let definitions = manifest["skills"]
        .as_array()
        .context("not_found: app has no skills")?;
    let matching: Vec<_> = definitions.iter().filter(|s| s["name"] == name).collect();
    ensure!(
        matching.len() == 1,
        "not_found: unambiguous app skill required"
    );
    let directory = format!("skills/{name}/");
    let expected = format!("{directory}SKILL.md");
    ensure!(
        matching[0]["path"] == expected,
        "invalid: skill path must be namespaced"
    );
    let instruction = text(source, manifest, &expected)?;
    let mut references = BTreeMap::new();
    for (path, hash) in manifest["files"]
        .as_object()
        .context("invalid: app files")?
    {
        if path.starts_with(&directory) && path != &expected {
            ensure!(crate::safe_path(path), "invalid: skill reference path");
            let hash = hash.as_str().context("invalid: skill reference hash")?;
            ensure!(
                hash.len() == 64 && hash.bytes().all(|b| b.is_ascii_hexdigit()),
                "invalid: skill reference hash"
            );
            references.insert(path.clone(), hash.to_owned());
        }
    }
    Ok(AppSkill {
        app_id: crate::string(manifest, "app_id")?.to_owned(),
        app_ver: crate::string(manifest, "app_ver")?.to_owned(),
        name: name.to_owned(),
        path: expected,
        instruction,
        references,
    })
}

pub fn prompt(source: &dyn ReleaseArtifacts, manifest: &Value, path: &str) -> Result<String> {
    ensure!(
        path.starts_with("prompts/") && path.ends_with(".md"),
        "invalid: prompt path"
    );
    text(source, manifest, path)
}
