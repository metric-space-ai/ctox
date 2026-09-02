// Origin: CTOX
// License: Apache-2.0

use super::module_lifecycle::{
    installed_module_app_source, load_installed_module_manifests, module_install_scope,
    module_ships_on_first_install,
};
use super::store::{
    augment_module_manifest_file_plane, backfill_local_module_icon,
    ensure_local_icon_manifest_value, hex_sha256, is_core_module, module_asset_revision,
    source_sanitize_slug, ModuleManifest, ModuleUpsertRequest,
};
use anyhow::Context;
use serde::Serialize;
use serde_json::Value;
use std::collections::HashMap;
use std::fs;
use std::path::Path;

#[derive(Debug, Clone, Serialize)]
pub(super) struct ModuleManifestCollision {
    pub(super) module_id: String,
    pub(super) winning_path: String,
    pub(super) shadowed_path: String,
}

#[derive(Debug, Clone)]
pub(super) struct ModuleManifestLoad {
    pub(super) manifests: Vec<ModuleManifest>,
    pub(super) collisions: Vec<ModuleManifestCollision>,
}

impl IntoIterator for ModuleManifestLoad {
    type Item = ModuleManifest;
    type IntoIter = std::vec::IntoIter<ModuleManifest>;

    fn into_iter(self) -> Self::IntoIter {
        self.manifests.into_iter()
    }
}

pub(super) fn load_module_manifests(
    root: &Path,
    source_app_root: &Path,
    installed_app_root: &Path,
) -> anyhow::Result<ModuleManifestLoad> {
    let modules_root = source_app_root.join("modules");
    let mut manifests = Vec::new();
    if modules_root.is_dir() {
        let mut entries = fs::read_dir(&modules_root)?.collect::<Result<Vec<_>, _>>()?;
        entries.sort_by_key(|entry| entry.path());
        for entry in entries {
            if !entry.file_type()?.is_dir() {
                continue;
            }
            let path = entry.path().join("module.json");
            if !path.is_file() {
                continue;
            }
            let text = fs::read_to_string(&path)
                .with_context(|| format!("failed to read module manifest {}", path.display()))?;
            let manifest_value: Value = serde_json::from_str(&text)
                .with_context(|| format!("failed to parse module manifest {}", path.display()))?;
            if super::customer_apps::authorize_global_module(&entry.path(), &manifest_value)
                .is_err()
            {
                continue;
            }
            let mut manifest: ModuleManifest = serde_json::from_value(manifest_value)
                .with_context(|| format!("failed to parse module manifest {}", path.display()))?;
            manifest.manifest_sha256 = hex_sha256(text.as_bytes());
            manifest.asset_revision = module_asset_revision(&entry.path())?;
            manifest.local_manifest_path = path.display().to_string();
            backfill_local_module_icon(&mut manifest, &entry.path());
            augment_module_manifest_file_plane(&mut manifest, &entry.path());
            if manifest.entry.is_empty() {
                manifest.entry = format!("modules/{}/index.html", manifest.id);
            }
            let scope = module_install_scope(&manifest);
            if !module_ships_on_first_install(&scope) {
                continue;
            }
            let core = scope == "core";
            manifest.install_scope = scope;
            manifest.default_installed = true;
            manifest.source = if core { "core" } else { "internal" }.to_owned();
            manifest.origin = "core".to_owned();
            manifest.core = core;
            manifest.editable = true;
            manifest.deletable = !core;
            manifests.push(manifest);
        }
    }

    let mut winning_roots = manifests
        .iter()
        .map(|manifest| (manifest.id.clone(), "source"))
        .collect::<HashMap<_, _>>();
    let mut collisions = Vec::new();
    let mut installed_manifests = load_installed_module_manifests(root, installed_app_root)?;
    for manifest in &mut installed_manifests {
        manifest.origin = installed_module_origin(installed_app_root, &manifest.id).to_owned();
    }
    append_lower_precedence_manifests(
        &mut manifests,
        installed_manifests,
        "installed",
        &mut winning_roots,
        &mut collisions,
    );
    append_lower_precedence_manifests(
        &mut manifests,
        load_local_module_manifests(root, installed_app_root, true)?,
        "local",
        &mut winning_roots,
        &mut collisions,
    );

    manifests.sort_by(|a, b| match (a.id.as_str(), b.id.as_str()) {
        ("ctox", "ctox") => std::cmp::Ordering::Equal,
        ("ctox", _) => std::cmp::Ordering::Less,
        (_, "ctox") => std::cmp::Ordering::Greater,
        _ => a.title.cmp(&b.title).then_with(|| a.id.cmp(&b.id)),
    });
    collisions.sort_by(|a, b| {
        a.module_id
            .cmp(&b.module_id)
            .then_with(|| a.winning_path.cmp(&b.winning_path))
            .then_with(|| a.shadowed_path.cmp(&b.shadowed_path))
    });
    for collision in &collisions {
        eprintln!(
            "[business-os] WARNING: DUPLICATE MODULE ID `{}`; keeping `{}` and ignoring `{}`",
            collision.module_id, collision.winning_path, collision.shadowed_path
        );
    }

    Ok(ModuleManifestLoad {
        manifests,
        collisions,
    })
}

