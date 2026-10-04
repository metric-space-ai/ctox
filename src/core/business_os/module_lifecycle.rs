// Origin: CTOX
// License: Apache-2.0

use super::policy::{BusinessOsPermission, PolicyDecision};
use super::rxdb_peer::sync_module_catalog as sync_module_catalog_projection_now;
use super::session::session_user_id;
use super::store::{
    activate_staged_module_directory, app_root_for_module_manifest,
    augment_module_manifest_file_plane, backfill_local_module_icon, compute_module_bundle,
    copy_dir_recursive, decode_verified_desktop_file_or_equivalent,
    ensure_local_icon_manifest_value, find_module_json_dir_for_install, github_archive_url,
    hex_sha256, is_allowed_source_path, is_core_module, load_module_layout,
    materialize_runtime_app_starter_artifacts_at, module_asset_revision, module_catalog_source_id,
    module_governance_map, module_manifest_collection_ids, module_manifest_path,
    module_policy_decision, normalize_source_relative_path, now_ms, open_store,
    parse_business_app_semver_major, remove_module_from_layout_value, resolve_module_source_root,
    resolve_module_source_root_for_root, runtime_app_starter_is_owned, rxdb_desktop_file_chunks,
    rxdb_desktop_file_document, sanitize_business_os_workspace_component, sanitize_git_ref,
    save_module_layout, save_module_source_record, seed_session_user, session_can_modify_module,
    session_has_workspace_permission, source_relative_subpath, source_sanitize_slug,
    update_module_catalog_stamp_hash, validate_github_repo, validate_runtime_app_starter_artifacts,
    validate_staged_catalog_module, version_summary_row, write_module_source_snapshot,
    AppStoreInstallRequest, BusinessCommand, BusinessOsSession, CommandOrigin, ModuleDeleteRequest,
    ModuleInstallTemplateRequest, ModuleManifest, ModuleReleaseRequest, ModuleRollbackRequest,
    ModuleSourceRollbackSnapshotRequest, ModuleSourceSaveMutation, ModuleVersionListRequest,
    ModuleVersionRollbackRequest, RuntimeAppStarterAction, TemplateManifest,
};
use super::store_projections::upsert_business_record;
use super::store_release_review::{
    data_access_review_from_release_snapshot, module_release_data_access_review_summary,
    release_review_data_access_projection,
};
use anyhow::Context;
use rusqlite::{params, Connection, OptionalExtension};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};
use uuid::Uuid;

pub(super) fn module_version_timeline_policy_decision(
    root: &Path,
    session: &BusinessOsSession,
    module_id: &str,
) -> anyhow::Result<PolicyDecision> {
    let permissions = [
        BusinessOsPermission::AppsSourceView,
        BusinessOsPermission::AppsRelease,
        BusinessOsPermission::AppsRollback,
    ];
    let mut denied: Option<PolicyDecision> = None;
    for permission in permissions {
        let decision = module_policy_decision(root, session, permission, module_id)?;
        if decision.allowed {
            return Ok(decision);
        }
        denied.get_or_insert(decision);
    }
    denied.context("failed to evaluate module version timeline policy")
}

fn public_runtime_app_line_major(
    conn: &Connection,
    module_id: &str,
    manifest: &Value,
) -> anyhow::Result<Option<u64>> {
    let latest_release = conn
        .query_row(
            "SELECT snapshot_json, manifest_json
             FROM business_module_releases
             WHERE module_id = ?1 AND status = 'released'
             ORDER BY version DESC
             LIMIT 1",
            params![module_id],
            |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)),
        )
        .optional()?;
    if let Some((snapshot_json, manifest_json)) = latest_release {
        let snapshot = serde_json::from_str::<Value>(&snapshot_json).unwrap_or(Value::Null);
        if let Some(major) = snapshot
            .get("target_version")
            .and_then(Value::as_str)
            .and_then(parse_business_app_semver_major)
            .filter(|major| *major >= 1)
        {
            return Ok(Some(major));
        }
        let release_manifest = serde_json::from_str::<Value>(&manifest_json).unwrap_or(Value::Null);
        if let Some(major) = release_manifest
            .get("version")
            .and_then(Value::as_str)
            .and_then(parse_business_app_semver_major)
            .filter(|major| *major >= 1)
        {
            return Ok(Some(major));
        }
    }
    Ok(manifest
        .get("version")
        .and_then(Value::as_str)
        .and_then(parse_business_app_semver_major)
        .filter(|major| *major >= 1))
}

fn normalized_module_release_channel(raw: &str) -> anyhow::Result<String> {
    let value = raw.trim().to_ascii_lowercase();
    if value.is_empty() {
        return Ok("team".to_owned());
    }
    match value.as_str() {
        "team" | "restricted" => Ok(value),
        _ => anyhow::bail!("release_channel must be team or restricted"),
    }
}

fn manifest_value_is_runtime_installed(manifest: &Value) -> bool {
    let source = manifest
        .get("source")
        .and_then(Value::as_str)
        .unwrap_or_default();
    let install_scope = manifest
        .get("install_scope")
        .and_then(Value::as_str)
        .unwrap_or_default();
    let entry = manifest
        .get("entry")
        .and_then(Value::as_str)
        .unwrap_or_default();
    source == "installed" || install_scope == "installed" || entry.starts_with("installed-modules/")
}

fn ensure_module_version_ref_exists(
    conn: &Connection,
    module_id: &str,
    version_id: &str,
    field_name: &str,
) -> anyhow::Result<()> {
    let version_id = version_id.trim();
    if version_id.is_empty() {
        return Ok(());
    }
    let exists = module_version_ref_exists(conn, module_id, version_id)?;
    anyhow::ensure!(
        exists,
        "{field_name} must reference an existing module version for this module"
    );
    Ok(())
}

fn module_version_ref_exists(
    conn: &Connection,
    module_id: &str,
    version_id: &str,
) -> anyhow::Result<bool> {
    let version_id = version_id.trim();
    if version_id.is_empty() {
        return Ok(false);
    }
    Ok(conn
        .query_row(
            "SELECT 1
             FROM business_module_versions
             WHERE module_id = ?1 AND version_id = ?2
             LIMIT 1",
            params![module_id, version_id],
            |_| Ok(()),
        )
        .optional()?
        .is_some())
}

fn release_snapshot_string(release: &Value, key: &str) -> String {
    release
        .get("snapshot")
        .and_then(|snapshot| snapshot.get(key))
        .and_then(Value::as_str)
        .unwrap_or_default()
        .trim()
        .to_owned()
}

pub(super) fn module_release_lifecycle_summary(release: &Value) -> Value {
    serde_json::json!({
        "version_id": release
            .get("version_id")
            .cloned()
            .unwrap_or(Value::Null),
        "version": release
            .get("version")
            .cloned()
            .unwrap_or(Value::Null),
        "status": release
            .get("status")
            .cloned()
            .unwrap_or(Value::Null),
        "target_version": release_snapshot_string(release, "target_version"),
        "release_channel": release_snapshot_string(release, "release_channel"),
        "source_version_id": release_snapshot_string(release, "source_version_id"),
        "rollback_version_id": release_snapshot_string(release, "rollback_version_id"),
        "created_by": release
            .get("created_by")
            .cloned()
            .unwrap_or(Value::Null),
        "created_at_ms": release
            .get("created_at_ms")
            .cloned()
            .unwrap_or(Value::Null),
        "notes": release
            .get("notes")
            .cloned()
            .unwrap_or(Value::Null),
    })
}

pub(super) fn module_is_runtime_installed(manifest: &ModuleManifest) -> bool {
    manifest.source == "installed"
        || manifest.install_scope == "installed"
        || manifest.entry.trim().starts_with("installed-modules/")
}

pub fn install_template_module_command(
    root: &Path,
    source_app_root: &Path,
    installed_app_root: &Path,
    session: &BusinessOsSession,
    request: ModuleInstallTemplateRequest,
) -> anyhow::Result<Value> {
    anyhow::ensure!(
        session_has_workspace_permission(root, session, BusinessOsPermission::AppsInstall)?,
        "chef or admin role required"
    );
    let manifest = install_template_module(root, source_app_root, installed_app_root, request)?;
    let created_by = session_user_id(session).unwrap_or("").to_string();
    record_module_version(
        root,
        installed_app_root,
        &manifest.id,
        "install",
        "Installed from template",
        &created_by,
    )?;
    Ok(serde_json::json!({
        "ok": true,
        "module_id": manifest.id,
        "module": manifest
    }))
}

/// Baseline bundle hash for a module: the bundle this instance last
/// installed/updated from (latest install/update/app_create row), falling back
/// to the earliest recorded version. Mirrors `module_version_states` so the
/// update precondition and the projected `update_available` agree.
pub(super) fn installed_baseline_bundle_sha(
    root: &Path,
    module_id: &str,
) -> anyhow::Result<String> {
    let conn = open_store(root)?;
    let sha: Option<String> = conn
        .query_row(
            "SELECT bundle_sha256 FROM business_module_versions v1
             WHERE v1.module_id = ?1
               AND v1.seq = COALESCE(
                   (SELECT MAX(seq) FROM business_module_versions v2
                    WHERE v2.module_id = ?1
                      AND v2.origin IN ('install', 'update', 'app_create')),
                   (SELECT MIN(seq) FROM business_module_versions v3
                    WHERE v3.module_id = ?1))",
            params![module_id],
            |row| row.get(0),
        )
        .optional()?;
    Ok(sha.unwrap_or_default())
}

pub(super) fn release_managed_shadow_source(
    source_app_root: &Path,
    installed_manifest: &Value,
) -> anyhow::Result<Option<String>> {
    let Some(source_module_id) = module_catalog_source_id(source_app_root, installed_manifest)
    else {
        return Ok(None);
    };
    let source_manifest_path = source_app_root
        .join("modules")
        .join(&source_module_id)
        .join("module.json");
    if !source_manifest_path.is_file() {
        return Ok(None);
    }
    let source_manifest: ModuleManifest = serde_json::from_slice(&fs::read(&source_manifest_path)?)
        .with_context(|| {
            format!(
                "failed to parse release-managed module manifest {}",
                source_manifest_path.display()
            )
        })?;
    if !module_ships_on_first_install(&module_install_scope(&source_manifest))
        || source_manifest.developer.trim() != "CTOX"
        || installed_manifest
            .get("developer")
            .and_then(Value::as_str)
            .map(str::trim)
            != Some("CTOX")
    {
        return Ok(None);
    }

    let explicit_catalog_clone = installed_manifest
        .get("source_module_id")
        .and_then(Value::as_str)
        .map(str::trim)
        == Some(source_module_id.as_str())
        || (installed_manifest
            .pointer("/app_source/kind")
            .and_then(Value::as_str)
            == Some("catalog")
            && installed_manifest
                .pointer("/app_source/verified")
                .and_then(Value::as_bool)
                == Some(true));
    Ok(explicit_catalog_clone.then_some(source_module_id))
}

pub(super) fn release_managed_module_payload_sha(module_dir: &Path) -> anyhow::Result<String> {
    fn collect(
        root: &Path,
        current: &Path,
        files: &mut Vec<(PathBuf, PathBuf)>,
    ) -> anyhow::Result<()> {
        let mut entries = fs::read_dir(current)?
            .collect::<Result<Vec<_>, _>>()
            .with_context(|| format!("failed to list module payload {}", current.display()))?;
        entries.sort_by_key(|entry| entry.file_name());
        for entry in entries {
            let path = entry.path();
            let file_type = entry.file_type()?;
            if file_type.is_dir() {
                collect(root, &path, files)?;
            } else if file_type.is_file()
                && path.strip_prefix(root).ok() != Some(Path::new("module.json"))
            {
                files.push((path.strip_prefix(root)?.to_path_buf(), path));
            }
        }
        Ok(())
    }

    let mut files = Vec::new();
    collect(module_dir, module_dir, &mut files)?;
    let mut hasher = Sha256::new();
    for (relative, path) in files {
        let relative = relative.to_string_lossy();
        update_module_catalog_stamp_hash(&mut hasher, &relative);
        let bytes = fs::read(&path)
            .with_context(|| format!("failed to read module payload {}", path.display()))?;
        hasher.update(bytes.len().to_le_bytes());
        hasher.update(bytes);
    }
    Ok(format!("{:x}", hasher.finalize()))
}

pub fn delete_installed_module_command(
    root: &Path,
    app_root: &Path,
    session: &BusinessOsSession,
    request: ModuleDeleteRequest,
) -> anyhow::Result<Value> {
    let module_id = source_sanitize_slug(&request.module_id);
    anyhow::ensure!(!module_id.is_empty(), "module id is required");
    anyhow::ensure!(
        session_can_modify_module(root, session, &module_id)?,
        "module modification rights required"
    );
    delete_installed_module(
        app_root,
        root,
        ModuleDeleteRequest {
            module_id: module_id.clone(),
        },
    )?;
    Ok(serde_json::json!({
        "ok": true,
        "module_id": module_id,
        "deleted": true
    }))
}

