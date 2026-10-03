// Target/script registry: SQLite schema access, target upsert/load,
// root-relative path persistence, and script registration.

use super::{
    build_target_api_contract, compute_sha256, default_entry_command, ensure_target_workspace,
    load_script_revisions, load_source_revisions, load_target_view, map_target_row,
    normalize_target_config, now_iso_string, registered_script_matches, resolve_input_path,
    resolve_registered_workspace, resolve_runtime_root, script_extension, slugify, stable_digest,
    target_sources, write_target_manifest, RegisteredTarget, ScrapeScriptRevisionRecord,
    ScrapeTargetView, SCHEMA,
};
use anyhow::{Context, Result};
use rusqlite::{params, Connection, OptionalExtension};
use serde_json::{json, Value};
use std::fs;
use std::path::{Path, PathBuf};

pub(super) fn list_targets(root: &Path) -> Result<Vec<ScrapeTargetView>> {
    let conn = open_db(root)?;
    let mut statement = conn.prepare(
        r#"
        SELECT target_id, target_key, display_name, start_url, target_kind, status, schedule_hint,
               config_json, output_schema_json, workspace_dir, latest_script_revision_no,
               latest_script_sha256, created_at, updated_at
        FROM scrape_target
        ORDER BY updated_at DESC, target_key ASC
        "#,
    )?;
    let rows = statement.query_map([], map_target_row)?;
    rows.collect::<rusqlite::Result<Vec<_>>>()
        .map_err(anyhow::Error::from)
}

pub(super) fn show_target(root: &Path, target_key: &str) -> Result<Option<Value>> {
    let conn = open_db(root)?;
    let target = load_target_view(&conn, target_key)?;
    let Some(target) = target else {
        return Ok(None);
    };
    let revisions = load_script_revisions(&conn, &target.target_id)?;
    let sources = target_sources(&target);
    let source_revisions = load_source_revisions(&conn, &target.target_id)?;
    Ok(Some(json!({
        "target_id": target.target_id,
        "target_key": target.target_key,
        "display_name": target.display_name,
        "start_url": target.start_url,
        "target_kind": target.target_kind,
        "status": target.status,
        "schedule_hint": target.schedule_hint,
        "config": target.config,
        "sources": sources,
        "output_schema": target.output_schema,
        "workspace_dir": target.workspace_dir,
        "latest_script_revision_no": target.latest_script_revision_no,
        "latest_script_sha256": target.latest_script_sha256,
        "created_at": target.created_at,
        "updated_at": target.updated_at,
        "revisions": revisions,
        "source_revisions": source_revisions,
    })))
}

/// Report the activated library revision without importing a source-tree script
/// or changing target configuration. Files are checked execution materializations.
pub(crate) fn target_script_registration(root: &Path, target_key: &str) -> Result<Option<Value>> {
    let conn = open_db(root)?;
    let Some(target) = load_target_view(&conn, target_key)? else {
        return Ok(None);
    };
    let active = target.latest_script_revision_no.map(|revision| {
        conn.query_row(
            "SELECT script_path, script_sha256, script_body FROM scrape_script_revision WHERE target_id=?1 AND revision_no=?2",
            params![target.target_id, revision],
            |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?, row.get::<_, String>(2)?)),
        ).optional()
    }).transpose()?.flatten();
    let usable = active.as_ref().is_some_and(|(path, hash, body)| {
        !body.trim().is_empty()
            && target.latest_script_sha256.as_deref() == Some(hash.as_str())
            && compute_sha256(body.trim()) == *hash
            && registered_script_matches(&resolve_input_path(root, path), hash)
    });
    Ok(Some(json!({
        "ok": true,
        "target_key": target.target_key,
        "registered_from": "runtime_sqlite",
        "workspace_dir": resolve_registered_workspace(root, &target),
        "script_registered": usable,
        "revision_no": target.latest_script_revision_no,
        "script_sha256": target.latest_script_sha256,
    })))
}

pub(super) fn show_api(root: &Path, target_key: &str) -> Result<Option<Value>> {
    let conn = open_db(root)?;
    let Some(target) = load_target_view(&conn, target_key)? else {
        return Ok(None);
    };
    Ok(Some(build_target_api_contract(root, &target)))
}