fn installed_module_origin(installed_app_root: &Path, module_id: &str) -> &'static str {
    let first_party = installed_module_app_source(installed_app_root, module_id)
        .and_then(|source| {
            source
                .get("trust_model")
                .and_then(Value::as_str)
                .map(str::to_owned)
        })
        .is_some_and(|trust_model| trust_model == "ctox-first-party-source");
    // Everything else — notably `untrusted-user-source` (zip upload, download
    // URL, foreign repository) and any missing stamp — is a user app. The
    // stamp is server-written at install time; a manifest cannot claim it.
    if first_party {
        "official"
    } else {
        "user"
    }
}

fn append_lower_precedence_manifests(
    manifests: &mut Vec<ModuleManifest>,
    mut candidates: Vec<ModuleManifest>,
    candidate_root: &'static str,
    winning_roots: &mut HashMap<String, &'static str>,
    collisions: &mut Vec<ModuleManifestCollision>,
) {
    candidates.sort_by(|a, b| {
        a.local_manifest_path
            .cmp(&b.local_manifest_path)
            .then_with(|| a.id.cmp(&b.id))
    });
    for manifest in candidates {
        if let Some(existing) = manifests.iter().find(|existing| existing.id == manifest.id) {
            if winning_roots.get(&manifest.id).copied() != Some(candidate_root) {
                collisions.push(ModuleManifestCollision {
                    module_id: manifest.id,
                    winning_path: existing.local_manifest_path.clone(),
                    shadowed_path: manifest.local_manifest_path,
                });
            }
            continue;
        }
        winning_roots.insert(manifest.id.clone(), candidate_root);
        manifests.push(manifest);
    }
}

/// Loads operator-owned local modules; app-store lifecycle never manages them (`deletable=false`).
pub(super) fn load_local_module_manifests(
    root: &Path,
    app_root: &Path,
    enrich: bool,
) -> anyhow::Result<Vec<ModuleManifest>> {
    let modules_root = app_root.join("local-modules");
    let mut manifests = Vec::new();
    if !modules_root.is_dir() {
        return Ok(manifests);
    }
    for entry in fs::read_dir(&modules_root)? {
        let entry = entry?;
        if !entry.file_type()?.is_dir() {
            continue;
        }
        let path = entry.path().join("module.json");
        if !path.is_file() {
            continue;
        }
        let text = fs::read_to_string(&path)
            .with_context(|| format!("failed to read module manifest {}", path.display()))?;
        let manifest_value: Value = serde_json::from_str(&text)
            .with_context(|| format!("failed to parse module manifest {}", path.display()))?;
        if super::customer_apps::authorize_runtime_module(root, &entry.path(), &manifest_value)
            .is_err()
        {
            continue;
        }
        let mut manifest: ModuleManifest = serde_json::from_value(manifest_value)
            .with_context(|| format!("failed to parse module manifest {}", path.display()))?;
        manifest.local_manifest_path = path.display().to_string();
        if enrich {
            manifest.manifest_sha256 = hex_sha256(text.as_bytes());
            manifest.asset_revision = module_asset_revision(&entry.path())?;
            augment_module_manifest_file_plane(&mut manifest, &entry.path());
            backfill_local_module_icon(&mut manifest, &entry.path());
        }
        if manifest.install_scope.trim().eq_ignore_ascii_case("sample") {
            continue;
        }
        if is_core_module(&manifest.id) {
            continue;
        }
        manifest.entry = format!("local-modules/{}/index.html", manifest.id);
        manifest.source = "local".to_owned();
        manifest.origin = "user".to_owned();
        manifest.install_scope = "local".to_owned();
        manifest.default_installed = false;
        manifest.core = false;
        manifest.editable = true;
        // Local modules are operator/developer-mode user apps (origin: user).
        // They used to be non-deletable, which left an imported app with no
        // removal path in any surface — the uninstall command now handles the
        // local-modules root as well.
        manifest.deletable = true;
        manifests.push(manifest);
    }
    Ok(manifests)
}