pub fn record_module_release(
    root: &Path,
    app_root: &Path,
    session: &BusinessOsSession,
    request: ModuleReleaseRequest,
) -> anyhow::Result<Value> {
    let module_id = request.module_id.trim();
    anyhow::ensure!(!module_id.is_empty(), "module_id is required");
    let release_decision =
        module_policy_decision(root, session, BusinessOsPermission::AppsRelease, module_id)?;
    anyhow::ensure!(
        release_decision.allowed,
        "{}",
        release_decision.display_reason
    );
    let manifest_path = module_manifest_path(root, app_root, module_id)?;
    let version_app_root = app_root_for_module_manifest(app_root, &manifest_path);
    let manifest_json = std::fs::read_to_string(&manifest_path)
        .with_context(|| format!("failed to read {}", manifest_path.display()))?;
    let original_manifest_json = manifest_json.clone();
    let mut manifest_value: Value = serde_json::from_str(&manifest_json)
        .with_context(|| format!("failed to parse {}", manifest_path.display()))?;
    let runtime_installed = manifest_value_is_runtime_installed(&manifest_value);
    let release_channel = normalized_module_release_channel(&request.release_channel)?;
    let requested_target_version = request.target_version.trim().to_owned();
    let source_version_id = request.source_version_id.trim().to_owned();
    let rollback_version_id = request.rollback_version_id.trim().to_owned();
    let notes = request.notes.trim().to_owned();
    let responsible_user_ids = request.responsible_user_ids;
    let target_version = if requested_target_version.is_empty() {
        manifest_value
            .get("version")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .trim()
            .to_owned()
    } else {
        requested_target_version
    };
    let now = now_ms() as i64;
    let mut conn = open_store(root)?;
    ensure_module_version_ref_exists(&conn, module_id, &source_version_id, "source_version_id")?;
    ensure_module_version_ref_exists(
        &conn,
        module_id,
        &rollback_version_id,
        "rollback_version_id",
    )?;
    let data_access_review = if runtime_installed {
        let semver_major = parse_business_app_semver_major(&target_version);
        anyhow::ensure!(
            semver_major.is_some(),
            "target_version must be plain SemVer x.y.z"
        );
        let target_major = semver_major.unwrap_or(0);
        anyhow::ensure!(
            target_major >= 1,
            "Team release requires target_version >= 1.0.0"
        );
        if let Some(existing_major) =
            public_runtime_app_line_major(&conn, module_id, &manifest_value)?
        {
            anyhow::ensure!(
                existing_major == target_major,
                "major app versions require a separate Business OS app line; create a new runtime module id for {target_version} instead of releasing over this v{existing_major}.x app"
            );
        }
        module_release_data_access_review_summary(
            &conn,
            module_id,
            &request.data_access_review,
            &module_manifest_collection_ids(&manifest_value),
            now,
        )?
    } else {
        Value::Null
    };
    if runtime_installed
        && runtime_app_starter_is_owned(&version_app_root.join("installed-modules").join(module_id))
    {
        validate_runtime_app_starter_artifacts(root, module_id)
            .context("release validator/test gate failed")?;
    }
    if runtime_installed {
        if let Some(object) = manifest_value.as_object_mut() {
            object.insert("version".to_owned(), Value::String(target_version.clone()));
            let lifecycle = object
                .entry("lifecycle".to_owned())
                .or_insert_with(|| serde_json::json!({}));
            if !lifecycle.is_object() {
                *lifecycle = serde_json::json!({});
            }
            if let Some(lifecycle_object) = lifecycle.as_object_mut() {
                lifecycle_object.insert(
                    "visibility_state".to_owned(),
                    Value::String(release_channel.clone()),
                );
                lifecycle_object.insert(
                    "audience".to_owned(),
                    Value::String(release_channel.clone()),
                );
                lifecycle_object.insert(
                    "release_channel".to_owned(),
                    Value::String(release_channel.clone()),
                );
            }
        }
        std::fs::write(&manifest_path, serde_json::to_vec_pretty(&manifest_value)?)
            .with_context(|| format!("failed to write {}", manifest_path.display()))?;
    }
    let snapshot = serde_json::json!({
        "module_json": manifest_value,
        "path": manifest_path.display().to_string(),
        "target_version": target_version,
        "release_channel": release_channel,
        "source_version_id": source_version_id,
        "rollback_version_id": rollback_version_id,
        "responsible_user_ids": responsible_user_ids,
        "data_access_review": data_access_review
    });
    let created_by = session_user_id(session).unwrap_or("").to_owned();
    let db_result = (|| -> anyhow::Result<(String, Vec<String>)> {
        let tx = conn.transaction()?;
        seed_session_user(&tx, session)?;
        let next_version: i64 = tx.query_row(
            "SELECT COALESCE(MAX(version), 0) + 1 FROM business_module_releases WHERE module_id = ?1",
            params![module_id],
            |row| row.get(0),
        )?;
        let version_id = format!("modrel_{}_{}_{}", module_id, next_version, Uuid::new_v4());
        tx.execute(
            "UPDATE business_module_releases SET status = 'rolled_back' WHERE module_id = ?1 AND status = 'released'",
            params![module_id],
        )?;
        tx.execute(
            "INSERT INTO business_module_releases
                (version_id, module_id, version, status, manifest_json, snapshot_json, created_by, created_at_ms, notes)
             VALUES (?1, ?2, ?3, 'released', ?4, ?5, ?6, ?7, ?8)",
            params![
                version_id,
                module_id,
                next_version,
                serde_json::to_string(&manifest_value)?,
                serde_json::to_string(&snapshot)?,
                created_by,
                now,
                notes
            ],
        )?;
        let release_ids = sync_module_release_records(&tx, module_id, now)?;
        record_module_version_with_conn(
            &tx,
            &version_app_root,
            module_id,
            "manual_release",
            &format!("Release v{next_version}"),
            &created_by,
        )?;
        tx.commit()?;
        Ok((version_id, release_ids))
    })();
    let (version_id, release_ids) = match db_result {
        Ok(result) => result,
        Err(error) => {
            if runtime_installed {
                if let Err(restore_error) =
                    std::fs::write(&manifest_path, original_manifest_json.as_bytes())
                {
                    anyhow::bail!(
                        "{error}; additionally failed to restore {} after release failure: {restore_error}",
                        manifest_path.display()
                    );
                }
            }
            return Err(error);
        }
    };
    // A completed release command is an authorization to consume the new
    // lifecycle state. Commit its authoritative RxDB catalog projection before
    // the command outcome is acknowledged; otherwise browsers can observe a
    // terminal success while business_module_catalog still advertises the old
    // private version until the periodic projection sweep runs.
    sync_module_catalog_projection_now(root)
        .context("released module was persisted but its RxDB catalog projection failed")?;
    let mut governance = module_governance_map(root, session)?;
    if let Some(object) = governance.as_object_mut() {
        object.insert(
            "module_id".to_string(),
            Value::String(module_id.to_string()),
        );
        object.insert("version_id".to_string(), Value::String(version_id));
        object.insert(
            "business_module_release_ids".to_string(),
            Value::Array(release_ids.into_iter().map(Value::String).collect()),
        );
    }
    Ok(governance)
}

pub fn rollback_module_release(
    root: &Path,
    app_root: &Path,
    session: &BusinessOsSession,
    request: ModuleRollbackRequest,
) -> anyhow::Result<Value> {
    let module_id = request.module_id.trim();
    let version_id = request.version_id.trim();
    anyhow::ensure!(!module_id.is_empty(), "module_id is required");
    anyhow::ensure!(!version_id.is_empty(), "version_id is required");
    let rollback_decision =
        module_policy_decision(root, session, BusinessOsPermission::AppsRollback, module_id)?;
    anyhow::ensure!(
        rollback_decision.allowed,
        "{}",
        rollback_decision.display_reason
    );
    let mut conn = open_store(root)?;
    let manifest_json: String = conn.query_row(
        "SELECT manifest_json FROM business_module_releases WHERE module_id = ?1 AND version_id = ?2",
        params![module_id, version_id],
        |row| row.get(0),
    )?;
    let manifest_path = module_manifest_path(root, app_root, module_id)?;
    let original_manifest_json = std::fs::read_to_string(&manifest_path)
        .with_context(|| format!("failed to read {}", manifest_path.display()))?;
    std::fs::write(
        &manifest_path,
        serde_json::to_vec_pretty(&serde_json::from_str::<Value>(&manifest_json)?)?,
    )
    .with_context(|| format!("failed to write {}", manifest_path.display()))?;
    let now = now_ms() as i64;
    let release_ids = match (|| -> anyhow::Result<Vec<String>> {
        let tx = conn.transaction()?;
        tx.execute(
            "UPDATE business_module_releases SET status = CASE WHEN version_id = ?2 THEN 'released' ELSE 'rolled_back' END WHERE module_id = ?1",
            params![module_id, version_id],
        )?;
        let release_ids = sync_module_release_records(&tx, module_id, now)?;
        tx.commit()?;
        Ok(release_ids)
    })() {
        Ok(release_ids) => release_ids,
        Err(error) => {
            if let Err(restore_error) =
                std::fs::write(&manifest_path, original_manifest_json.as_bytes())
            {
                anyhow::bail!(
                    "{error}; additionally failed to restore {} after rollback failure: {restore_error}",
                    manifest_path.display()
                );
            }
            return Err(error);
        }
    };
    sync_module_catalog_projection_now(root)
        .context("module rollback was persisted but its RxDB catalog projection failed")?;
    let mut governance = module_governance_map(root, session)?;
    if let Some(object) = governance.as_object_mut() {
        object.insert(
            "module_id".to_string(),
            Value::String(module_id.to_string()),
        );
        object.insert(
            "version_id".to_string(),
            Value::String(version_id.to_string()),
        );
        object.insert("rolled_back_at_ms".to_string(), Value::from(now));
        object.insert(
            "business_module_release_ids".to_string(),
            Value::Array(release_ids.into_iter().map(Value::String).collect()),
        );
    }
    Ok(governance)
}

pub(super) fn sync_module_release_records(
    conn: &Connection,
    module_id: &str,
    updated_at_ms: i64,
) -> anyhow::Result<Vec<String>> {
    let mut stmt = conn.prepare(
        "SELECT version_id, module_id, version, status, created_by, created_at_ms, notes, snapshot_json
         FROM business_module_releases
         WHERE module_id = ?1
         ORDER BY version DESC",
    )?;
    let rows = stmt.query_map(params![module_id], |row| {
        Ok((
            row.get::<_, String>(0)?,
            row.get::<_, String>(1)?,
            row.get::<_, i64>(2)?,
            row.get::<_, String>(3)?,
            row.get::<_, String>(4)?,
            row.get::<_, i64>(5)?,
            row.get::<_, String>(6)?,
            row.get::<_, String>(7)?,
        ))
    })?;
    let release_rows = rows.collect::<rusqlite::Result<Vec<_>>>()?;
    drop(stmt);
    let mut release_ids = Vec::new();
    for (version_id, module_id, version, status, created_by, created_at_ms, notes, snapshot_json) in
        release_rows
    {
        let snapshot = serde_json::from_str::<Value>(&snapshot_json).unwrap_or(Value::Null);
        let data_access_review = data_access_review_from_release_snapshot(&snapshot);
        let record_updated_at = next_business_record_updated_at(
            conn,
            "business_module_releases",
            &version_id,
            updated_at_ms,
        )?;
        upsert_business_record(
            conn,
            "business_module_releases",
            &version_id,
            record_updated_at,
            serde_json::json!({
                "id": version_id.clone(),
                "version_id": version_id.clone(),
                "module_id": module_id,
                "version": version,
                "status": status,
                "created_by": created_by,
                "created_at_ms": created_at_ms,
                "notes": notes,
                "target_version": snapshot
                    .get("target_version")
                    .and_then(Value::as_str)
                    .unwrap_or_default(),
                "release_channel": snapshot
                    .get("release_channel")
                    .and_then(Value::as_str)
                    .unwrap_or_default(),
                "source_version_id": snapshot
                    .get("source_version_id")
                    .and_then(Value::as_str)
                    .unwrap_or_default(),
                "rollback_version_id": snapshot
                    .get("rollback_version_id")
                    .and_then(Value::as_str)
                    .unwrap_or_default(),
                "data_access_review": data_access_review,
                "data_access": release_review_data_access_projection(&data_access_review),
                "updated_at_ms": record_updated_at
            }),
        )?;
        release_ids.push(version_id);
    }
    Ok(release_ids)
}