pub(super) fn upsert_target(
    root: &Path,
    runtime_root_arg: &str,
    payload: Value,
) -> Result<ScrapeTargetView> {
    let object = payload
        .as_object()
        .context("target payload must be a json object")?;
    let start_url = object
        .get("start_url")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .context("target payload requires non-empty start_url")?;
    let target_key = slugify(
        object
            .get("target_key")
            .and_then(Value::as_str)
            .or_else(|| object.get("display_name").and_then(Value::as_str))
            .unwrap_or(start_url),
    );
    let display_name = object
        .get("display_name")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or(&target_key)
        .to_string();
    let target_kind = object
        .get("target_kind")
        .and_then(Value::as_str)
        .unwrap_or("generic")
        .to_string();
    let status = object
        .get("status")
        .and_then(Value::as_str)
        .unwrap_or("active")
        .to_string();
    let schedule_hint = object
        .get("schedule_hint")
        .and_then(Value::as_str)
        .map(ToOwned::to_owned);
    let raw_config = object.get("config").cloned().unwrap_or_else(|| json!({}));
    let config = normalize_target_config(start_url, &target_key, &raw_config);
    let output_schema = object
        .get("output_schema")
        .cloned()
        .unwrap_or_else(|| json!({}));
    let target_id = format!("scrape_target-{}", stable_digest(&target_key));
    let runtime_root = resolve_runtime_root(root, runtime_root_arg);
    let workspace_dir = ensure_target_workspace(&runtime_root, &target_key)?;
    let stored_workspace_dir = path_for_storage(root, &workspace_dir);
    let conn = open_db(root)?;
    let existing = conn
        .query_row(
            r#"
            SELECT created_at, latest_script_revision_no, latest_script_sha256
            FROM scrape_target
            WHERE target_key = ?1
            "#,
            params![target_key],
            |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, Option<i64>>(1)?,
                    row.get::<_, Option<String>>(2)?,
                ))
            },
        )
        .optional()?;
    let created_at = existing
        .as_ref()
        .map(|item| item.0.clone())
        .unwrap_or_else(now_iso_string);
    let latest_script_revision_no = existing.as_ref().and_then(|item| item.1);
    let latest_script_sha256 = existing.as_ref().and_then(|item| item.2.clone());
    let updated_at = now_iso_string();
    conn.execute(
        r#"
        INSERT INTO scrape_target (
            target_id, target_key, display_name, start_url, target_kind, status, schedule_hint,
            config_json, output_schema_json, workspace_dir, latest_script_revision_no,
            latest_script_sha256, created_at, updated_at
        ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14)
        ON CONFLICT(target_key) DO UPDATE SET
            display_name = excluded.display_name,
            start_url = excluded.start_url,
            target_kind = excluded.target_kind,
            status = excluded.status,
            schedule_hint = excluded.schedule_hint,
            config_json = excluded.config_json,
            output_schema_json = excluded.output_schema_json,
            workspace_dir = excluded.workspace_dir,
            updated_at = excluded.updated_at
        "#,
        params![
            target_id,
            target_key,
            display_name,
            start_url,
            target_kind,
            status,
            schedule_hint,
            serde_json::to_string(&config)?,
            serde_json::to_string(&output_schema)?,
            stored_workspace_dir.to_string_lossy(),
            latest_script_revision_no,
            latest_script_sha256,
            created_at,
            updated_at,
        ],
    )?;
    let target =
        load_target_view(&conn, &target_key)?.context("failed to reload target after upsert")?;
    write_target_manifest(root, &target)?;
    Ok(target)
}