pub(super) fn upsert_module_manifest(
    source_app_root: &Path,
    installed_app_root: &Path,
    request: ModuleUpsertRequest,
) -> anyhow::Result<ModuleManifest> {
    let module_id = source_sanitize_slug(&request.id);
    anyhow::ensure!(!module_id.is_empty(), "module id is required");
    let title = request.title.trim();
    anyhow::ensure!(!title.is_empty(), "module title is required");
    let is_core = is_core_module(&module_id);
    let target = if is_core {
        source_app_root.join("modules").join(&module_id)
    } else {
        installed_app_root
            .join("installed-modules")
            .join(&module_id)
    };
    let manifest_path = target.join("module.json");
    if !manifest_path.is_file() {
        anyhow::bail!(
            "module `{module_id}` does not exist. Create new Business OS apps through the App Creator (`ctox.business_os.app.create`) or install a shipped template; `ctox.module.save` only updates existing module manifests."
        );
    }
    let mut manifest_value: Value = serde_json::from_str(
        &fs::read_to_string(&manifest_path)
            .with_context(|| format!("failed to read {}", manifest_path.display()))?,
    )?;
    manifest_value["id"] = Value::String(module_id.clone());
    manifest_value["title"] = Value::String(title.to_owned());
    manifest_value["description"] = Value::String(request.description.trim().to_owned());
    let requested_version = request.version.trim();
    if !requested_version.is_empty() {
        manifest_value["version"] = Value::String(requested_version.to_owned());
    } else if manifest_value
        .get("version")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .trim()
        .is_empty()
    {
        manifest_value["version"] = Value::String("0.1.0".to_owned());
    }
    let entry = if is_core {
        format!("modules/{module_id}/index.html")
    } else if request.entry.trim().is_empty() {
        format!("installed-modules/{module_id}/index.html")
    } else {
        request.entry.trim().to_owned()
    };
    manifest_value["entry"] = Value::String(entry);
    manifest_value["collections"] = Value::Array(
        request
            .collections
            .into_iter()
            .map(|item| item.trim().to_owned())
            .filter(|item| !item.is_empty())
            .map(Value::String)
            .collect(),
    );
    if !request.layout.is_null() {
        manifest_value["layout"] = request.layout;
    }
    if !is_core {
        ensure_local_icon_manifest_value(&mut manifest_value, &target);
    }
    fs::write(&manifest_path, serde_json::to_vec_pretty(&manifest_value)?)
        .with_context(|| format!("failed to write {}", manifest_path.display()))?;

    let mut manifest: ModuleManifest = serde_json::from_value(manifest_value)?;
    manifest.source = if is_core { "core" } else { "installed" }.to_owned();
    manifest.origin = if is_core { "core" } else { "user" }.to_owned();
    manifest.install_scope = if is_core { "core" } else { "installed" }.to_owned();
    manifest.default_installed = is_core;
    manifest.core = is_core;
    manifest.editable = true;
    manifest.deletable = !is_core;
    Ok(manifest)
}