pub(super) fn repair_invalid_module_release_version_refs(
    conn: &Connection,
    module_ids: &[String],
    dry_run: bool,
) -> anyhow::Result<Vec<Value>> {
    let mut actions = Vec::new();
    for module_id in module_ids {
        let mut stmt = conn.prepare(
            "SELECT version_id, snapshot_json
             FROM business_module_releases
             WHERE module_id = ?1
             ORDER BY version DESC",
        )?;
        let rows = stmt
            .query_map(params![module_id], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        drop(stmt);

        for (release_id, snapshot_json) in rows {
            let mut snapshot = serde_json::from_str::<Value>(&snapshot_json)
                .unwrap_or_else(|_| serde_json::json!({}));
            let mut changed = false;
            for field_name in ["source_version_id", "rollback_version_id"] {
                let version_id = snapshot
                    .get(field_name)
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .trim()
                    .to_owned();
                if version_id.is_empty() || module_version_ref_exists(conn, module_id, &version_id)?
                {
                    continue;
                }
                actions.push(serde_json::json!({
                    "kind": "invalid_module_version_reference",
                    "release_id": release_id,
                    "module_id": module_id,
                    "field": field_name,
                    "invalid_version_id": version_id,
                    "action": "clear_snapshot_field",
                    "apply": !dry_run,
                }));
                if !dry_run {
                    let object = snapshot
                        .as_object_mut()
                        .context("release snapshot must be a JSON object")?;
                    object.insert(field_name.to_owned(), Value::String(String::new()));
                    changed = true;
                }
            }
            if changed {
                conn.execute(
                    "UPDATE business_module_releases
                     SET snapshot_json = ?2
                     WHERE version_id = ?1",
                    params![release_id, serde_json::to_string(&snapshot)?],
                )?;
            }
        }
    }
    Ok(actions)
}

fn next_business_record_updated_at(
    conn: &Connection,
    collection: &str,
    record_id: &str,
    candidate: i64,
) -> anyhow::Result<i64> {
    let existing: Option<i64> = conn
        .query_row(
            "SELECT updated_at_ms FROM business_records WHERE collection = ?1 AND record_id = ?2",
            params![collection, record_id],
            |row| row.get(0),
        )
        .optional()?;
    Ok(match existing {
        Some(existing) if existing >= candidate => existing + 1,
        _ => candidate,
    })
}

/// Load and verify a desktop file's bytes from the RxDB chunk store (the WebRTC
/// data plane — no HTTP). Used to install an app from an uploaded `.zip`.
fn load_desktop_file_bytes(root: &Path, file_id: &str) -> anyhow::Result<Vec<u8>> {
    let doc = rxdb_desktop_file_document(root, file_id)?;
    let generation_id = doc
        .get("content_generation_id")
        .or_else(|| doc.get("generation_id"))
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_owned();
    anyhow::ensure!(
        !generation_id.is_empty(),
        "desktop file `{file_id}` has no generation id"
    );
    let content_hash = doc
        .get("content_hash")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_owned();
    let size_bytes = doc.get("size_bytes").and_then(Value::as_u64).unwrap_or(0);
    let chunks = rxdb_desktop_file_chunks(root, file_id, &generation_id)?;
    decode_verified_desktop_file_or_equivalent(
        root,
        file_id,
        &generation_id,
        size_bytes,
        &content_hash,
        chunks,
    )
}

pub(super) fn load_installed_module_manifests(
    root: &Path,
    app_root: &Path,
) -> anyhow::Result<Vec<ModuleManifest>> {
    let modules_root = app_root.join("installed-modules");
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
        if !runtime_import_catalog_evidence_allows_module(root, &manifest_value) {
            continue;
        }
        if super::customer_apps::authorize_runtime_module(root, &entry.path(), &manifest_value)
            .is_err()
        {
            continue;
        }
        let mut manifest: ModuleManifest = serde_json::from_value(manifest_value)
            .with_context(|| format!("failed to parse module manifest {}", path.display()))?;
        manifest.manifest_sha256 = hex_sha256(text.as_bytes());
        manifest.asset_revision = module_asset_revision(&entry.path())?;
        manifest.local_manifest_path = path.display().to_string();
        augment_module_manifest_file_plane(&mut manifest, &entry.path());
        backfill_local_module_icon(&mut manifest, &entry.path());
        if manifest.install_scope.trim().eq_ignore_ascii_case("sample") {
            continue;
        }
        if is_core_module(&manifest.id) {
            continue;
        }
        if manifest.entry.is_empty() {
            manifest.entry = format!("installed-modules/{}/index.html", manifest.id);
        }
        manifest.source = "installed".to_owned();
        manifest.install_scope = "installed".to_owned();
        manifest.default_installed = false;
        manifest.core = false;
        manifest.editable = true;
        manifest.deletable = true;
        manifests.push(manifest);
    }
    Ok(manifests)
}

fn runtime_import_catalog_evidence_allows_module(root: &Path, manifest: &Value) -> bool {
    if manifest.pointer("/store/generator").and_then(Value::as_str)
        != Some("business-os-app-importer")
    {
        return true;
    }
    let module_id = manifest
        .get("id")
        .and_then(Value::as_str)
        .unwrap_or_default();
    if module_id.is_empty() {
        return false;
    }
    let imports_root = root.join("runtime/business-os/app-imports");
    let mut evidence_roots = Vec::new();
    if let Some(command_id) = manifest
        .pointer("/store/import_command_id")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        let command_leaf = sanitize_business_os_workspace_component(command_id);
        evidence_roots.push(imports_root.join(command_leaf));
    } else if let Ok(entries) = fs::read_dir(&imports_root) {
        evidence_roots.extend(entries.flatten().take(512).map(|entry| entry.path()));
    }
    evidence_roots.into_iter().any(|evidence_root| {
        let passed = fs::read(evidence_root.join("smoke-result.json"))
            .ok()
            .and_then(|bytes| serde_json::from_slice::<Value>(&bytes).ok())
            .is_some_and(|evidence| {
                evidence.get("status").and_then(Value::as_str) == Some("passed")
                    && evidence.get("module_id").and_then(Value::as_str) == Some(module_id)
            });
        if passed {
            return true;
        }
        fs::read(evidence_root.join("smoke-in-progress.json"))
            .ok()
            .and_then(|bytes| serde_json::from_slice::<Value>(&bytes).ok())
            .is_some_and(|evidence| {
                evidence.get("status").and_then(Value::as_str) == Some("running")
                    && evidence.get("module_id").and_then(Value::as_str) == Some(module_id)
                    && evidence
                        .get("expires_at_ms")
                        .and_then(Value::as_u64)
                        .is_some_and(|expires_at_ms| expires_at_ms >= now_ms() as u64)
            })
    })
}

pub(super) fn normalize_catalog_installed_manifest(
    manifest: &mut Value,
    module_id: &str,
    module_dir: &Path,
) -> anyhow::Result<()> {
    let raw_version = manifest
        .get("version")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .trim()
        .trim_start_matches('v');
    let mut parts = raw_version.split('.');
    let major = parts.next().and_then(|part| part.parse::<u64>().ok());
    let minor = parts
        .next()
        .map(|part| part.parse::<u64>().ok())
        .unwrap_or(Some(0));
    let patch = parts
        .next()
        .map(|part| part.parse::<u64>().ok())
        .unwrap_or(Some(0));
    anyhow::ensure!(
        parts.next().is_none() && major.is_some() && minor.is_some() && patch.is_some(),
        "catalog module `{module_id}` has invalid version `{raw_version}`"
    );
    let version = format!(
        "{}.{}.{}",
        major.unwrap_or(0),
        minor.unwrap_or(0),
        patch.unwrap_or(0)
    );
    anyhow::ensure!(
        version != "0.0.0",
        "catalog module `{module_id}` must not use version 0.0.0"
    );

    manifest["version"] = Value::String(version);
    manifest["entry"] = Value::String(format!("installed-modules/{module_id}/index.html"));
    manifest["install_scope"] = Value::String("installed".to_owned());
    manifest["default_installed"] = Value::Bool(false);
    if !manifest.get("store").is_some_and(Value::is_object) {
        manifest["store"] = serde_json::json!({});
    }
    manifest["store"]["source_path"] = Value::String(format!("installed-modules/{module_id}"));
    manifest["store"]["distribution"] = Value::String("ctox-runtime-installed-module".to_owned());
    manifest["store"]["installable"] = Value::Bool(false);
    ensure_local_icon_manifest_value(manifest, module_dir);
    if let Some(layout) = manifest.get_mut("layout").and_then(Value::as_object_mut) {
        layout.remove("icon_svg");
    }
    if let Some(object) = manifest.as_object_mut() {
        for key in [
            "icon_svg",
            "iconSvg",
            "icon_path",
            "iconPath",
            "icon_url",
            "iconUrl",
        ] {
            object.remove(key);
        }
        if object.get("source").and_then(Value::as_str) == Some("local") {
            object.remove("source");
        }
    }
    Ok(())
}

fn install_template_module(
    root: &Path,
    source_app_root: &Path,
    installed_app_root: &Path,
    request: ModuleInstallTemplateRequest,
) -> anyhow::Result<ModuleManifest> {
    let template_id = source_sanitize_slug(&request.template_id);
    anyhow::ensure!(!template_id.is_empty(), "template_id is required");
    let template_path = source_app_root
        .join("template-store")
        .join(&template_id)
        .join("template.json");
    let text = fs::read_to_string(&template_path).with_context(|| {
        format!(
            "failed to read template manifest {}",
            template_path.display()
        )
    })?;
    let template: TemplateManifest = serde_json::from_str(&text).with_context(|| {
        format!(
            "failed to parse template manifest {}",
            template_path.display()
        )
    })?;
    let starter_archetype = source_sanitize_slug(&template.starter_archetype);
    let source_module = source_sanitize_slug(if template.source_module.is_empty() {
        &template.id
    } else {
        &template.source_module
    });
    let source = source_app_root.join("modules").join(&source_module);
    if starter_archetype.is_empty() && !source.join("module.json").is_file() {
        anyhow::bail!("template source module `{source_module}` is missing");
    }
    let requested_id = source_sanitize_slug(if request.module_id.trim().is_empty() {
        if request.title.trim().is_empty() {
            &template.id
        } else {
            &request.title
        }
    } else {
        &request.module_id
    });
    let module_id = unique_module_id(installed_app_root, &requested_id);
    let module_title = if request.title.trim().is_empty() {
        if template.default_title.trim().is_empty() {
            template.title.clone()
        } else {
            template.default_title.clone()
        }
    } else {
        request.title.trim().to_owned()
    };
    let target = installed_app_root
        .join("installed-modules")
        .join(&module_id);
    let staging =
        installed_app_root.join(format!(".module-template-{module_id}-{}", Uuid::new_v4()));
    if starter_archetype.is_empty() {
        copy_dir_recursive(&source, &staging)?;
    } else {
        fs::create_dir_all(&staging)
            .with_context(|| format!("failed to create {}", staging.display()))?;
        let command = BusinessCommand {
            origin: CommandOrigin::TrustedLocal,
            id: None,
            module: "app-store".to_owned(),
            command_type: "ctox.business_os.app.create".to_owned(),
            record_id: Some(module_id.clone()),
            payload: serde_json::json!({
                "module_id": module_id.clone(),
                "title": module_title.clone(),
                "description": template.description.clone(),
                "category": template.category.clone(),
                "version": "0.1.0",
                "archetype": starter_archetype.clone(),
                "instruction": format!("Installed from canonical template {}", template.id),
                "install_target": "runtime-installed-module",
                "target": "app",
                "mode": "app"
            }),
            client_context: serde_json::json!({
                "source": "business-os-template-store",
                "template_id": template.id.clone(),
                "archetype": starter_archetype.clone()
            }),
        };
        let materialized = materialize_runtime_app_starter_artifacts_at(
            &command,
            &module_id,
            RuntimeAppStarterAction::Create,
            &staging,
        )?;
        anyhow::ensure!(
            materialized.should_validate,
            "canonical template starter refused to materialize `{module_id}`"
        );
    }

    let manifest_path = staging.join("module.json");
    let mut manifest_value: Value = serde_json::from_str(
        &fs::read_to_string(&manifest_path)
            .with_context(|| format!("failed to read {}", manifest_path.display()))?,
    )?;
    manifest_value["id"] = Value::String(module_id.clone());
    manifest_value["title"] = Value::String(module_title);
    manifest_value["template_id"] = Value::String(template.id);
    // Link the installed instance back to the catalog module it was copied from
    // so the catalog/update diff can detect a newer upstream bundle later.
    if starter_archetype.is_empty() {
        manifest_value["source_module_id"] = Value::String(source_module.clone());
        manifest_value["app_source"] = serde_json::json!({
            "kind": "catalog",
            "module_id": source_module.clone(),
            "verified": true,
            "trust_model": "ctox-first-party-source"
        });
        normalize_catalog_installed_manifest(&mut manifest_value, &module_id, &staging)?;
    } else {
        manifest_value["archetype"] = Value::String(starter_archetype.clone());
        manifest_value["source_module_id"] = Value::String(String::new());
        ensure_local_icon_manifest_value(&mut manifest_value, &staging);
    }
    fs::write(&manifest_path, serde_json::to_vec_pretty(&manifest_value)?)
        .with_context(|| format!("failed to write {}", manifest_path.display()))?;

    let activation = (|| -> anyhow::Result<()> {
        super::customer_apps::authorize_runtime_module(root, &staging, &manifest_value)?;
        // CTOX Marketplace apps are authored against the catalog contract and
        // then rewritten to an installed runtime path. Validate them with the
        // dedicated catalog-installed profile; the stricter generated-app
        // profile intentionally rejects shared first-party app structures.
        validate_staged_catalog_module(root, &module_id, &staging)?;
        let backup =
            installed_app_root.join(format!(".module-backup-{module_id}-{}", Uuid::new_v4()));
        activate_staged_module_directory(&staging, &target, &backup)
    })();
    if let Err(error) = activation {
        let _ = fs::remove_dir_all(&staging);
        return Err(error);
    }

    let mut manifest: ModuleManifest = serde_json::from_value(manifest_value)?;
    manifest.source = "installed".to_owned();
    manifest.install_scope = "installed".to_owned();
    manifest.default_installed = false;
    manifest.core = false;
    manifest.editable = true;
    manifest.deletable = true;
    Ok(manifest)
}