pub(super) fn register_script(
    root: &Path,
    runtime_root_arg: &str,
    target_key: &str,
    script_file_arg: &str,
    language: &str,
    change_reason: Option<&str>,
    notes: Option<&str>,
) -> Result<Value> {
    let conn = open_db(root)?;
    let target = load_target_view(&conn, target_key)?.context("target_key not found")?;
    let source_path = resolve_input_path(root, script_file_arg);
    let script_body = fs::read_to_string(&source_path)
        .with_context(|| format!("failed to read script file {}", source_path.display()))?;
    if script_body.trim().is_empty() {
        anyhow::bail!(
            "scrape script {} must contain non-whitespace content",
            source_path.display()
        );
    }
    let script_sha256 = compute_sha256(script_body.trim());
    if let Some((revision_no, script_path, created_at)) = conn
        .query_row(
            r#"
            SELECT revision_no, script_path, created_at
            FROM scrape_script_revision
            WHERE target_id = ?1 AND script_sha256 = ?2
            "#,
            params![target.target_id, script_sha256],
            |row| {
                Ok((
                    row.get::<_, i64>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                ))
            },
        )
        .optional()?
    {
        let stored_revision_path = PathBuf::from(&script_path);
        let workspace_dir = ensure_target_workspace(
            &resolve_runtime_root(root, runtime_root_arg),
            &target.target_key,
        )?;
        let extension = stored_revision_path
            .extension()
            .and_then(|value| value.to_str())
            .map(|value| format!(".{value}"))
            .unwrap_or_else(|| script_extension(language, &source_path));
        let revision_file_name = stored_revision_path
            .file_name()
            .map(ToOwned::to_owned)
            .unwrap_or_else(|| {
                format!(
                    "rev{revision_no:04}_{}.{}",
                    &script_sha256[..8],
                    extension.trim_start_matches('.')
                )
                .into()
            });
        let revision_path = workspace_dir
            .join("scripts")
            .join("revisions")
            .join(revision_file_name);
        fs::create_dir_all(
            revision_path
                .parent()
                .context("deduplicated scrape script revision has no parent")?,
        )?;
        fs::write(&revision_path, &script_body).with_context(|| {
            format!(
                "failed to publish deduplicated script revision to {}",
                revision_path.display()
            )
        })?;

        let current_path = workspace_dir
            .join("scripts")
            .join(format!("current{extension}"));
        fs::write(&current_path, &script_body).with_context(|| {
            format!(
                "failed to reactivate script revision to {}",
                current_path.display()
            )
        })?;

        let activated_at = now_iso_string();
        let persisted_revision_path = path_for_storage(root, &revision_path);
        conn.execute(
            r#"
            UPDATE scrape_script_revision
            SET script_path = ?3
            WHERE target_id = ?1 AND revision_no = ?2
            "#,
            params![
                target.target_id,
                revision_no,
                persisted_revision_path.to_string_lossy()
            ],
        )?;
        conn.execute(
            r#"
            UPDATE scrape_target
            SET latest_script_revision_no = ?2,
                latest_script_sha256 = ?3,
                updated_at = ?4
            WHERE target_id = ?1
            "#,
            params![target.target_id, revision_no, script_sha256, activated_at],
        )?;
        let updated_target = load_target_view(&conn, target_key)?
            .context("failed to reload target after script reactivation")?;
        write_target_manifest(root, &updated_target)?;
        return Ok(json!({
            "target_key": updated_target.target_key,
            "target_id": updated_target.target_id,
            "revision_no": revision_no,
            "script_path": revision_path,
            "current_path": current_path,
            "script_sha256": script_sha256,
            "deduplicated": true,
            "reactivated": true,
            "activated_at": activated_at,
            "created_at": created_at,
        }));
    }
    let next_revision = conn.query_row(
        "SELECT COALESCE(MAX(revision_no), 0) + 1 FROM scrape_script_revision WHERE target_id = ?1",
        params![target.target_id],
        |row| row.get::<_, i64>(0),
    )?;
    let workspace_dir = ensure_target_workspace(
        &resolve_runtime_root(root, runtime_root_arg),
        &target.target_key,
    )?;
    let extension = script_extension(language, &source_path);
    let revision_path = workspace_dir
        .join("scripts")
        .join("revisions")
        .join(format!(
            "rev{next_revision:04}_{}.{}",
            &script_sha256[..8],
            extension.trim_start_matches('.')
        ));
    let current_path = workspace_dir
        .join("scripts")
        .join(format!("current{}", extension));
    fs::write(&revision_path, &script_body).with_context(|| {
        format!(
            "failed to publish script revision to {}",
            revision_path.display()
        )
    })?;
    fs::write(&current_path, &script_body).with_context(|| {
        format!(
            "failed to publish current script to {}",
            current_path.display()
        )
    })?;

    let created_at = now_iso_string();
    let entry_command = default_entry_command(language);
    let stored_revision_path = path_for_storage(root, &revision_path);
    conn.execute(
        r#"
        INSERT INTO scrape_script_revision (
            target_id, revision_no, script_path, language, entry_command_json, script_sha256,
            script_body, change_reason, notes, created_at
        ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)
        "#,
        params![
            target.target_id,
            next_revision,
            stored_revision_path.to_string_lossy(),
            language,
            serde_json::to_string(&entry_command)?,
            script_sha256,
            script_body,
            change_reason,
            notes,
            created_at,
        ],
    )?;
    conn.execute(
        r#"
        UPDATE scrape_target
        SET latest_script_revision_no = ?2,
            latest_script_sha256 = ?3,
            updated_at = ?4
        WHERE target_id = ?1
        "#,
        params![target.target_id, next_revision, script_sha256, created_at],
    )?;
    let updated_target = load_target_view(&conn, target_key)?
        .context("failed to reload target after script registration")?;
    write_target_manifest(root, &updated_target)?;
    Ok(json!({
        "target_key": updated_target.target_key,
        "target_id": updated_target.target_id,
        "revision_no": next_revision,
        "script_path": revision_path,
        "current_path": current_path,
        "script_sha256": script_sha256,
        "deduplicated": false,
        "created_at": created_at,
    }))
}

pub(super) fn register_source_module(
    root: &Path,
    runtime_root_arg: &str,
    target_key: &str,
    source_key_raw: &str,
    module_file_arg: &str,
    language: &str,
    change_reason: Option<&str>,
    notes: Option<&str>,
) -> Result<Value> {
    let conn = open_db(root)?;
    let target = load_target_view(&conn, target_key)?.context("target_key not found")?;
    let source_key = slugify(source_key_raw);
    let source = target_sources(&target)
        .into_iter()
        .find(|item| item.source_key == source_key)
        .with_context(|| format!("source_key `{source_key}` not found on target `{target_key}`"))?;
    let source_path = resolve_input_path(root, module_file_arg);
    let module_body = fs::read_to_string(&source_path)
        .with_context(|| format!("failed to read source module {}", source_path.display()))?;
    if module_body.trim().is_empty() {
        anyhow::bail!(
            "scrape source module {} must contain non-whitespace content",
            source_path.display()
        );
    }
    let module_sha256 = compute_sha256(module_body.trim());
    if let Some((revision_no, module_path, created_at)) = conn
        .query_row(
            r#"
            SELECT revision_no, module_path, created_at
            FROM scrape_source_revision
            WHERE target_id = ?1 AND source_key = ?2 AND module_sha256 = ?3
            "#,
            params![target.target_id, source_key, module_sha256],
            |row| {
                Ok((
                    row.get::<_, i64>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                ))
            },
        )
        .optional()?
    {
        let revision_path = PathBuf::from(&module_path);
        let workspace_dir = ensure_target_workspace(
            &resolve_runtime_root(root, runtime_root_arg),
            &target.target_key,
        )?;
        let extension = revision_path
            .extension()
            .and_then(|value| value.to_str())
            .map(|value| format!(".{value}"))
            .unwrap_or_else(|| script_extension(language, &source_path));
        let source_dir = workspace_dir.join("sources").join(&source.source_key);
        fs::create_dir_all(&source_dir)?;
        fs::write(&revision_path, &module_body).with_context(|| {
            format!(
                "failed to publish deduplicated source module revision to {}",
                revision_path.display()
            )
        })?;
        let current_path = source_dir.join(format!("current{extension}"));
        let configured_path = workspace_dir.join(&source.extraction_module);
        if let Some(parent) = configured_path.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(&current_path, &module_body).with_context(|| {
            format!(
                "failed to reactivate source module revision to {}",
                current_path.display()
            )
        })?;
        if configured_path != current_path {
            fs::write(&configured_path, &module_body).with_context(|| {
                format!(
                    "failed to reactivate configured source module to {}",
                    configured_path.display()
                )
            })?;
        }

        write_target_manifest(root, &target)?;
        return Ok(json!({
            "target_key": target.target_key,
            "target_id": target.target_id,
            "source_key": source.source_key,
            "revision_no": revision_no,
            "module_path": module_path,
            "current_path": current_path,
            "configured_path": configured_path,
            "module_sha256": module_sha256,
            "deduplicated": true,
            "reactivated": true,
            "created_at": created_at,
        }));
    }
    let next_revision = conn.query_row(
        "SELECT COALESCE(MAX(revision_no), 0) + 1 FROM scrape_source_revision WHERE target_id = ?1 AND source_key = ?2",
        params![target.target_id, source_key],
        |row| row.get::<_, i64>(0),
    )?;
    let workspace_dir = ensure_target_workspace(
        &resolve_runtime_root(root, runtime_root_arg),
        &target.target_key,
    )?;
    let extension = script_extension(language, &source_path);
    let source_dir = workspace_dir.join("sources").join(&source.source_key);
    fs::create_dir_all(source_dir.join("revisions"))?;
    let revision_path = source_dir.join("revisions").join(format!(
        "rev{next_revision:04}_{}.{}",
        &module_sha256[..8],
        extension.trim_start_matches('.')
    ));
    let current_path = source_dir.join(format!("current{}", extension));
    let configured_path = workspace_dir.join(&source.extraction_module);
    if let Some(parent) = configured_path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(&revision_path, &module_body).with_context(|| {
        format!(
            "failed to publish source module revision to {}",
            revision_path.display()
        )
    })?;
    fs::write(&current_path, &module_body).with_context(|| {
        format!(
            "failed to publish source module current file to {}",
            current_path.display()
        )
    })?;
    if configured_path != current_path {
        fs::write(&configured_path, &module_body).with_context(|| {
            format!(
                "failed to publish configured source module to {}",
                configured_path.display()
            )
        })?;
    }
    let created_at = now_iso_string();
    conn.execute(
        r#"
        INSERT INTO scrape_source_revision (
            target_id, source_key, revision_no, module_path, language, module_sha256,
            module_body, change_reason, notes, created_at
        ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)
        "#,
        params![
            target.target_id,
            source.source_key,
            next_revision,
            revision_path.to_string_lossy(),
            language,
            module_sha256,
            module_body,
            change_reason,
            notes,
            created_at,
        ],
    )?;
    write_target_manifest(root, &target)?;
    Ok(json!({
        "target_key": target.target_key,
        "target_id": target.target_id,
        "source_key": source.source_key,
        "revision_no": next_revision,
        "module_path": revision_path,
        "current_path": current_path,
        "configured_path": configured_path,
        "module_sha256": module_sha256,
        "deduplicated": false,
        "created_at": created_at,
    }))
}