pub(super) fn delete_installed_module(
    app_root: &Path,
    root: &Path,
    request: ModuleDeleteRequest,
) -> anyhow::Result<()> {
    let module_id = source_sanitize_slug(&request.module_id);
    anyhow::ensure!(!module_id.is_empty(), "module id is required");
    if is_core_module(&module_id) {
        anyhow::bail!("core modules cannot be deleted");
    }
    let target = app_root.join("installed-modules").join(&module_id);
    if !target.is_dir() {
        anyhow::bail!("installed module not found: {module_id}");
    }

    let layout_path = root.join("runtime").join("business-os-module-layout.json");
    let original_layout_bytes =
        if layout_path.is_file() {
            Some(fs::read(&layout_path).with_context(|| {
                format!("failed to read module layout {}", layout_path.display())
            })?)
        } else {
            None
        };
    let mut layout = load_module_layout(root)?;
    remove_module_from_layout_value(&mut layout, &module_id);

    let staged = app_root
        .join("installed-modules")
        .join(format!(".module-delete-{module_id}-{}", Uuid::new_v4()));
    anyhow::ensure!(
        !staged.exists(),
        "module deletion stage already exists: {}",
        staged.display()
    );

    let mut conn = open_store(root)?;
    let tx = conn.transaction()?;
    fs::rename(&target, &staged).with_context(|| {
        format!(
            "failed to stage module directory {} for deletion",
            target.display()
        )
    })?;

    let deletion = (|| -> anyhow::Result<()> {
        save_module_layout(root, &layout)?;
        tx.execute(
            "DELETE FROM business_permission_grants
             WHERE scope_type = 'module' AND scope_id = ?1",
            params![module_id],
        )?;
        tx.execute(
            "DELETE FROM business_module_acl WHERE module_id = ?1",
            params![module_id],
        )?;
        tx.execute(
            "DELETE FROM business_module_releases WHERE module_id = ?1",
            params![module_id],
        )?;
        tx.execute(
            "DELETE FROM business_module_versions WHERE module_id = ?1",
            params![module_id],
        )?;
        tx.commit()?;
        Ok(())
    })();

    if let Err(error) = deletion {
        let layout_restore = match original_layout_bytes {
            Some(bytes) => fs::write(&layout_path, bytes).with_context(|| {
                format!("failed to restore module layout {}", layout_path.display())
            }),
            None if layout_path.exists() => fs::remove_file(&layout_path).with_context(|| {
                format!(
                    "failed to remove newly created module layout {}",
                    layout_path.display()
                )
            }),
            None => Ok(()),
        };
        let module_restore = fs::rename(&staged, &target).with_context(|| {
            format!(
                "failed to restore staged module directory {}",
                target.display()
            )
        });
        if let Err(restore_error) = layout_restore.and(module_restore) {
            return Err(anyhow::anyhow!(
                "module deletion failed ({error:#}); rollback also failed ({restore_error:#}); staged module remains at {}",
                staged.display()
            ));
        }
        return Err(error.context("module deletion rolled back"));
    }

    fs::remove_dir_all(&staged)
        .with_context(|| format!("failed to remove deleted module stage {}", staged.display()))?;
    Ok(())
}

fn unique_module_id(app_root: &Path, requested_id: &str) -> String {
    let base = if requested_id.is_empty() {
        "module".to_owned()
    } else if is_core_module(requested_id) {
        format!("{requested_id}-copy")
    } else {
        requested_id.to_owned()
    };
    let installed_root = app_root.join("installed-modules");
    if !installed_root.join(&base).exists() {
        return base;
    }
    for index in 2..1000 {
        let candidate = format!("{base}-{index}");
        if !installed_root.join(&candidate).exists() {
            return candidate;
        }
    }
    format!("{base}-{}", Uuid::new_v4())
}

pub(super) fn module_install_scope(manifest: &ModuleManifest) -> String {
    let explicit = manifest.install_scope.trim().to_ascii_lowercase();
    // System membership is defined only by the embedded canonical manifest.
    // A module cannot promote itself to system status through module.json.
    if is_core_module(&manifest.id) {
        return "core".to_owned();
    }
    // `starter` is a retired compatibility value. Treat old manifests as
    // marketplace apps instead of silently installing them on every instance.
    if matches!(explicit.as_str(), "core" | "starter") {
        return "store".to_owned();
    }
    if matches!(
        explicit.as_str(),
        "store" | "internal" | "installed" | "local"
    ) {
        return explicit;
    }
    "store".to_owned()
}

pub(super) fn module_ships_on_first_install(scope: &str) -> bool {
    matches!(scope, "core" | "internal")
}

pub fn rollback_module_source_snapshot(
    root: &Path,
    request: ModuleSourceRollbackSnapshotRequest,
) -> anyhow::Result<Value> {
    let module_id = source_sanitize_slug(&request.module_id);
    anyhow::ensure!(!module_id.is_empty(), "module_id is required");
    let snapshot_id = request.snapshot_id.trim();
    anyhow::ensure!(!snapshot_id.is_empty(), "snapshot_id is required");

    let snapshot_root = root
        .join("runtime")
        .join("business-os-source-snapshots")
        .join(&module_id);

    let metadata_path = snapshot_root.join(format!("{}.json", snapshot_id));
    anyhow::ensure!(metadata_path.is_file(), "snapshot metadata not found");

    let source_path = snapshot_root.join(format!("{}.source", snapshot_id));
    anyhow::ensure!(source_path.is_file(), "snapshot source file not found");

    let metadata_content = fs::read_to_string(&metadata_path)?;
    let metadata: Value = serde_json::from_str(&metadata_content)?;
    let rel_path = metadata
        .get("path")
        .and_then(Value::as_str)
        .context("invalid snapshot metadata: path missing")?;

    let source_content = fs::read_to_string(&source_path)?;

    let mutation = ModuleSourceSaveMutation {
        module_id: module_id.clone(),
        path: rel_path.to_string(),
        content: source_content,
    };

    let outcome = save_module_source_record(root, mutation)?;
    Ok(outcome)
}

pub(super) fn sync_module_version_records(
    conn: &Connection,
    module_id: &str,
    updated_at_ms: i64,
) -> anyhow::Result<()> {
    let mut stmt = conn.prepare(
        "SELECT version_id FROM business_module_versions WHERE module_id = ?1 ORDER BY seq DESC",
    )?;
    let ids = stmt
        .query_map(params![module_id], |row| row.get::<_, String>(0))?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    drop(stmt);
    for id in ids {
        let mut doc = version_summary_row(conn, &id)?;
        let rec_updated =
            next_business_record_updated_at(conn, "business_module_versions", &id, updated_at_ms)?;
        if let Some(object) = doc.as_object_mut() {
            object.insert("id".to_string(), Value::String(id.clone()));
            object.insert("updated_at_ms".to_string(), Value::from(rec_updated));
        }
        upsert_business_record(conn, "business_module_versions", &id, rec_updated, doc)?;
    }
    sync_module_commit_records(conn, module_id, updated_at_ms)?;
    Ok(())
}

/// Project each SEALED module version into an immutable, content-addressed
/// `business_module_commits` doc — the replicated, git-style source history.
/// Unsealed working rows are NOT commits (they are the working tree). The id is
/// content-addressed over the sealed row's immutable facts, so a re-run upserts
/// byte-identical docs (idempotent) and this also backfills existing history.
/// `parent_id` links each sealed commit to the previous sealed one in seq order,
/// making the otherwise-linear log DAG-ready. Bodies are NOT inlined here (the
/// file_manifest carries per-file blob pointers; bodies stream as demand chunks).
fn sync_module_commit_records(
    conn: &Connection,
    module_id: &str,
    updated_at_ms: i64,
) -> anyhow::Result<()> {
    let mut stmt = conn.prepare(
        "SELECT version_id, seq, origin, label, bundle_sha256, files_json,
                created_by, created_at_ms
         FROM business_module_versions
         WHERE module_id = ?1 AND sealed = 1 ORDER BY seq ASC",
    )?;
    let rows = stmt
        .query_map(params![module_id], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, i64>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
                row.get::<_, String>(4)?,
                row.get::<_, String>(5)?,
                row.get::<_, String>(6)?,
                row.get::<_, i64>(7)?,
            ))
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    drop(stmt);

    let mut parent_id = String::new();
    for (version_id, seq, origin, label, bundle_sha256, files_json, created_by, created_at_ms) in
        rows
    {
        let files: Vec<Value> = serde_json::from_str(&files_json).unwrap_or_default();
        let file_manifest: Vec<Value> = files
            .iter()
            .map(|file| {
                let sha = file
                    .get("sha256")
                    .and_then(Value::as_str)
                    .unwrap_or_default();
                serde_json::json!({
                    "path": file.get("path").and_then(Value::as_str).unwrap_or_default(),
                    "sha256": sha,
                    // Content-addressed blob id == the file's content hash, so an
                    // unchanged file shares one blob across every commit (dedup).
                    "blob_id": sha,
                    "size_bytes": file
                        .get("content")
                        .and_then(Value::as_str)
                        .map(str::len)
                        .unwrap_or(0),
                })
            })
            .collect();
        let message = if label.trim().is_empty() {
            origin.clone()
        } else {
            label.clone()
        };
        let commit_id = format!(
            "commit_{}",
            hex_sha256(
                format!(
                    "{module_id}\n{parent_id}\n{seq}\n{bundle_sha256}\n{origin}\n{message}\n{created_by}\n{created_at_ms}"
                )
                .as_bytes()
            )
        );
        let rec_updated = next_business_record_updated_at(
            conn,
            "business_module_commits",
            &commit_id,
            updated_at_ms,
        )?;
        let doc = serde_json::json!({
            "id": commit_id,
            "module_id": module_id,
            "seq": seq,
            "parent_id": parent_id,
            "bundle_sha256": bundle_sha256,
            "message": message,
            "origin": origin,
            "label": label,
            "author": created_by,
            "authored_at_ms": created_at_ms,
            "sealed": true,
            "file_manifest": file_manifest,
            "version_id": version_id,
            "created_at_ms": created_at_ms,
            "updated_at_ms": rec_updated,
        });
        upsert_business_record(
            conn,
            "business_module_commits",
            &commit_id,
            rec_updated,
            doc,
        )?;
        parent_id = commit_id;
    }
    Ok(())
}

/// Capture a full-bundle restore point for a module.
///
/// `origin == "edit"` coalesces into the single open working version (so a burst
/// of agent edits is one rolling restore point); any other origin is a sealed
/// boundary (install, manual_release, rollback, creator_deploy).
pub(super) fn record_module_version(
    root: &Path,
    app_root: &Path,
    module_id: &str,
    origin: &str,
    label: &str,
    created_by: &str,
) -> anyhow::Result<Option<Value>> {
    let conn = open_store(root)?;
    record_module_version_with_conn(&conn, app_root, module_id, origin, label, created_by)
}

pub(super) fn record_module_version_with_conn(
    conn: &Connection,
    app_root: &Path,
    module_id: &str,
    origin: &str,
    label: &str,
    created_by: &str,
) -> anyhow::Result<Option<Value>> {
    let module_id = normalize_module_id_for_bundle_snapshot(module_id)?;
    if module_id.is_empty() {
        return Ok(None);
    }
    let bundle = compute_module_bundle(app_root, &module_id)?;
    record_module_bundle_version(conn, &module_id, bundle, origin, label, created_by)
}