fn path_under_root(root: &Path, path: &Path) -> Option<PathBuf> {
    if let Ok(relative) = path.strip_prefix(root) {
        return Some(relative.to_path_buf());
    }

    let canonical_root = fs::canonicalize(root).ok()?;
    let canonical_path = fs::canonicalize(path).ok()?;
    canonical_path
        .strip_prefix(canonical_root)
        .ok()
        .map(Path::to_path_buf)
}

fn path_for_storage(root: &Path, path: &Path) -> PathBuf {
    if path.is_absolute() {
        path_under_root(root, path).unwrap_or_else(|| path.to_path_buf())
    } else {
        path.to_path_buf()
    }
}

fn table_has_column(conn: &Connection, table: &str, column: &str) -> Result<bool> {
    let sql = format!("PRAGMA table_info({table})");
    let mut statement = conn.prepare(&sql)?;
    let columns = statement.query_map([], |row| row.get::<_, String>(1))?;
    for existing in columns {
        if existing? == column {
            return Ok(true);
        }
    }
    Ok(false)
}

fn migrate_registered_paths(root: &Path, conn: &Connection) -> Result<()> {
    if table_has_column(conn, "scrape_target", "workspace_dir")? {
        let rows = {
            let mut statement =
                conn.prepare("SELECT target_id, workspace_dir FROM scrape_target")?;
            let rows = statement.query_map([], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
            })?;
            rows.collect::<rusqlite::Result<Vec<_>>>()?
        };
        for (target_id, stored_path) in rows {
            let path = PathBuf::from(&stored_path);
            if !path.is_absolute() {
                continue;
            }
            let Some(relative) = path_under_root(root, &path) else {
                // Historical paths outside this root are deliberately retained:
                // there is no safe root-relative representation for them.
                continue;
            };
            conn.execute(
                "UPDATE scrape_target SET workspace_dir = ?2 WHERE target_id = ?1",
                params![target_id, relative.to_string_lossy()],
            )?;
        }
    }

    if table_has_column(conn, "scrape_script_revision", "script_path")? {
        let rows = {
            let mut statement =
                conn.prepare("SELECT revision_id, script_path FROM scrape_script_revision")?;
            let rows = statement.query_map([], |row| {
                Ok((row.get::<_, i64>(0)?, row.get::<_, String>(1)?))
            })?;
            rows.collect::<rusqlite::Result<Vec<_>>>()?
        };
        for (revision_id, stored_path) in rows {
            let path = PathBuf::from(&stored_path);
            if !path.is_absolute() {
                continue;
            }
            let Some(relative) = path_under_root(root, &path) else {
                // See workspace_dir above: outside-root paths remain absolute.
                continue;
            };
            conn.execute(
                "UPDATE scrape_script_revision SET script_path = ?2 WHERE revision_id = ?1",
                params![revision_id, relative.to_string_lossy()],
            )?;
        }
    }
    Ok(())
}