/// Snapshot exactly the directory selected for a source write, without resolving
/// its id again through another bundled/installed namespace.
pub(super) fn record_module_version_at(
    root: &Path,
    module_root: &Path,
    module_id: &str,
) -> anyhow::Result<Option<Value>> {
    let conn = open_store(root)?;
    let bundle = super::store::compute_module_bundle_at(module_root, module_id)?;
    record_module_bundle_version(&conn, module_id, bundle, "edit", "", "")
}

fn record_module_bundle_version(
    conn: &Connection,
    module_id: &str,
    bundle: super::store::ModuleBundle,
    origin: &str,
    label: &str,
    created_by: &str,
) -> anyhow::Result<Option<Value>> {
    let now = now_ms() as i64;
    let is_boundary = origin != "edit";

    let latest: Option<(String, String, i64)> = conn
        .query_row(
            "SELECT version_id, bundle_sha256, sealed FROM business_module_versions
             WHERE module_id = ?1 ORDER BY seq DESC LIMIT 1",
            params![module_id],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .optional()?;

    if !is_boundary {
        if let Some((latest_id, latest_sha, latest_sealed)) = latest.as_ref() {
            if latest_sha == &bundle.sha256 {
                return Ok(None);
            }
            if *latest_sealed == 0 {
                conn.execute(
                    "UPDATE business_module_versions
                     SET bundle_sha256 = ?2, files_json = ?3, updated_at_ms = ?4,
                         label = CASE WHEN ?5 <> '' THEN ?5 ELSE label END
                     WHERE version_id = ?1",
                    params![
                        latest_id,
                        bundle.sha256,
                        serde_json::to_string(&bundle.files)?,
                        now,
                        label
                    ],
                )?;
                sync_module_version_records(&conn, &module_id, now)?;
                return Ok(Some(version_summary_row(&conn, latest_id)?));
            }
        }
    } else {
        conn.execute(
            "UPDATE business_module_versions SET sealed = 1, updated_at_ms = ?2
             WHERE module_id = ?1 AND sealed = 0",
            params![module_id, now],
        )?;
        if origin == "install" {
            if let Some((latest_id, latest_sha, _)) = latest.as_ref() {
                if latest_sha == &bundle.sha256 {
                    sync_module_version_records(&conn, &module_id, now)?;
                    return Ok(Some(version_summary_row(&conn, latest_id)?));
                }
            }
        }
    }

    let next_seq: i64 = conn.query_row(
        "SELECT COALESCE(MAX(seq), 0) + 1 FROM business_module_versions WHERE module_id = ?1",
        params![module_id],
        |row| row.get(0),
    )?;
    let version_id = format!("modver_{}_{}_{}", module_id, next_seq, Uuid::new_v4());
    let sealed = i64::from(is_boundary);
    conn.execute(
        "INSERT INTO business_module_versions
            (version_id, module_id, seq, origin, label, bundle_sha256, files_json,
             sealed, created_by, created_at_ms, updated_at_ms)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?10)",
        params![
            version_id,
            module_id,
            next_seq,
            origin,
            label,
            bundle.sha256,
            serde_json::to_string(&bundle.files)?,
            sealed,
            created_by,
            now
        ],
    )?;
    sync_module_version_records(&conn, &module_id, now)?;
    Ok(Some(version_summary_row(&conn, &version_id)?))
}

fn normalize_module_id_for_bundle_snapshot(module_id: &str) -> anyhow::Result<String> {
    let module_id = module_id.trim();
    if module_id.is_empty() {
        return Ok(String::new());
    }
    anyhow::ensure!(
        module_id != "."
            && module_id != ".."
            && !module_id.contains('/')
            && !module_id.contains('\\')
            && !module_id.contains('\0'),
        "invalid module id for bundle snapshot: {module_id}"
    );
    Ok(module_id.to_owned())
}

pub fn list_module_versions(
    root: &Path,
    request: ModuleVersionListRequest,
) -> anyhow::Result<Value> {
    let module_id = source_sanitize_slug(&request.module_id);
    anyhow::ensure!(!module_id.is_empty(), "module_id is required");
    let conn = open_store(root)?;
    let mut stmt = conn.prepare(
        "SELECT version_id FROM business_module_versions WHERE module_id = ?1 ORDER BY seq DESC",
    )?;
    let ids = stmt
        .query_map(params![module_id], |row| row.get::<_, String>(0))?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    drop(stmt);
    let mut versions = Vec::with_capacity(ids.len());
    for id in &ids {
        versions.push(version_summary_row(&conn, id)?);
    }
    Ok(serde_json::json!({
        "ok": true,
        "module_id": module_id,
        "versions": versions
    }))
}

/// Read the declared version string of a catalog (`modules/<id>`) module.
pub(super) fn catalog_module_version(app_root: &Path, module_id: &str) -> String {
    let path = app_root.join("modules").join(module_id).join("module.json");
    fs::read_to_string(&path)
        .ok()
        .and_then(|text| serde_json::from_str::<Value>(&text).ok())
        .and_then(|value| {
            value
                .get("version")
                .and_then(Value::as_str)
                .map(|version| version.trim().to_owned())
        })
        .unwrap_or_default()
}

pub fn rollback_module_to_version(
    root: &Path,
    app_root: &Path,
    session: &BusinessOsSession,
    request: ModuleVersionRollbackRequest,
) -> anyhow::Result<Value> {
    let module_id = source_sanitize_slug(&request.module_id);
    let version_id = request.version_id.trim().to_string();
    anyhow::ensure!(!module_id.is_empty(), "module_id is required");
    anyhow::ensure!(!version_id.is_empty(), "version_id is required");
    let rollback_decision = module_policy_decision(
        root,
        session,
        BusinessOsPermission::AppsRollback,
        &module_id,
    )?;
    anyhow::ensure!(
        rollback_decision.allowed,
        "{}",
        rollback_decision.display_reason
    );

    let files_json: String = {
        let conn = open_store(root)?;
        conn.query_row(
            "SELECT files_json FROM business_module_versions
             WHERE module_id = ?1 AND version_id = ?2",
            params![module_id, version_id],
            |row| row.get(0),
        )
        .optional()?
        .context("version not found for module")?
    };
    let target_files: Vec<Value> = serde_json::from_str(&files_json).unwrap_or_default();
    let (module_root, source_app_root) =
        resolve_module_source_root_for_root(root, app_root, &module_id)?;
    let module_parent = module_root
        .parent()
        .context("module source root has no parent")?;
    let staging = module_parent.join(format!(".rollback-stage-{module_id}-{}", Uuid::new_v4()));
    let backup = module_parent.join(format!(".rollback-backup-{module_id}-{}", Uuid::new_v4()));

    let current = compute_module_bundle(&source_app_root, &module_id)?;
    let mut target_paths = std::collections::BTreeSet::new();
    let mut restored = 0usize;
    let mut removed = 0usize;
    let stage_result = (|| -> anyhow::Result<()> {
        // Preserve non-source assets such as operator-approved PNG icons while
        // replacing the complete versioned text bundle in an isolated sibling.
        copy_dir_recursive(&module_root, &staging)?;
        for file in &target_files {
            let path = file.get("path").and_then(Value::as_str).unwrap_or_default();
            let content = file
                .get("content")
                .and_then(Value::as_str)
                .unwrap_or_default();
            if path.is_empty() {
                continue;
            }
            let rel = normalize_source_relative_path(path)?;
            anyhow::ensure!(
                is_allowed_source_path(&rel),
                "version contains a disallowed source path: {}",
                rel.display()
            );
            target_paths.insert(rel.to_string_lossy().to_string());
            let target = staging.join(&rel);
            if let Some(parent) = target.parent() {
                fs::create_dir_all(parent)?;
            }
            fs::write(&target, content.as_bytes())
                .with_context(|| format!("failed to stage {}", target.display()))?;
            restored += 1;
        }

        for file in &current.files {
            let path = file.get("path").and_then(Value::as_str).unwrap_or_default();
            if path.is_empty() {
                continue;
            }
            let rel = normalize_source_relative_path(path)?;
            let rel_display = rel.to_string_lossy().to_string();
            if target_paths.contains(&rel_display) || !is_allowed_source_path(&rel) {
                continue;
            }
            let live = module_root.join(&rel);
            if let Ok(content) = fs::read_to_string(&live) {
                let previous_sha = hex_sha256(content.as_bytes());
                let _ = write_module_source_snapshot(
                    root,
                    &module_id,
                    &rel,
                    &content,
                    Some(&previous_sha),
                );
            }
            let staged = staging.join(&rel);
            if staged.is_file() {
                fs::remove_file(&staged)
                    .with_context(|| format!("failed to remove staged {}", staged.display()))?;
                removed += 1;
            }
        }

        ensure_rollback_collection_schema_compatible(&module_root, &staging)?;
        validate_staged_catalog_module(root, &module_id, &staging)?;
        activate_staged_module_directory(&staging, &module_root, &backup)
    })();
    if let Err(error) = stage_result {
        let _ = fs::remove_dir_all(&staging);
        return Err(error.context("module rollback stage/validate/atomic-swap failed"));
    }

    let created_by = session_user_id(session).unwrap_or("").to_string();
    record_module_version(
        root,
        &source_app_root,
        &module_id,
        "rollback",
        &format!("Rolled back to {version_id}"),
        &created_by,
    )?;
    sync_module_catalog_projection_now(root)?;

    Ok(serde_json::json!({
        "ok": true,
        "module_id": module_id,
        "rolled_back_to": version_id,
        "restored_files": restored,
        "removed_files": removed
    }))
}

fn ensure_rollback_collection_schema_compatible(
    live_module: &Path,
    staged_module: &Path,
) -> anyhow::Result<()> {
    let live_path = live_module.join("collections.schema.json");
    let staged_path = staged_module.join("collections.schema.json");
    match (live_path.is_file(), staged_path.is_file()) {
        (false, false) => return Ok(()),
        (true, true) => {}
        _ => anyhow::bail!(
            "rollback rejected: collections.schema.json presence differs from the active module"
        ),
    }
    let live: Value = serde_json::from_str(
        &fs::read_to_string(&live_path)
            .with_context(|| format!("failed to read {}", live_path.display()))?,
    )
    .context("active collection schema is invalid")?;
    let staged: Value = serde_json::from_str(
        &fs::read_to_string(&staged_path)
            .with_context(|| format!("failed to read {}", staged_path.display()))?,
    )
    .context("staged collection schema is invalid")?;
    anyhow::ensure!(
        live.get("collections") == staged.get("collections"),
        "rollback rejected: target version has an incompatible collection schema"
    );
    Ok(())
}

/// Download an archive over the SSRF-guarded agent and read it into memory.
fn fetch_archive_bytes(url: &str) -> anyhow::Result<Vec<u8>> {
    let response = super::importer::fetch_url_guarded(url)
        .with_context(|| format!("Failed to download module archive from {url}"))?;
    let mut bytes = Vec::new();
    response
        .into_reader()
        .read_to_end(&mut bytes)
        .context("Failed to read module archive stream")?;
    Ok(bytes)
}

/// Read the `app_source` descriptor from an installed module's manifest.
pub(super) fn installed_module_app_source(
    installed_app_root: &Path,
    module_id: &str,
) -> Option<Value> {
    let path = installed_app_root
        .join("installed-modules")
        .join(module_id)
        .join("module.json");
    let manifest: Value = serde_json::from_str(&fs::read_to_string(path).ok()?).ok()?;
    manifest.get("app_source").cloned().filter(Value::is_object)
}

pub fn install_app_module(
    root: &Path,
    app_root: &Path,
    session: &BusinessOsSession,
    request: AppStoreInstallRequest,
) -> anyhow::Result<Value> {
    let module_id = source_sanitize_slug(&request.module_id);
    anyhow::ensure!(!module_id.is_empty(), "module_id is required");
    anyhow::ensure!(
        module_policy_decision(root, session, BusinessOsPermission::AppsInstall, &module_id)?
            .allowed,
        "chef or admin role required to install modules"
    );

    // Resolve the install source. GitHub repos and legacy download URLs are
    // fetched through the SSRF-guarded agent (PublicOnlyResolver) so an
    // attacker-supplied source can never reach loopback/metadata/private hosts.
    // Zip uploads are read from the RxDB chunk store (WebRTC data plane) — never
    // over HTTP.
    let source_kind = if !request.source_kind.trim().is_empty() {
        request.source_kind.trim().to_ascii_lowercase()
    } else if !request.repo.trim().is_empty() {
        "github".to_owned()
    } else if !request.file_id.trim().is_empty() {
        "zip".to_owned()
    } else {
        "url".to_owned()
    };
    let (effective_source_path, app_source_base, zip_bytes) = match source_kind.as_str() {
        "github" => {
            let repo = validate_github_repo(&request.repo)?;
            let git_ref = sanitize_git_ref(&request.git_ref)?;
            let subpath = source_relative_subpath(&request.subpath)?;
            let url = github_archive_url(&repo, &git_ref);
            let descriptor = serde_json::json!({
                "kind": "github",
                "repo": repo.clone(),
                "ref": git_ref,
                "subpath": subpath,
                "verified": repo == "metric-space-ai/ctox",
                "trust_model": if repo == "metric-space-ai/ctox" { "ctox-first-party-source" } else { "untrusted-third-party" },
            });
            (subpath, descriptor, fetch_archive_bytes(&url)?)
        }
        "zip" => {
            let file_id = request.file_id.trim();
            anyhow::ensure!(!file_id.is_empty(), "file_id is required for a zip install");
            let subpath = source_relative_subpath(&request.subpath)?;
            let bytes = load_desktop_file_bytes(root, file_id)?;
            (
                subpath,
                serde_json::json!({ "kind": "zip", "file_id": file_id }),
                bytes,
            )
        }
        "url" | "" => {
            anyhow::ensure!(
                !request.download_url.trim().is_empty(),
                "download_url is required for a url install"
            );
            let url = request.download_url.trim().to_owned();
            (
                request.source_path.clone(),
                serde_json::json!({ "kind": "url", "url": url.clone() }),
                fetch_archive_bytes(&url)?,
            )
        }
        other => anyhow::bail!("unsupported install source kind '{other}'"),
    };

    // Extract ZIP to a temporary directory
    let temp_dir = std::env::temp_dir().join(format!("ctox-app-install-{}", Uuid::new_v4()));
    fs::create_dir_all(&temp_dir)
        .with_context(|| format!("Failed to create temp extract dir {}", temp_dir.display()))?;

    let cursor = std::io::Cursor::new(zip_bytes);
    let mut archive =
        zip::ZipArchive::new(cursor).context("Failed to open downloaded archive as a zip file")?;

    for i in 0..archive.len() {
        let mut file = archive
            .by_index(i)
            .context("Failed to read file from zip archive")?;
        let filepath = match file.enclosed_name() {
            Some(path) => path.to_owned(),
            None => continue,
        };
        let outpath = temp_dir.join(filepath);
        if file.is_dir() {
            fs::create_dir_all(&outpath)?;
        } else {
            if let Some(p) = outpath.parent() {
                if !p.exists() {
                    fs::create_dir_all(&p)?;
                }
            }
            let mut outfile = fs::File::create(&outpath)?;
            std::io::copy(&mut file, &mut outfile)?;
        }
    }

    // Search recursively for the directory containing module.json
    let found_dir =
        find_module_json_dir_for_install(&temp_dir, &module_id, &effective_source_path)?
            .with_context(|| {
                format!(
                    "No module.json for module '{}' found in the downloaded repository archive",
                    module_id
                )
            })?;

    // Read and parse module.json to ensure it's a valid manifest
    let manifest_path = found_dir.join("module.json");
    let manifest_content = fs::read_to_string(&manifest_path)
        .context("Failed to read module.json in downloaded archive")?;
    let mut manifest: Value = serde_json::from_str(&manifest_content)
        .context("Downloaded module.json is not a valid JSON")?;

    // Ensure the ID matches (or just use the ID in the manifest to create destination)
    let manifest_id = manifest
        .get("id")
        .and_then(Value::as_str)
        .context("Downloaded module.json is missing 'id' field")?;
    let sanitized_manifest_id = source_sanitize_slug(manifest_id);
    anyhow::ensure!(
        sanitized_manifest_id == module_id,
        "Module ID in module.json ('{}') does not match request module ID ('{}')",
        sanitized_manifest_id,
        module_id
    );

    // Prepare the complete replacement beside installed-modules and activate it
    // with the same stage/backup/restore invariant as catalog updates. The live
    // app is never deleted before the replacement is known to be complete.
    let installed_root = app_root.join("installed-modules");
    fs::create_dir_all(&installed_root)?;
    let dest_dir = installed_root.join(&module_id);
    let staging = app_root.join(format!(".module-install-{module_id}-{}", Uuid::new_v4()));
    let backup = app_root.join(format!(".module-backup-{module_id}-{}", Uuid::new_v4()));
    let install_result = (|| -> anyhow::Result<()> {
        copy_dir_recursive(&found_dir, &staging)
            .context("Failed to stage extracted module files")?;

        // Stamp source provenance. Same-origin runtime modules are executable
        // code, so untrusted third-party archives remain blocked until an
        // isolated sandbox exists. The first-party repository is an explicit
        // trust root and the recorded module bundle hash binds the installed
        // revision for later update/release evidence.
        let app_source = app_source_base;
        let trusted_source = app_source
            .get("verified")
            .and_then(Value::as_bool)
            .unwrap_or(false);
        anyhow::ensure!(
            trusted_source,
            "untrusted third-party apps cannot run same-origin; install a CTOX first-party source or wait for the sandbox runtime"
        );
        manifest["app_source"] = app_source;
        normalize_catalog_installed_manifest(&mut manifest, &module_id, &staging)?;
        fs::write(
            staging.join("module.json"),
            serde_json::to_vec_pretty(&manifest)?,
        )
        .with_context(|| format!("Failed to rewrite staged manifest for {module_id}"))?;

        super::customer_apps::authorize_runtime_module(root, &staging, &manifest)?;
        validate_staged_catalog_module(root, &module_id, &staging)?;

        activate_staged_module_directory(&staging, &dest_dir, &backup)
    })();
    let _ = fs::remove_dir_all(&temp_dir);
    if let Err(error) = install_result {
        let _ = fs::remove_dir_all(&staging);
        return Err(error);
    }

    let created_by = session_user_id(session).unwrap_or("").to_string();
    record_module_version(
        root,
        app_root,
        &module_id,
        "install",
        "Installed from store",
        &created_by,
    )?;
    sync_module_catalog_projection_now(root)?;

    Ok(serde_json::json!({
        "ok": true,
        "module_id": module_id,
        "installed": true,
        "manifest": manifest
    }))
}

#[cfg(test)]
mod tests {
    use super::super::policy::BusinessOsPermission;
    use super::super::store::tests::{
        chef_session, locked_inventory_data_review, save_widget_source, seed_business_user,
        seed_module_founder_acl, seed_permission_grant, seed_test_business_os_app_root,
        test_session, write_installed_inventory_module, write_runtime_installed_inventory_module,
        write_widget_module,
    };
    use super::super::store::{
        accept_rxdb_business_command, now_ms, open_store, outbound_load_required,
        resolve_business_os_installed_app_root, ModuleDeleteRequest, ModuleReleaseRequest,
        ModuleVersionListRequest, ModuleVersionRollbackRequest,
    };
    use super::{
        compute_module_bundle, copy_dir_recursive, delete_installed_module,
        ensure_rollback_collection_schema_compatible, list_module_versions,
        module_catalog_source_id, module_policy_decision, normalize_catalog_installed_manifest,
        record_module_release, record_module_version, release_managed_shadow_source,
        rollback_module_to_version, runtime_import_catalog_evidence_allows_module,
        sync_module_version_records, validate_staged_catalog_module,
    };
    use rusqlite::{params, Connection};
    use serde_json::Value;
    use std::fs;
    use std::path::Path;
    use tempfile::tempdir;

    #[test]
    fn imported_modules_need_a_passed_or_bounded_in_progress_smoke_gate() -> anyhow::Result<()> {
        let temp = tempdir()?;
        let root = temp.path();
        let evidence_root = root.join("runtime/business-os/app-imports/cmd-import-radio");
        fs::create_dir_all(&evidence_root)?;
        let manifest = serde_json::json!({
            "id": "radio",
            "store": {
                "generator": "business-os-app-importer",
                "import_command_id": "cmd-import-radio"
            }
        });

        assert!(!runtime_import_catalog_evidence_allows_module(
            root, &manifest
        ));
        fs::write(
            evidence_root.join("smoke-in-progress.json"),
            serde_json::to_vec(&serde_json::json!({
                "module_id": "radio",
                "status": "running",
                "expires_at_ms": now_ms() + 60_000
            }))?,
        )?;
        assert!(runtime_import_catalog_evidence_allows_module(
            root, &manifest
        ));
        fs::write(
            evidence_root.join("smoke-in-progress.json"),
            serde_json::to_vec(&serde_json::json!({
                "module_id": "radio",
                "status": "running",
                "expires_at_ms": now_ms().saturating_sub(1)
            }))?,
        )?;
        assert!(!runtime_import_catalog_evidence_allows_module(
            root, &manifest
        ));
        fs::write(
            evidence_root.join("smoke-result.json"),
            serde_json::to_vec(&serde_json::json!({
                "module_id": "radio",
                "status": "passed"
            }))?,
        )?;
        assert!(runtime_import_catalog_evidence_allows_module(
            root, &manifest
        ));
        Ok(())
    }

    #[test]
    fn marketplace_buchhaltung_uses_catalog_installed_validation() -> anyhow::Result<()> {
        let repo_root = Path::new(env!("CARGO_MANIFEST_DIR"));
        let source = repo_root.join("src/apps/business-os/modules/buchhaltung");
        let temp = tempdir()?;
        let staging = temp.path().join("buchhaltung");
        copy_dir_recursive(&source, &staging)?;

        let manifest_path = staging.join("module.json");
        let mut manifest: Value = serde_json::from_slice(&fs::read(&manifest_path)?)?;

        manifest["app_source"] = serde_json::json!({
            "kind": "github",
            "repo": "metric-space-ai/ctox",
            "verified": true,
            "trust_model": "ctox-first-party-source"
        });
        normalize_catalog_installed_manifest(&mut manifest, "buchhaltung", &staging)?;
        fs::write(&manifest_path, serde_json::to_vec_pretty(&manifest)?)?;

        validate_staged_catalog_module(repo_root, "buchhaltung", &staging)
    }

    #[test]
    fn legacy_ctox_runtime_install_resolves_same_id_catalog_source_only() -> anyhow::Result<()> {
        let temp = tempdir()?;
        let app_root = temp.path().join("business-os");
        let catalog_dir = app_root.join("modules/research");
        fs::create_dir_all(&catalog_dir)?;
        fs::write(
            catalog_dir.join("module.json"),
            r#"{"id":"research","title":"Research","developer":"CTOX","install_scope":"internal"}"#,
        )?;

        let legacy = serde_json::json!({
            "id": "research",
            "developer": "CTOX",
            "default_installed": true,
            "source_module_id": "research",
            "store": {
                "distribution": "ctox-runtime-installed-module",
                "source_path": "installed-modules/research",
                "installable": false
            }
        });
        assert_eq!(
            module_catalog_source_id(&app_root, &legacy).as_deref(),
            Some("research")
        );
        assert_eq!(
            release_managed_shadow_source(&app_root, &legacy)?.as_deref(),
            Some("research")
        );

        let bespoke = serde_json::json!({
            "id": "research",
            "developer": "Customer",
            "default_installed": false,
            "source_module_id": "research",
            "store": {
                "distribution": "custom",
                "source_path": "installed-modules/research",
                "installable": false
            }
        });
        assert_eq!(
            module_catalog_source_id(&app_root, &bespoke).as_deref(),
            Some("research")
        );
        assert_eq!(release_managed_shadow_source(&app_root, &bespoke)?, None);
        Ok(())
    }