pub(super) fn open_db(root: &Path) -> Result<Connection> {
    let path = resolve_db_path(root);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("failed to create scrape db parent {}", parent.display()))?;
    }
    let conn = Connection::open(&path)
        .with_context(|| format!("failed to open scrape db {}", path.display()))?;
    conn.execute_batch(SCHEMA)?;
    migrate_registered_paths_once(&path, || migrate_registered_paths(root, &conn))?;
    Ok(conn)
}

/// Migrating historical absolute paths is a one-time repair, but it used to run
/// on every open of the shared core database — two table scans plus conditional
/// writes per scrape operation. Under the parallel test suite that was enough
/// to fail 45 unrelated Business OS commands. Legacy rows only ever arrive
/// before the process starts, so running it once per database is sufficient.
static MIGRATED_PATHS: std::sync::OnceLock<std::sync::Mutex<std::collections::HashSet<PathBuf>>> =
    std::sync::OnceLock::new();

fn migrate_registered_paths_once(
    db_path: &Path,
    migrate: impl FnOnce() -> Result<()>,
) -> Result<()> {
    let migrated = MIGRATED_PATHS.get_or_init(Default::default);
    if migrated
        .lock()
        .unwrap_or_else(|err| err.into_inner())
        .contains(db_path)
    {
        return Ok(());
    }
    migrate()?;
    migrated
        .lock()
        .unwrap_or_else(|err| err.into_inner())
        .insert(db_path.to_path_buf());
    Ok(())
}

/// The migration runs once per database per process. Tests that stage legacy
/// rows after a database has already been opened need that memory cleared.
#[cfg(test)]
pub(super) fn forget_path_migrations_for_test() {
    if let Some(migrated) = MIGRATED_PATHS.get() {
        migrated
            .lock()
            .unwrap_or_else(|err| err.into_inner())
            .clear();
    }
}

pub(super) fn resolve_db_path(root: &Path) -> PathBuf {
    crate::paths::core_db(root)
}

pub(super) fn load_registered_target(
    root: &Path,
    conn: &Connection,
    target_key: &str,
) -> Result<Option<RegisteredTarget>> {
    let Some(mut view) = load_target_view(conn, target_key)? else {
        return Ok(None);
    };
    let script = view
        .latest_script_revision_no
        .map(|active_revision_no| {
            conn.query_row(
                r#"
            SELECT revision_no, script_path, language, entry_command_json, script_sha256,
                   script_body
            FROM scrape_script_revision
            WHERE target_id = ?1 AND revision_no = ?2
            "#,
                params![view.target_id, active_revision_no],
                |row| {
                    let language: String = row.get(2)?;
                    let entry_command_text: String = row.get(3)?;
                    let entry_command = serde_json::from_str::<Vec<String>>(&entry_command_text)
                        .unwrap_or_else(|_| default_entry_command(&language));
                    Ok((
                        ScrapeScriptRevisionRecord {
                            revision_no: row.get(0)?,
                            script_path: row.get(1)?,
                            language,
                            entry_command,
                            script_sha256: row.get(4)?,
                        },
                        row.get::<_, String>(5)?,
                    ))
                },
            )
            .optional()
        })
        .transpose()?
        .flatten();
    let Some((mut script, script_body)) = script else {
        return Ok(None);
    };
    let workspace_root = resolve_registered_workspace(root, &view);
    view.workspace_dir = workspace_root.to_string_lossy().to_string();
    script.script_path = resolve_registered_script_path(
        root,
        &script.script_path,
        &script.script_sha256,
        &script_body,
    )?
    .to_string_lossy()
    .to_string();

    Ok(Some(RegisteredTarget {
        view,
        script,
        workspace_root,
    }))
}

pub(super) fn resolve_registered_script_path(
    root: &Path,
    stored_path: &str,
    expected_sha256: &str,
    script_body: &str,
) -> Result<PathBuf> {
    anyhow::ensure!(
        compute_sha256(script_body.trim()) == expected_sha256,
        "persisted scrape script body does not match registered SHA-256"
    );
    let resolved = resolve_input_path(root, stored_path);
    anyhow::ensure!(
        registered_script_matches(&resolved, expected_sha256),
        "registered scrape script is missing or does not match SHA-256: {}",
        resolved.display()
    );
    Ok(resolved)
}

pub(super) fn count_rows(conn: &Connection, table: &str) -> Result<i64> {
    let sql = format!("SELECT COUNT(*) FROM {table}");
    conn.query_row(&sql, [], |row| row.get::<_, i64>(0))
        .map_err(anyhow::Error::from)
}