    #[test]
    fn delete_installed_module_clears_only_deleted_module_authority_and_history(
    ) -> anyhow::Result<()> {
        let temp = tempdir()?;
        let root = temp.path();
        seed_test_business_os_app_root(root)?;
        let installed_app_root = resolve_business_os_installed_app_root(root);
        let write_module = |module_id: &str| -> anyhow::Result<()> {
            let module_dir = installed_app_root.join("installed-modules").join(module_id);
            fs::create_dir_all(&module_dir)?;
            fs::write(
                module_dir.join("module.json"),
                serde_json::to_vec_pretty(&serde_json::json!({
                    "id": module_id,
                    "title": module_id,
                    "version": "1.0.0",
                    "entry": format!("installed-modules/{module_id}/index.html"),
                    "install_scope": "installed"
                }))?,
            )?;
            fs::write(module_dir.join("index.html"), "<!doctype html>")?;
            Ok(())
        };
        write_module("inventory")?;
        write_module("ledger")?;
        fs::create_dir_all(root.join("runtime"))?;
        fs::write(
            root.join("runtime/business-os-module-layout.json"),
            serde_json::to_vec_pretty(&serde_json::json!({
                "version": 1,
                "labels": {},
                "ungrouped": ["inventory", "ledger"],
                "groups": []
            }))?,
        )?;

        seed_business_user(root, "viewer", "user")?;
        seed_business_user(root, "ledger-owner", "user")?;
        seed_module_founder_acl(root, "inventory", "viewer")?;
        seed_module_founder_acl(root, "ledger", "ledger-owner")?;
        seed_permission_grant(
            root,
            "grant_inventory_modify_old",
            "user",
            "viewer",
            BusinessOsPermission::AppsModify,
            "module",
            "inventory",
        )?;
        seed_permission_grant(
            root,
            "grant_ledger_modify_keep",
            "user",
            "viewer",
            BusinessOsPermission::AppsModify,
            "module",
            "ledger",
        )?;
        seed_permission_grant(
            root,
            "grant_collection_named_inventory_keep",
            "user",
            "viewer",
            BusinessOsPermission::AppsModify,
            "collection",
            "inventory",
        )?;

        let now = now_ms() as i64;
        let conn = open_store(root)?;
        for module_id in ["inventory", "ledger"] {
            conn.execute(
                "INSERT INTO business_module_releases
                    (version_id, module_id, version, status, manifest_json, snapshot_json,
                     created_by, created_at_ms, notes)
                 VALUES (?1, ?2, 1, 'released', '{}', '{}', 'tester', ?3, 'old evidence')",
                params![format!("release_{module_id}"), module_id, now],
            )?;
            conn.execute(
                "INSERT INTO business_module_versions
                    (version_id, module_id, seq, origin, label, bundle_sha256, files_json,
                     sealed, created_by, created_at_ms, updated_at_ms)
                 VALUES (?1, ?2, 1, 'install', 'Installed', ?3, '[]', 1, 'tester', ?4, ?4)",
                params![
                    format!("version_{module_id}"),
                    module_id,
                    format!("sha-{module_id}"),
                    now
                ],
            )?;
        }
        drop(conn);

        let viewer = test_session("viewer", "user");
        assert!(
            module_policy_decision(root, &viewer, BusinessOsPermission::AppsModify, "inventory")?
                .allowed
        );
        assert!(
            module_policy_decision(root, &viewer, BusinessOsPermission::AppsModify, "ledger")?
                .allowed
        );

        delete_installed_module(
            &installed_app_root,
            root,
            ModuleDeleteRequest {
                module_id: "inventory".to_owned(),
            },
        )?;

        let conn = open_store(root)?;
        let target_acl: i64 = conn.query_row(
            "SELECT COUNT(*) FROM business_module_acl WHERE module_id = 'inventory'",
            [],
            |row| row.get(0),
        )?;
        let target_grants: i64 = conn.query_row(
            "SELECT COUNT(*) FROM business_permission_grants
             WHERE scope_type = 'module' AND scope_id = 'inventory'",
            [],
            |row| row.get(0),
        )?;
        let target_releases: i64 = conn.query_row(
            "SELECT COUNT(*) FROM business_module_releases WHERE module_id = 'inventory'",
            [],
            |row| row.get(0),
        )?;
        let target_versions: i64 = conn.query_row(
            "SELECT COUNT(*) FROM business_module_versions WHERE module_id = 'inventory'",
            [],
            |row| row.get(0),
        )?;
        assert_eq!(
            (target_acl, target_grants, target_releases, target_versions),
            (0, 0, 0, 0)
        );

        let ledger_state: (i64, i64, i64, i64) = conn.query_row(
            "SELECT
                (SELECT COUNT(*) FROM business_module_acl WHERE module_id = 'ledger'),
                (SELECT COUNT(*) FROM business_permission_grants
                    WHERE scope_type = 'module' AND scope_id = 'ledger'),
                (SELECT COUNT(*) FROM business_module_releases WHERE module_id = 'ledger'),
                (SELECT COUNT(*) FROM business_module_versions WHERE module_id = 'ledger')",
            [],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
        )?;
        assert_eq!(ledger_state, (1, 1, 1, 1));
        let other_scope_grant: i64 = conn.query_row(
            "SELECT COUNT(*) FROM business_permission_grants
             WHERE grant_id = 'grant_collection_named_inventory_keep'
               AND scope_type = 'collection' AND scope_id = 'inventory'",
            [],
            |row| row.get(0),
        )?;
        assert_eq!(other_scope_grant, 1);
        drop(conn);

        write_module("inventory")?;
        assert!(
            !module_policy_decision(root, &viewer, BusinessOsPermission::AppsModify, "inventory")?
                .allowed,
            "reinstalling the same id must not revive its old grant or ACL"
        );
        assert!(
            module_policy_decision(root, &viewer, BusinessOsPermission::AppsModify, "ledger")?
                .allowed,
            "the parallel module grant must survive unchanged"
        );
        Ok(())
    }

    #[test]
    fn delete_installed_module_rolls_back_files_layout_and_state_on_cleanup_failure(
    ) -> anyhow::Result<()> {
        let temp = tempdir()?;
        let root = temp.path();
        seed_test_business_os_app_root(root)?;
        let installed_app_root = resolve_business_os_installed_app_root(root);
        let module_dir = installed_app_root.join("installed-modules/inventory");
        fs::create_dir_all(&module_dir)?;
        fs::write(
            module_dir.join("module.json"),
            r#"{"id":"inventory","title":"Inventory","install_scope":"installed"}"#,
        )?;
        fs::create_dir_all(root.join("runtime"))?;
        let layout_path = root.join("runtime/business-os-module-layout.json");
        let original_layout = serde_json::to_vec_pretty(&serde_json::json!({
            "version": 1,
            "labels": {},
            "ungrouped": ["inventory"],
            "groups": []
        }))?;
        fs::write(&layout_path, &original_layout)?;

        seed_business_user(root, "viewer", "user")?;
        seed_module_founder_acl(root, "inventory", "viewer")?;
        seed_permission_grant(
            root,
            "grant_inventory_delete_rollback",
            "user",
            "viewer",
            BusinessOsPermission::AppsModify,
            "module",
            "inventory",
        )?;
        let now = now_ms() as i64;
        let conn = open_store(root)?;
        conn.execute(
            "INSERT INTO business_module_releases
                (version_id, module_id, version, status, manifest_json, snapshot_json,
                 created_by, created_at_ms, notes)
             VALUES ('release_inventory_rollback', 'inventory', 1, 'released', '{}', '{}',
                 'tester', ?1, 'evidence')",
            params![now],
        )?;
        conn.execute(
            "INSERT INTO business_module_versions
                (version_id, module_id, seq, origin, label, bundle_sha256, files_json,
                 sealed, created_by, created_at_ms, updated_at_ms)
             VALUES ('version_inventory_rollback', 'inventory', 1, 'install', 'Installed',
                 'sha-inventory', '[]', 1, 'tester', ?1, ?1)",
            params![now],
        )?;
        conn.execute_batch(
            "CREATE TRIGGER fail_inventory_release_delete
             BEFORE DELETE ON business_module_releases
             WHEN OLD.module_id = 'inventory'
             BEGIN
               SELECT RAISE(FAIL, 'injected module cleanup failure');
             END;",
        )?;
        drop(conn);

        let error = delete_installed_module(
            &installed_app_root,
            root,
            ModuleDeleteRequest {
                module_id: "inventory".to_owned(),
            },
        )
        .expect_err("injected cleanup failure must abort deletion");
        assert!(error.to_string().contains("module deletion rolled back"));
        assert!(module_dir.join("module.json").is_file());
        assert_eq!(fs::read(&layout_path)?, original_layout);

        let conn = open_store(root)?;
        let state: (i64, i64, i64, i64) = conn.query_row(
            "SELECT
                (SELECT COUNT(*) FROM business_module_acl WHERE module_id = 'inventory'),
                (SELECT COUNT(*) FROM business_permission_grants
                    WHERE scope_type = 'module' AND scope_id = 'inventory'),
                (SELECT COUNT(*) FROM business_module_releases WHERE module_id = 'inventory'),
                (SELECT COUNT(*) FROM business_module_versions WHERE module_id = 'inventory')",
            [],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
        )?;
        assert_eq!(state, (1, 1, 1, 1));
        Ok(())
    }

    #[test]
    fn sealed_module_versions_project_content_addressed_commit_chain() -> anyhow::Result<()> {
        let temp = tempdir()?;
        let root = temp.path();
        let app_root = root.join("src").join("apps").join("business-os");
        fs::create_dir_all(&app_root)?;
        fs::write(app_root.join("index.html"), b"<!doctype html>")?;
        write_widget_module(&app_root, "export const v = 1;\n")?;

        // Two sealed boundaries => two immutable commits with a parent chain.
        record_module_version(root, &app_root, "widget", "install", "Installed", "tester")?
            .expect("first sealed version");
        fs::write(
            app_root.join("modules/widget/index.js"),
            "export const v = 2;\n",
        )?;
        record_module_version(
            root,
            &app_root,
            "widget",
            "manual_release",
            "Release 2",
            "tester",
        )?
        .expect("second sealed version");

        let conn = open_store(root)?;
        let read_commits = |conn: &Connection| -> anyhow::Result<Vec<Value>> {
            let mut stmt = conn.prepare(
                "SELECT payload_json FROM business_records
                 WHERE collection = 'business_module_commits' AND deleted = 0",
            )?;
            let mut commits: Vec<Value> = stmt
                .query_map([], |row| row.get::<_, String>(0))?
                .collect::<rusqlite::Result<Vec<_>>>()?
                .into_iter()
                .filter_map(|json| serde_json::from_str::<Value>(&json).ok())
                .collect();
            commits.sort_by_key(|c| c.get("seq").and_then(Value::as_i64).unwrap_or(0));
            Ok(commits)
        };

        let commits = read_commits(&conn)?;
        assert_eq!(commits.len(), 2, "each sealed version projects one commit");
        let (first, second) = (&commits[0], &commits[1]);

        assert!(
            first
                .get("id")
                .and_then(Value::as_str)
                .map(|id| id.starts_with("commit_"))
                .unwrap_or(false),
            "commit id is content-addressed"
        );
        assert_eq!(
            first.get("module_id").and_then(Value::as_str),
            Some("widget")
        );
        assert_eq!(first.get("sealed").and_then(Value::as_bool), Some(true));
        assert_eq!(
            first.get("parent_id").and_then(Value::as_str),
            Some(""),
            "root commit has no parent"
        );
        assert_eq!(
            second.get("parent_id").and_then(Value::as_str),
            first.get("id").and_then(Value::as_str),
            "second commit links to the first"
        );
        assert!(
            second
                .get("file_manifest")
                .and_then(Value::as_array)
                .map(|manifest| !manifest.is_empty())
                .unwrap_or(false),
            "commit carries a non-empty file manifest"
        );

        // Re-projecting the ledger must not duplicate commits (content-addressed).
        sync_module_version_records(&conn, "widget", now_ms() as i64)?;
        assert_eq!(
            read_commits(&conn)?.len(),
            2,
            "commit projection is idempotent"
        );
        Ok(())
    }

    #[test]
    fn module_release_rejects_stale_source_and_rollback_version_refs_before_manifest_write(
    ) -> anyhow::Result<()> {
        let temp = tempdir()?;
        let root = temp.path();
        seed_test_business_os_app_root(root)?;
        let installed_app_root = write_runtime_installed_inventory_module(root, "0.8.0")?;
        let manifest_path = installed_app_root.join("installed-modules/inventory/module.json");
        let original_manifest = fs::read_to_string(&manifest_path)?;
        seed_business_user(root, "release-user", "user")?;
        seed_permission_grant(
            root,
            "grant_inventory_release_stale_refs",
            "user",
            "release-user",
            BusinessOsPermission::AppsRelease,
            "module",
            "inventory",
        )?;

        for (command_id, field_name) in [
            ("cmd_inventory_release_stale_source", "source_version_id"),
            (
                "cmd_inventory_release_stale_rollback",
                "rollback_version_id",
            ),
        ] {
            let mut payload = serde_json::json!({
                "module_id": "inventory",
                "target_version": "1.0.0",
                "release_channel": "team",
                "data_access_review": locked_inventory_data_review(),
                "notes": "release"
            });
            payload.as_object_mut().expect("payload object").insert(
                field_name.to_string(),
                Value::String("missing-version".to_string()),
            );
            let outcome = accept_rxdb_business_command(
                root,
                serde_json::json!({
                    "id": command_id,
                    "command_id": command_id,
                    "module": "app-store",
                    "command_type": "ctox.module.release",
                    "record_id": "inventory",
                    "status": "pending_sync",
                    "payload": payload,
                    "client_context": {
                        "actor": {
                            "id": "release-user",
                            "display_name": "Release User"
                        }
                    }
                }),
            )?;
            assert_eq!(outcome.get("ok").and_then(Value::as_bool), Some(false));
            assert_eq!(
                outcome.get("status").and_then(Value::as_str),
                Some("failed")
            );
            assert!(outcome
                .pointer("/result/error")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .contains(field_name));
            assert_eq!(fs::read_to_string(&manifest_path)?, original_manifest);
        }

        let conn = open_store(root)?;
        let release_rows: i64 = conn.query_row(
            "SELECT COUNT(*) FROM business_module_releases WHERE module_id = 'inventory'",
            [],
            |row| row.get(0),
        )?;
        assert_eq!(release_rows, 0);
        Ok(())
    }

    #[test]
    fn module_release_restores_manifest_when_release_db_write_fails() -> anyhow::Result<()> {
        let temp = tempdir()?;
        let root = temp.path();
        seed_test_business_os_app_root(root)?;
        let installed_app_root = write_runtime_installed_inventory_module(root, "0.8.0")?;
        let manifest_path = installed_app_root.join("installed-modules/inventory/module.json");
        let original_manifest = fs::read_to_string(&manifest_path)?;
        seed_business_user(root, "release-user", "user")?;
        seed_permission_grant(
            root,
            "grant_inventory_release_db_fail",
            "user",
            "release-user",
            BusinessOsPermission::AppsRelease,
            "module",
            "inventory",
        )?;
        let conn = open_store(root)?;
        conn.execute_batch(
            "CREATE TRIGGER fail_inventory_release_insert
             BEFORE INSERT ON business_module_releases
             BEGIN
               SELECT RAISE(FAIL, 'injected release insert failure');
             END;",
        )?;
        drop(conn);

        let outcome = accept_rxdb_business_command(
            root,
            serde_json::json!({
                "id": "cmd_inventory_release_db_fail",
                "command_id": "cmd_inventory_release_db_fail",
                "module": "app-store",
                "command_type": "ctox.module.release",
                "record_id": "inventory",
                "status": "pending_sync",
                "payload": {
                    "module_id": "inventory",
                    "target_version": "1.0.0",
                    "release_channel": "team",
                    "data_access_review": locked_inventory_data_review(),
                    "notes": "release"
                },
                "client_context": {
                    "actor": {
                        "id": "release-user",
                        "display_name": "Release User"
                    }
                }
            }),
        )?;
        assert_eq!(outcome.get("ok").and_then(Value::as_bool), Some(false));
        assert_eq!(
            outcome.get("status").and_then(Value::as_str),
            Some("failed")
        );
        assert!(outcome
            .pointer("/result/error")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .contains("injected release insert failure"));
        assert_eq!(fs::read_to_string(&manifest_path)?, original_manifest);

        let conn = open_store(root)?;
        let release_rows: i64 = conn.query_row(
            "SELECT COUNT(*) FROM business_module_releases WHERE module_id = 'inventory'",
            [],
            |row| row.get(0),
        )?;
        assert_eq!(release_rows, 0);
        let stored = outbound_load_required(
            &conn,
            "business_commands",
            "cmd_inventory_release_db_fail",
            "command",
        )?;
        assert_eq!(stored.get("status").and_then(Value::as_str), Some("failed"));
        Ok(())
    }

    #[test]
    fn module_release_rollback_restores_manifest_when_status_update_fails() -> anyhow::Result<()> {
        let temp = tempdir()?;
        let root = temp.path();
        seed_test_business_os_app_root(root)?;
        let installed_app_root = write_runtime_installed_inventory_module(root, "1.1.0")?;
        let manifest_path = installed_app_root.join("installed-modules/inventory/module.json");
        let original_manifest = fs::read_to_string(&manifest_path)?;
        seed_business_user(root, "rollback-user", "user")?;
        seed_permission_grant(
            root,
            "grant_inventory_rollback_db_fail",
            "user",
            "rollback-user",
            BusinessOsPermission::AppsRollback,
            "module",
            "inventory",
        )?;

        let now = now_ms() as i64;
        let release_manifest = serde_json::json!({
            "id": "inventory",
            "title": "Inventory",
            "version": "1.0.0",
            "entry": "installed-modules/inventory/index.html",
            "install_scope": "installed",
            "collections": ["inventory_items"],
            "lifecycle": {
                "visibility_state": "team",
                "audience": "team",
                "release_channel": "team"
            }
        });
        let conn = open_store(root)?;
        conn.execute(
            "INSERT INTO business_module_releases
                (version_id, module_id, version, status, manifest_json, snapshot_json,
                 created_by, created_at_ms, notes)
             VALUES ('modrel_inventory_db_fail', 'inventory', 1, 'rolled_back', ?1, '{}',
                 'release-owner', ?2, 'release 1')",
            params![serde_json::to_string(&release_manifest)?, now],
        )?;
        conn.execute_batch(
            "CREATE TRIGGER fail_inventory_release_status_update
             BEFORE UPDATE OF status ON business_module_releases
             BEGIN
               SELECT RAISE(FAIL, 'injected release status failure');
             END;",
        )?;
        drop(conn);

        let outcome = accept_rxdb_business_command(
            root,
            serde_json::json!({
                "id": "cmd_inventory_rollback_db_fail",
                "command_id": "cmd_inventory_rollback_db_fail",
                "module": "app-store",
                "command_type": "ctox.module.rollback",
                "record_id": "inventory",
                "status": "pending_sync",
                "payload": {
                    "module_id": "inventory",
                    "version_id": "modrel_inventory_db_fail"
                },
                "client_context": {
                    "actor": {
                        "id": "rollback-user",
                        "display_name": "Rollback User"
                    }
                }
            }),
        )?;
        assert_eq!(outcome.get("ok").and_then(Value::as_bool), Some(false));
        assert_eq!(
            outcome.get("status").and_then(Value::as_str),
            Some("failed")
        );
        assert!(outcome
            .pointer("/result/error")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .contains("injected release status failure"));
        assert_eq!(fs::read_to_string(&manifest_path)?, original_manifest);

        let conn = open_store(root)?;
        let status: String = conn.query_row(
            "SELECT status FROM business_module_releases WHERE version_id = 'modrel_inventory_db_fail'",
            [],
            |row| row.get(0),
        )?;
        assert_eq!(status, "rolled_back");
        Ok(())
    }

    #[test]
    fn module_versions_record_rollback_and_remove_added_files() -> anyhow::Result<()> {
        let temp = tempdir()?;
        let root = temp.path();
        let app_root = root.join("src").join("apps").join("business-os");
        fs::create_dir_all(&app_root)?;
        fs::write(app_root.join("index.html"), b"<!doctype html>")?;
        write_widget_module(&app_root, "export const v = 1;\n")?;

        // Install baseline (v0): sealed boundary.
        let v0 =
            record_module_version(root, &app_root, "widget", "install", "Installed", "tester")?
                .expect("install version recorded");
        let v0_id = v0
            .get("version_id")
            .and_then(Value::as_str)
            .unwrap()
            .to_string();
        assert_eq!(v0.get("origin").and_then(Value::as_str), Some("install"));
        assert_eq!(v0.get("sealed").and_then(Value::as_bool), Some(true));
        let baseline_sha = v0
            .get("bundle_sha256")
            .and_then(Value::as_str)
            .unwrap()
            .to_string();
        let baseline_manifest =
            fs::read_to_string(app_root.join("modules").join("widget").join("module.json"))?;

        // Two edits coalesce into a single open working version.
        save_widget_source(root, "index.js", "export const v = 2;\n")?;
        save_widget_source(root, "index.js", "export const v = 3;\n")?;
        let listed = list_module_versions(
            root,
            ModuleVersionListRequest {
                module_id: "widget".to_string(),
            },
        )?;
        let versions = listed.get("versions").and_then(Value::as_array).unwrap();
        assert_eq!(versions.len(), 2, "install + one coalesced edit version");
        assert_ne!(
            baseline_sha,
            compute_module_bundle(&app_root, "widget")?.sha256,
            "edits change the bundle hash"
        );

        // A brand new source file added after the baseline.
        save_widget_source(root, "extra.js", "export const extra = true;\n")?;
        assert!(app_root
            .join("modules")
            .join("widget")
            .join("extra.js")
            .is_file());
        save_widget_source(
            root,
            "module.json",
            r#"{"id":"widget","title":"Broken Widget","entry":"modules/widget/index.html","version":"0.9.0"}"#,
        )?;

        // Roll back to the install baseline.
        let session = chef_session();
        let result = rollback_module_to_version(
            root,
            &app_root,
            &session,
            ModuleVersionRollbackRequest {
                module_id: "widget".to_string(),
                version_id: v0_id,
            },
        )?;
        assert_eq!(result.get("ok").and_then(Value::as_bool), Some(true));

        // index.js restored to baseline; extra.js removed; bundle hash matches baseline.
        let restored =
            fs::read_to_string(app_root.join("modules").join("widget").join("index.js"))?;
        assert_eq!(restored, "export const v = 1;\n");
        let restored_manifest =
            fs::read_to_string(app_root.join("modules").join("widget").join("module.json"))?;
        assert_eq!(
            restored_manifest, baseline_manifest,
            "module.json must be restored together with source files"
        );
        assert!(!app_root
            .join("modules")
            .join("widget")
            .join("extra.js")
            .is_file());
        assert_eq!(
            baseline_sha,
            compute_module_bundle(&app_root, "widget")?.sha256
        );

        // A sealed rollback boundary is now the newest version.
        let listed = list_module_versions(
            root,
            ModuleVersionListRequest {
                module_id: "widget".to_string(),
            },
        )?;
        let versions = listed.get("versions").and_then(Value::as_array).unwrap();
        assert_eq!(
            versions
                .first()
                .and_then(|version| version.get("origin"))
                .and_then(Value::as_str),
            Some("rollback")
        );
        Ok(())
    }

    #[test]
    fn rollback_rejects_incompatible_collection_schema_before_swap() -> anyhow::Result<()> {
        let temp = tempdir()?;
        let live = temp.path().join("live");
        let staged = temp.path().join("staged");
        fs::create_dir_all(&live)?;
        fs::create_dir_all(&staged)?;
        fs::write(
            live.join("collections.schema.json"),
            r#"{"collections":[{"name":"knowledge","schema":{"version":0}}]}"#,
        )?;
        fs::write(
            staged.join("collections.schema.json"),
            r#"{"collections":[{"name":"knowledge","schema":{"version":1}}]}"#,
        )?;

        let error = ensure_rollback_collection_schema_compatible(&live, &staged)
            .expect_err("schema-changing rollback must fail closed");
        assert!(error.to_string().contains("incompatible collection schema"));
        Ok(())
    }

    #[test]
    fn module_release_rejects_in_place_public_major_line_bump() -> anyhow::Result<()> {
        let temp = tempdir()?;
        let root = temp.path();
        seed_test_business_os_app_root(root)?;
        let app_root = root.join("src/apps/business-os");
        fs::create_dir_all(root.join("runtime"))?;
        let installed_app_root = resolve_business_os_installed_app_root(root);
        write_installed_inventory_module(&installed_app_root, "0.9.0")?;

        record_module_release(
            root,
            &app_root,
            &chef_session(),
            ModuleReleaseRequest {
                module_id: "inventory".to_owned(),
                target_version: "1.0.0".to_owned(),
                release_channel: "team".to_owned(),
                source_version_id: String::new(),
                rollback_version_id: String::new(),
                responsible_user_ids: Vec::new(),
                data_access_review: locked_inventory_data_review(),
                notes: "First team release".to_owned(),
            },
        )?;

        record_module_release(
            root,
            &app_root,
            &chef_session(),
            ModuleReleaseRequest {
                module_id: "inventory".to_owned(),
                target_version: "1.1.0".to_owned(),
                release_channel: "team".to_owned(),
                source_version_id: String::new(),
                rollback_version_id: String::new(),
                responsible_user_ids: Vec::new(),
                data_access_review: locked_inventory_data_review(),
                notes: "Allowed same-major release".to_owned(),
            },
        )?;

        let err = record_module_release(
            root,
            &app_root,
            &chef_session(),
            ModuleReleaseRequest {
                module_id: "inventory".to_owned(),
                target_version: "2.0.0".to_owned(),
                release_channel: "team".to_owned(),
                source_version_id: String::new(),
                rollback_version_id: String::new(),
                responsible_user_ids: Vec::new(),
                data_access_review: locked_inventory_data_review(),
                notes: "Must be a separate app line".to_owned(),
            },
        )
        .unwrap_err();
        assert!(
            err.to_string().contains("separate Business OS app line"),
            "unexpected error: {err}"
        );

        let manifest = fs::read_to_string(
            installed_app_root
                .join("installed-modules")
                .join("inventory")
                .join("module.json"),
        )?;
        let manifest: Value = serde_json::from_str(&manifest)?;
        assert_eq!(
            manifest.get("version").and_then(Value::as_str),
            Some("1.1.0")
        );
        let conn = open_store(root)?;
        let release_rows: i64 = conn.query_row(
            "SELECT COUNT(*) FROM business_module_releases WHERE module_id = 'inventory'",
            [],
            |row| row.get(0),
        )?;
        assert_eq!(release_rows, 2);

        Ok(())
    }
}
