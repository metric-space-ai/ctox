// Origin: CTOX
// License: Apache-2.0

use super::store::{
    augment_modules_with_catalog_update_state, augment_modules_with_instance_visibility,
    business_os_module_allowlist, business_os_store_path,
    configured_business_users_projection_hash, load_marketplace_module_manifests,
    load_module_manifests, load_rxdb_collection_record, load_template_manifests, modified_at_ms,
    module_governance_map, module_version_states, modules_with_projected_lifecycle, now_ms,
    resolve_business_os_app_root, resolve_business_os_installed_app_root,
    rxdb_collection_table_name, rxdb_store_path, sqlite_table_exists,
    update_module_catalog_stamp_hash, BusinessOsSession, BusinessOsSessionUser, ModuleManifest,
    TemplateManifest, MODULE_CATALOG_SOURCE_STAMP_FILE_LIMIT,
};
use anyhow::Context;
use rusqlite::types::Value as SqlValue;
use rusqlite::{params, Connection, OpenFlags, OptionalExtension};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::fs;
use std::path::Path;
use std::time::Duration;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ModuleCatalogProjectionStamp {
    source_modules: ModuleCatalogFileTreeStamp,
    installed_modules: ModuleCatalogFileTreeStamp,
    local_modules: ModuleCatalogFileTreeStamp,
    templates: ModuleCatalogFileTreeStamp,
    store_hash: String,
    allowlist_hash: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ModuleCatalogFileTreeStamp {
    dir_exists: bool,
    file_count: usize,
    latest_modified_at_ms: u64,
    truncated: bool,
    content_hash: String,
}

pub(super) fn rxdb_module_catalog_status(root: &Path) -> Value {
    let path = rxdb_store_path(root);
    if !path.is_file() {
        return serde_json::json!({
            "ok": false,
            "path": path.display().to_string(),
            "reason": "native RxDB SQLite store is missing",
        });
    }
    let conn = match Connection::open_with_flags(&path, OpenFlags::SQLITE_OPEN_READ_ONLY) {
        Ok(conn) => conn,
        Err(err) => {
            return serde_json::json!({
                "ok": false,
                "path": path.display().to_string(),
                "reason": format!("open native RxDB SQLite store: {err}"),
            });
        }
    };
    let _ = conn.busy_timeout(Duration::from_millis(100));
    let path = rxdb_store_path(root);
    let Some(table) = rxdb_collection_table_name(&path, &conn, "business_module_catalog") else {
        return serde_json::json!({
            "ok": false,
            "path": path.display().to_string(),
            "reason": "business_module_catalog RxDB collection table is missing",
        });
    };
    let data = match conn
        .query_row(
            &format!("SELECT data FROM {table} WHERE id = 'module-catalog'"),
            [],
            |row| row.get::<_, String>(0),
        )
        .optional()
    {
        Ok(Some(data)) => data,
        Ok(None) => {
            return serde_json::json!({
                "ok": false,
                "path": path.display().to_string(),
                "table": table,
                "reason": "module-catalog document is missing",
            });
        }
        Err(err) => {
            return serde_json::json!({
                "ok": false,
                "path": path.display().to_string(),
                "table": table,
                "reason": format!("read module-catalog document: {err}"),
            });
        }
    };
    let parsed = serde_json::from_str::<Value>(&data).unwrap_or(Value::Null);
    let module_count = parsed
        .get("modules")
        .and_then(Value::as_array)
        .map(Vec::len)
        .unwrap_or(0);
    serde_json::json!({
        "ok": module_count > 0,
        "path": path.display().to_string(),
        "table": table,
        "document_id": "module-catalog",
        "module_count": module_count,
        "template_count": parsed
            .get("templates")
            .and_then(Value::as_array)
            .map(Vec::len)
            .unwrap_or(0),
        "updated_at_ms": parsed.get("updated_at_ms").cloned().unwrap_or(Value::Null),
    })
}

pub(crate) fn module_catalog_projection_stamp(
    root: &Path,
) -> anyhow::Result<ModuleCatalogProjectionStamp> {
    let app_root = resolve_business_os_app_root(root)?;
    let installed_app_root = resolve_business_os_installed_app_root(root);
    Ok(ModuleCatalogProjectionStamp {
        source_modules: module_catalog_file_tree_stamp(&app_root.join("modules"))?,
        installed_modules: module_catalog_file_tree_stamp(
            &installed_app_root.join("installed-modules"),
        )?,
        local_modules: module_catalog_file_tree_stamp(&installed_app_root.join("local-modules"))?,
        templates: module_catalog_file_tree_stamp(&app_root.join("template-store"))?,
        store_hash: module_catalog_store_hash(root)?,
        allowlist_hash: module_catalog_allowlist_hash(root),
    })
}

fn module_catalog_file_tree_stamp(root: &Path) -> anyhow::Result<ModuleCatalogFileTreeStamp> {
    let dir_exists = root.is_dir();
    if !dir_exists {
        return Ok(ModuleCatalogFileTreeStamp {
            dir_exists: false,
            file_count: 0,
            latest_modified_at_ms: 0,
            truncated: false,
            content_hash: String::new(),
        });
    }

    let mut hasher = Sha256::new();
    let mut file_count = 0usize;
    let mut latest_modified_at_ms = 0u64;
    let mut truncated = false;
    collect_module_catalog_file_tree_stamp(
        root,
        root,
        &mut hasher,
        &mut file_count,
        &mut latest_modified_at_ms,
        &mut truncated,
    )?;
    Ok(ModuleCatalogFileTreeStamp {
        dir_exists: true,
        file_count,
        latest_modified_at_ms,
        truncated,
        content_hash: format!("{:x}", hasher.finalize()),
    })
}

fn collect_module_catalog_file_tree_stamp(
    root: &Path,
    current: &Path,
    hasher: &mut Sha256,
    file_count: &mut usize,
    latest_modified_at_ms: &mut u64,
    truncated: &mut bool,
) -> anyhow::Result<()> {
    if *truncated {
        return Ok(());
    }
    let mut entries = fs::read_dir(current)
        .with_context(|| format!("read module catalog stamp dir {}", current.display()))?
        .collect::<Result<Vec<_>, _>>()
        .with_context(|| format!("list module catalog stamp dir {}", current.display()))?;
    entries.sort_by(|left, right| left.path().cmp(&right.path()));
    for entry in entries {
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if name.starts_with('.')
            || matches!(name.as_ref(), "node_modules" | "dist" | "build" | "target")
        {
            continue;
        }
        let path = entry.path();
        let file_type = entry.file_type()?;
        if file_type.is_dir() {
            collect_module_catalog_file_tree_stamp(
                root,
                &path,
                hasher,
                file_count,
                latest_modified_at_ms,
                truncated,
            )?;
            if *truncated {
                return Ok(());
            }
            continue;
        }
        if !file_type.is_file() {
            continue;
        }
        if *file_count >= MODULE_CATALOG_SOURCE_STAMP_FILE_LIMIT {
            *truncated = true;
            return Ok(());
        }
        let metadata = fs::metadata(&path)?;
        let modified_at_ms = modified_at_ms(&metadata);
        let rel = path
            .strip_prefix(root)
            .unwrap_or(&path)
            .to_string_lossy()
            .replace('\\', "/");
        *file_count += 1;
        *latest_modified_at_ms = (*latest_modified_at_ms).max(modified_at_ms);
        update_module_catalog_stamp_hash(hasher, &rel);
        hasher.update(metadata.len().to_le_bytes());
        hasher.update(modified_at_ms.to_le_bytes());
    }
    Ok(())
}

fn module_catalog_store_hash(root: &Path) -> anyhow::Result<String> {
    let path = business_os_store_path(root);
    let mut hasher = Sha256::new();
    hasher.update(u8::from(path.exists()).to_le_bytes());
    update_module_catalog_stamp_hash(&mut hasher, &configured_business_users_projection_hash());
    if !path.exists() {
        return Ok(format!("{:x}", hasher.finalize()));
    }

    let conn = Connection::open_with_flags(
        &path,
        OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )
    .with_context(|| {
        format!(
            "open Business OS store for module catalog stamp {}",
            path.display()
        )
    })?;
    conn.busy_timeout(crate::persistence::sqlite_busy_timeout_duration())
        .context("configure module catalog stamp busy_timeout")?;

    for (table, query) in [
        (
            "business_users",
            "SELECT user_id, display_name, role, active, created_at_ms, updated_at_ms
             FROM business_users
             ORDER BY user_id ASC",
        ),
        (
            "business_module_acl",
            "SELECT module_id, user_id, role, active, created_at_ms, updated_at_ms
             FROM business_module_acl
             ORDER BY module_id ASC, user_id ASC, role ASC",
        ),
        (
            "business_permission_grants",
            "SELECT grant_id, subject_type, subject_id, permission, scope_type, scope_id,
                    active, reason, created_by, created_at_ms, updated_at_ms
             FROM business_permission_grants
             ORDER BY grant_id ASC",
        ),
        (
            "business_module_versions",
            "SELECT version_id, module_id, seq, origin, label, bundle_sha256, files_json,
                    sealed, created_by, created_at_ms, updated_at_ms
             FROM business_module_versions
             ORDER BY module_id ASC, seq ASC, version_id ASC",
        ),
        (
            "business_module_releases",
            "SELECT version_id, module_id, version, status, manifest_json, snapshot_json,
                    created_by, created_at_ms, notes
             FROM business_module_releases
             ORDER BY module_id ASC, version DESC, version_id ASC",
        ),
    ] {
        hash_module_catalog_query(&conn, &mut hasher, table, query)?;
    }

    Ok(format!("{:x}", hasher.finalize()))
}

fn hash_module_catalog_query(
    conn: &Connection,
    hasher: &mut Sha256,
    table: &str,
    query: &str,
) -> anyhow::Result<()> {
    update_module_catalog_stamp_hash(hasher, table);
    if !sqlite_table_exists(conn, table)? {
        hasher.update(0u8.to_le_bytes());
        return Ok(());
    }
    hasher.update(1u8.to_le_bytes());
    let mut stmt = conn
        .prepare(query)
        .with_context(|| format!("prepare module catalog stamp query for {table}"))?;
    let column_count = stmt.column_count();
    let mut rows = stmt
        .query([])
        .with_context(|| format!("query module catalog stamp table {table}"))?;
    let mut row_count = 0usize;
    while let Some(row) = rows
        .next()
        .with_context(|| format!("read module catalog stamp table {table}"))?
    {
        row_count += 1;
        hasher.update(row_count.to_le_bytes());
        for column in 0..column_count {
            let value: SqlValue = row.get(column)?;
            update_module_catalog_sql_value_hash(hasher, value);
        }
    }
    hasher.update(row_count.to_le_bytes());
    Ok(())
}

fn update_module_catalog_sql_value_hash(hasher: &mut Sha256, value: SqlValue) {
    match value {
        SqlValue::Null => {
            hasher.update([0]);
        }
        SqlValue::Integer(value) => {
            hasher.update([1]);
            hasher.update(value.to_le_bytes());
        }
        SqlValue::Real(value) => {
            hasher.update([2]);
            hasher.update(value.to_le_bytes());
        }
        SqlValue::Text(value) => {
            hasher.update([3]);
            update_module_catalog_stamp_hash(hasher, &value);
        }
        SqlValue::Blob(value) => {
            hasher.update([4]);
            hasher.update(value.len().to_le_bytes());
            hasher.update(value);
        }
    }
}

fn module_catalog_allowlist_hash(root: &Path) -> String {
    let mut hasher = Sha256::new();
    for id in business_os_module_allowlist(root) {
        update_module_catalog_stamp_hash(&mut hasher, &id);
    }
    format!("{:x}", hasher.finalize())
}

fn module_catalog_projection_service_session() -> BusinessOsSession {
    BusinessOsSession {
        ok: true,
        authenticated: true,
        auth_required: false,
        user: Some(BusinessOsSessionUser {
            id: "service:module-catalog-projection".to_owned(),
            display_name: "Module Catalog Projection Service".to_owned(),
            role: "service".to_owned(),
            is_admin: false,
        }),
        login_url: None,
        reason: None,
    }
}

pub fn module_catalog_for_rxdb(root: &Path) -> anyhow::Result<Value> {
    let app_root = resolve_business_os_app_root(root)?;
    let installed_app_root = resolve_business_os_installed_app_root(root);
    let modules = load_module_manifests(root, &app_root, &installed_app_root)?;
    let marketplace = load_marketplace_module_manifests(root, &app_root)?;
    let templates = load_template_manifests(&app_root)?;
    let mut governance = module_governance_map(root, &module_catalog_projection_service_session())?;
    let version_states = module_version_states(root, &app_root).unwrap_or(Value::Null);
    let (mut modules, lifecycle) =
        modules_with_projected_lifecycle(root, modules, &version_states)?;
    augment_modules_with_catalog_update_state(root, &app_root, &mut modules, &version_states);
    augment_modules_with_instance_visibility(root, &installed_app_root, &mut modules);
    if let Some(object) = governance.as_object_mut() {
        object.insert("lifecycle".to_owned(), lifecycle);
    }
    Ok(serde_json::json!({
        "id": "module-catalog",
        "ok": true,
        "modules": modules,
        "marketplace": marketplace,
        "templates": templates,
        "governance": governance,
        "version_states": version_states,
        // Per-instance allowlist that rides the RxDB data plane. Empty = no restriction.
        // The shell intersects its merged module list with this set when non-empty.
        "allowed_module_ids": business_os_module_allowlist(root),
        "updated_at_ms": now_ms(),
        "_deleted": false,
    }))
}

pub fn write_module_catalog_projection_to_rxdb(root: &Path) -> anyhow::Result<()> {
    refresh_module_catalog_after_schema_change(|| write_module_catalog_projection_once(root))
}

fn refresh_module_catalog_after_schema_change(
    mut refresh: impl FnMut() -> anyhow::Result<()>,
) -> anyhow::Result<()> {
    let mut schema_retries = 0;
    loop {
        match refresh() {
            Err(err)
                if schema_retries < 2
                    && err.chain().any(|cause| {
                        matches!(
                            cause.downcast_ref::<rusqlite::Error>(),
                            Some(rusqlite::Error::SqliteFailure(code, _))
                                if code.code == rusqlite::ErrorCode::SchemaChanged
                        )
                    }) =>
            {
                // Native peer collection registration can invalidate a catalog
                // statement at startup. Drop this attempt's connections and
                // prepare the complete idempotent projection again; do not
                // reuse the invalid statement or retry unrelated SQL errors.
                schema_retries += 1;
                std::thread::sleep(Duration::from_millis(50));
            }
            result => return result,
        }
    }
}

fn write_module_catalog_projection_once(root: &Path) -> anyhow::Result<()> {
    let mut document = module_catalog_for_rxdb(root)?;
    let now = now_ms();
    let revision = format!("{now}-ctox-module-catalog");
    if let Some(object) = document.as_object_mut() {
        // This writer intentionally bypasses RxCollection because it is also
        // used during Business OS bring-up, before the native peer is ready.
        // Keep the serialized data envelope in lockstep with the SQLite
        // revision/lwt columns: RxCollection's document cache reads these
        // fields from `data` and rejects legacy bare rows with DOC_CACHE_REV.
        object.insert("_rev".to_string(), Value::String(revision.clone()));
        object.insert(
            "_meta".to_string(),
            serde_json::json!({ "lwt": now as f64 }),
        );
        object.insert("_attachments".to_string(), serde_json::json!({}));
        object.insert("_deleted".to_string(), Value::Bool(false));
        object.insert("is_deleted".to_string(), Value::Bool(false));
    }
    let path = rxdb_store_path(root);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("failed to create RxDB runtime dir {}", parent.display()))?;
    }
    let conn =
        Connection::open(&path).with_context(|| format!("failed to open {}", path.display()))?;
    conn.busy_timeout(std::time::Duration::from_secs(10))?;
    conn.execute_batch(
        r#"
        PRAGMA journal_mode = WAL;
        PRAGMA busy_timeout = 10000;
        CREATE TABLE IF NOT EXISTS "ctox_business_os__business_module_catalog__v0"(
            id TEXT NOT NULL PRIMARY KEY UNIQUE,
            revision TEXT,
            deleted INTEGER NOT NULL CHECK (deleted IN (0, 1)),
            lastWriteTime REAL NOT NULL,
            data TEXT NOT NULL
        );
        CREATE INDEX IF NOT EXISTS "ctox_business_os__business_module_catalog__v0_lwt_id_idx"
            ON "ctox_business_os__business_module_catalog__v0"(lastWriteTime, id);
        CREATE INDEX IF NOT EXISTS "ctox_business_os__business_module_catalog__v0_deleted_lwt_id_idx"
            ON "ctox_business_os__business_module_catalog__v0"(deleted, lastWriteTime, id);
        "#,
    )?;
    conn.execute(
        r#"
        INSERT INTO "ctox_business_os__business_module_catalog__v0"
            (id, revision, deleted, lastWriteTime, data)
        VALUES ('module-catalog', ?1, 0, ?2, ?3)
        ON CONFLICT(id) DO UPDATE SET
            revision = excluded.revision,
            deleted = 0,
            lastWriteTime = excluded.lastWriteTime,
            data = excluded.data
        "#,
        params![revision, now as f64, serde_json::to_string(&document)?],
    )?;
    Ok(())
}

pub(super) fn write_module_catalog_projection_to_rxdb_for_module(
    root: &Path,
    module_id: &str,
) -> anyhow::Result<()> {
    write_module_catalog_projection_to_rxdb(root).with_context(|| {
        format!("failed to refresh Business OS module catalog after validating `{module_id}`")
    })?;
    let catalog = load_rxdb_collection_record(root, "business_module_catalog", "module-catalog")?;
    let catalog = catalog
        .context("Business OS module catalog projection did not create module-catalog record")?;
    let present = catalog
        .get("modules")
        .and_then(Value::as_array)
        .is_some_and(|modules| {
            modules
                .iter()
                .any(|module| module.get("id").and_then(Value::as_str) == Some(module_id))
        });
    anyhow::ensure!(
        present,
        "Business OS module catalog projection does not contain validated module `{module_id}`"
    );
    Ok(())
}

pub(super) fn module_release_ids_for_projection_repair(
    conn: &Connection,
    module_id: &str,
) -> anyhow::Result<Vec<String>> {
    let mut stmt = conn.prepare(
        "SELECT version_id
         FROM business_module_releases
         WHERE module_id = ?1
         ORDER BY version DESC",
    )?;
    let rows = stmt.query_map(params![module_id], |row| row.get::<_, String>(0))?;
    Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
}

#[cfg(test)]
mod tests {
    use super::super::policy::BusinessOsPermission;
    use super::super::store::tests::{
        chef_session, locked_inventory_data_review, seed_module_founder_acl, seed_permission_grant,
        seed_test_business_os_app_root, write_installed_inventory_module,
    };
    use super::super::store::{
        legacy_preview_audience_grant_id, load_rxdb_collection_record,
        migrate_legacy_module_lifecycle_authority, now_ms, open_store, record_module_release,
        resolve_business_os_installed_app_root, rollback_module_release, rxdb_store_path,
        ModuleReleaseRequest, ModuleRollbackRequest,
    };
    use super::{
        module_catalog_for_rxdb, refresh_module_catalog_after_schema_change,
        write_module_catalog_projection_once, write_module_catalog_projection_to_rxdb,
    };

    use anyhow::Context;
    use rusqlite::{params, Connection};
    use serde_json::Value;
    use std::fs;
    use tempfile::tempdir;

    #[test]
    fn module_catalog_refresh_recovers_a_real_sqlite_schema_invalidation() -> anyhow::Result<()> {
        let temp = tempdir()?;
        let root = temp.path();
        let schema_path = root.join("schema-race.sqlite");
        let reader = Connection::open(&schema_path)?;
        reader.execute_batch("CREATE TABLE schema_fixture(value TEXT)")?;
        let sql = std::ffi::CString::new("SELECT value FROM schema_fixture")?;
        let mut statement = std::ptr::null_mut();
        let prepared = unsafe {
            rusqlite::ffi::sqlite3_prepare(
                reader.handle(),
                sql.as_ptr(),
                -1,
                &mut statement,
                std::ptr::null_mut(),
            )
        };
        assert_eq!(prepared, rusqlite::ffi::SQLITE_OK);
        let registrar = Connection::open(&schema_path)?;
        registrar.execute_batch("CREATE TABLE registered_collection(id TEXT)")?;
        let (stepped, reset) = unsafe {
            let stepped = rusqlite::ffi::sqlite3_step(statement);
            let reset = rusqlite::ffi::sqlite3_reset(statement);
            rusqlite::ffi::sqlite3_finalize(statement);
            (stepped, reset)
        };
        assert_eq!(stepped, rusqlite::ffi::SQLITE_ERROR);
        assert_eq!(reset, rusqlite::ffi::SQLITE_SCHEMA);
        let mut invalidated = Some(rusqlite::Error::SqliteFailure(
            rusqlite::ffi::Error::new(reset),
            None,
        ));

        let module_dir = root.join("src/apps/business-os/modules/ctox");
        fs::create_dir_all(&module_dir)?;
        fs::write(
            root.join("src/apps/business-os/index.html"),
            "<!doctype html>",
        )?;
        fs::write(
            module_dir.join("module.json"),
            r#"{"id":"ctox","title":"CTOX","entry":"modules/ctox/index.html","install_scope":"core"}"#,
        )?;
        let mut attempts = 0;
        refresh_module_catalog_after_schema_change(|| {
            attempts += 1;
            if let Some(err) = invalidated.take() {
                return Err(anyhow::Error::new(err).context("catalog schema registration race"));
            }
            write_module_catalog_projection_once(root)
        })?;
        assert_eq!(attempts, 2);
        let catalog =
            load_rxdb_collection_record(root, "business_module_catalog", "module-catalog")?
                .context("recovered catalog must be persisted")?;
        assert!(catalog["modules"]
            .as_array()
            .unwrap()
            .iter()
            .any(|module| module["id"] == "ctox"));
        Ok(())
    }

    #[test]
    fn module_catalog_schema_recovery_is_bounded_and_preserves_other_errors() {
        for (code, expected_attempts) in [
            (rusqlite::ffi::SQLITE_SCHEMA, 3),
            (rusqlite::ffi::SQLITE_BUSY, 1),
        ] {
            let mut attempts = 0;
            let err = refresh_module_catalog_after_schema_change(|| {
                attempts += 1;
                Err(anyhow::Error::new(rusqlite::Error::SqliteFailure(
                    rusqlite::ffi::Error::new(code),
                    None,
                ))
                .context("retained catalog failure"))
            })
            .unwrap_err();
            assert_eq!(attempts, expected_attempts);
            assert_eq!(
                err.downcast_ref::<rusqlite::Error>()
                    .unwrap()
                    .sqlite_error()
                    .unwrap()
                    .extended_code,
                code
            );
            assert!(err.to_string().contains("retained catalog failure"));
        }
        let mut attempts = 0;
        assert!(refresh_module_catalog_after_schema_change(|| {
            attempts += 1;
            anyhow::bail!("database schema has changed")
        })
        .is_err());
        assert_eq!(attempts, 1, "error text alone must never authorize retry");
    }

    #[test]
    fn direct_module_catalog_projection_includes_installed_modules() -> anyhow::Result<()> {
        let temp = tempdir()?;
        let root = temp.path();
        let app_root = root.join("src/apps/business-os");
        let installed_app_root = resolve_business_os_installed_app_root(root);
        fs::create_dir_all(app_root.join("modules/ctox"))?;
        fs::create_dir_all(installed_app_root.join("installed-modules/research"))?;
        fs::write(app_root.join("index.html"), "<!doctype html>")?;
        fs::write(
            app_root.join("modules/ctox/module.json"),
            r#"{"id":"ctox","title":"CTOX","entry":"modules/ctox/index.html","install_scope":"core"}"#,
        )?;
        fs::write(
            installed_app_root.join("installed-modules/research/module.json"),
            r#"{"id":"research","title":"Web Research","entry":"installed-modules/research/index.html","install_scope":"installed"}"#,
        )?;
        fs::write(
            installed_app_root.join("installed-modules/research/icon.svg"),
            r#"<svg xmlns="http://www.w3.org/2000/svg"></svg>"#,
        )?;

        write_module_catalog_projection_to_rxdb(root)?;

        let conn = Connection::open(rxdb_store_path(root))?;
        let (stored_revision, stored_lwt, catalog_json): (String, f64, String) = conn.query_row(
            "SELECT revision, lastWriteTime, data FROM ctox_business_os__business_module_catalog__v0 WHERE id = 'module-catalog'",
            [],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )?;
        let catalog: Value = serde_json::from_str(&catalog_json)?;
        assert_eq!(
            catalog.get("_rev").and_then(Value::as_str),
            Some(stored_revision.as_str()),
            "direct SQL writer must keep the RxDB revision envelope in data"
        );
        assert_eq!(
            catalog.pointer("/_meta/lwt").and_then(Value::as_f64),
            Some(stored_lwt),
            "direct SQL writer must keep the RxDB lwt envelope in data"
        );
        assert!(
            catalog.get("_attachments").is_some_and(Value::is_object),
            "direct SQL writer must materialize the RxDB attachments envelope"
        );
        let ids = catalog
            .get("modules")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default()
            .into_iter()
            .filter_map(|module| module.get("id").and_then(Value::as_str).map(str::to_owned))
            .collect::<Vec<_>>();
        assert!(ids.contains(&"ctox".to_owned()), "missing ctox: {ids:?}");
        assert!(
            ids.contains(&"research".to_owned()),
            "missing installed research: {ids:?}"
        );
        let research = catalog
            .get("modules")
            .and_then(Value::as_array)
            .and_then(|modules| {
                modules
                    .iter()
                    .find(|module| module.get("id").and_then(Value::as_str) == Some("research"))
            })
            .expect("installed research module missing from projected catalog");
        assert_eq!(
            research.get("icon").and_then(Value::as_str),
            Some("icon.svg")
        );
        Ok(())
    }

    #[test]
    fn module_catalog_projects_file_plane_declarations_from_schema() -> anyhow::Result<()> {
        let temp = tempdir()?;
        let root = temp.path();
        let app_root = root.join("src/apps/business-os");
        let installed_app_root = resolve_business_os_installed_app_root(root);
        let module_dir = installed_app_root.join("installed-modules/dynamic-files");
        fs::create_dir_all(app_root.join("modules/ctox"))?;
        fs::create_dir_all(&module_dir)?;
        fs::write(app_root.join("index.html"), "<!doctype html>")?;
        fs::write(
            app_root.join("modules/ctox/module.json"),
            r#"{"id":"ctox","title":"CTOX","entry":"modules/ctox/index.html","install_scope":"core"}"#,
        )?;
        fs::write(
            module_dir.join("module.json"),
            r#"{"id":"dynamic-files","title":"Dynamic Files","entry":"installed-modules/dynamic-files/index.html","install_scope":"installed","collections":["business_commands","dynamic_files","dynamic_file_chunks"]}"#,
        )?;
        fs::write(
            module_dir.join("collections.schema.json"),
            serde_json::to_vec_pretty(&serde_json::json!({
                "schema_format": "ctox-business-os-module-collections-v1",
                "collections": {
                    "dynamic_files": {
                        "schema": {
                            "version": 0,
                            "type": "object",
                            "primaryKey": "id",
                            "properties": {"id": {"type": "string", "maxLength": 128}},
                            "required": ["id"]
                        }
                    },
                    "dynamic_file_chunks": {
                        "schema": {
                            "version": 0,
                            "type": "object",
                            "primaryKey": "id",
                            "properties": {
                                "id": {"type": "string", "maxLength": 180},
                                "blob_id": {"type": "string"},
                                "idx": {"type": "number"},
                                "data": {"type": "string"}
                            },
                            "required": ["id", "blob_id", "idx", "data"]
                        }
                    }
                },
                "file_plane": {
                    "declarations": [{
                        "role": "file-chunks",
                        "request_collection": "dynamic_files",
                        "storage_collection": "dynamic_file_chunks",
                        "key_field": "blob_id",
                        "content_hash_field": "content_hash",
                        "chunk_index_field": "idx"
                    }]
                }
            }))?,
        )?;

        let catalog = module_catalog_for_rxdb(root)?;
        let module = catalog
            .get("modules")
            .and_then(Value::as_array)
            .and_then(|modules| {
                modules.iter().find(|module| {
                    module.get("id").and_then(Value::as_str) == Some("dynamic-files")
                })
            })
            .expect("dynamic-files module must be projected");
        let declaration = module
            .pointer("/file_plane/declarations/0")
            .expect("file-plane declaration must be projected");
        assert_eq!(
            declaration
                .get("storage_collection")
                .and_then(Value::as_str),
            Some("dynamic_file_chunks")
        );
        assert_eq!(
            declaration.get("key_field").and_then(Value::as_str),
            Some("blob_id")
        );
        Ok(())
    }

    #[test]
    fn module_catalog_keeps_uninstalled_research_in_marketplace() -> anyhow::Result<()> {
        let temp = tempdir()?;
        let root = temp.path();
        let app_root = root.join("src/apps/business-os");
        fs::create_dir_all(app_root.join("modules/ctox"))?;
        fs::create_dir_all(app_root.join("modules/research"))?;
        fs::write(app_root.join("index.html"), "<!doctype html>")?;
        fs::write(
            app_root.join("modules/ctox/module.json"),
            r#"{"id":"ctox","title":"CTOX","entry":"modules/ctox/index.html","install_scope":"core"}"#,
        )?;
        fs::write(
            app_root.join("modules/research/module.json"),
            r#"{"id":"research","title":"Web Research","entry":"modules/research/index.html","install_scope":"store"}"#,
        )?;

        write_module_catalog_projection_to_rxdb(root)?;

        let conn = Connection::open(rxdb_store_path(root))?;
        let catalog_json: String = conn.query_row(
            "SELECT data FROM ctox_business_os__business_module_catalog__v0 WHERE id = 'module-catalog'",
            [],
            |row| row.get(0),
        )?;
        let catalog: Value = serde_json::from_str(&catalog_json)?;
        assert!(!catalog
            .get("modules")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .any(|module| module.get("id").and_then(Value::as_str) == Some("research")));
        let research = catalog
            .get("marketplace")
            .and_then(Value::as_array)
            .and_then(|modules| {
                modules
                    .iter()
                    .find(|module| module.get("id").and_then(Value::as_str) == Some("research"))
            })
            .expect("research store app missing from projected marketplace");
        assert_eq!(
            research.get("install_scope").and_then(Value::as_str),
            Some("store")
        );
        assert_eq!(
            research.get("entry").and_then(Value::as_str),
            Some("modules/research/index.html")
        );
        Ok(())
    }

    #[test]
    fn module_catalog_asset_revision_tracks_module_asset_bytes() -> anyhow::Result<()> {
        let temp = tempdir()?;
        let root = temp.path();
        let app_root = root.join("src/apps/business-os");
        let module_dir = app_root.join("modules/ctox");
        fs::create_dir_all(&module_dir)?;
        fs::write(app_root.join("index.html"), "<!doctype html>")?;
        fs::write(
            module_dir.join("module.json"),
            r#"{"id":"ctox","title":"CTOX","entry":"modules/ctox/index.html","install_scope":"core","launch_kind":"desktop-app","presentation":{"default_mode":"window","supported_modes":["window","maximized","focus"],"initial_size":{"width":1280,"height":820},"minimum_size":{"width":640,"height":480},"multi_instance":false,"auto_restore":false}}"#,
        )?;
        fs::write(module_dir.join("index.js"), "export const marker = 'one';")?;

        let first_catalog = module_catalog_for_rxdb(root)?;
        let first_module = first_catalog
            .get("modules")
            .and_then(Value::as_array)
            .and_then(|modules| {
                modules
                    .iter()
                    .find(|module| module.get("id").and_then(Value::as_str) == Some("ctox"))
            })
            .context("first ctox module projection")?;
        let first_revision = first_module
            .get("asset_revision")
            .and_then(Value::as_str)
            .context("first asset revision")?
            .to_owned();
        let manifest_sha = first_module
            .get("manifest_sha256")
            .and_then(Value::as_str)
            .context("manifest sha")?
            .to_owned();
        assert_eq!(
            first_module.get("launch_kind").and_then(Value::as_str),
            Some("desktop-app")
        );
        assert_eq!(
            first_module
                .get("presentation")
                .and_then(|value| value.get("minimum_size"))
                .and_then(|value| value.get("width"))
                .and_then(Value::as_u64),
            Some(640)
        );

        fs::write(module_dir.join("index.js"), "export const marker = 'two';")?;

        let second_catalog = module_catalog_for_rxdb(root)?;
        let second_module = second_catalog
            .get("modules")
            .and_then(Value::as_array)
            .and_then(|modules| {
                modules
                    .iter()
                    .find(|module| module.get("id").and_then(Value::as_str) == Some("ctox"))
            })
            .context("second ctox module projection")?;
        let second_revision = second_module
            .get("asset_revision")
            .and_then(Value::as_str)
            .context("second asset revision")?;

        assert_ne!(first_revision, second_revision);
        assert_eq!(
            second_module.get("manifest_sha256").and_then(Value::as_str),
            Some(manifest_sha.as_str())
        );
        Ok(())
    }

    #[test]
    fn module_catalog_does_not_install_or_grant_uninstalled_store_apps() -> anyhow::Result<()> {
        let temp = tempdir()?;
        let root = temp.path();
        let app_root = root.join("src/apps/business-os");
        fs::create_dir_all(app_root.join("modules/ctox"))?;
        fs::create_dir_all(app_root.join("modules/cv-print-builder"))?;
        fs::write(app_root.join("index.html"), "<!doctype html>")?;
        fs::write(
            app_root.join("modules/ctox/module.json"),
            r#"{"id":"ctox","title":"CTOX","entry":"modules/ctox/index.html","install_scope":"core"}"#,
        )?;
        fs::write(
            app_root.join("modules/cv-print-builder/module.json"),
            r#"{"id":"cv-print-builder","title":"CV Print Builder","entry":"modules/cv-print-builder/index.html","install_scope":"store","collections":["business_commands","business_chats","ctox_queue_tasks","desktop_files","desktop_file_chunks","documents","document_versions"]}"#,
        )?;

        let catalog = module_catalog_for_rxdb(root)?;
        let modules = catalog
            .get("modules")
            .and_then(Value::as_array)
            .context("catalog modules")?;
        assert!(!modules.iter().any(|module| {
            module.get("id").and_then(Value::as_str) == Some("cv-print-builder")
        }));
        assert!(catalog
            .get("marketplace")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .any(|module| {
                module.get("id").and_then(Value::as_str) == Some("cv-print-builder")
                    && module.get("install_scope").and_then(Value::as_str) == Some("store")
            }));

        let conn = open_store(root)?;
        let count: i64 = conn.query_row(
            "SELECT COUNT(*)
             FROM business_permission_grants
             WHERE active = 1
               AND subject_type = 'role'
               AND subject_id IN ('user', 'founder')
               AND permission IN (?1, ?2)
               AND scope_type = 'module'
               AND scope_id = 'cv-print-builder'",
            params![
                BusinessOsPermission::DataRead.as_str(),
                BusinessOsPermission::DataWrite.as_str()
            ],
            |row| row.get(0),
        )?;
        assert_eq!(count, 0);
        drop(conn);

        let catalog_again = module_catalog_for_rxdb(root)?;
        let conn = open_store(root)?;
        let count_again: i64 = conn.query_row(
            "SELECT COUNT(*)
             FROM business_permission_grants
             WHERE active = 1
               AND subject_type = 'role'
               AND subject_id IN ('user', 'founder')
               AND permission IN (?1, ?2)
               AND scope_type = 'module'
               AND scope_id = 'cv-print-builder'",
            params![
                BusinessOsPermission::DataRead.as_str(),
                BusinessOsPermission::DataWrite.as_str()
            ],
            |row| row.get(0),
        )?;
        assert_eq!(count_again, 0);

        let explicit_grants = catalog_again
            .pointer("/governance/permission_model/explicit_grants")
            .and_then(Value::as_array)
            .context("explicit grants projection")?;
        assert!(!explicit_grants.iter().any(|grant| {
            grant.get("subject_type").and_then(Value::as_str) == Some("role")
                && grant.get("scope_type").and_then(Value::as_str) == Some("module")
                && grant.get("scope_id").and_then(Value::as_str) == Some("cv-print-builder")
        }));

        Ok(())
    }

    #[test]
    fn module_catalog_prefers_release_source_over_stale_runtime_app_root() -> anyhow::Result<()> {
        let temp = tempdir()?;
        let release_root = temp.path().join("release");
        let runtime_root = release_root.join("runtime");
        let stale_app_root = runtime_root.join("business-os");
        let release_app_root = release_root.join("src/apps/business-os");

        fs::create_dir_all(stale_app_root.join("modules/ctox"))?;
        fs::create_dir_all(release_app_root.join("modules/ctox"))?;
        fs::create_dir_all(release_app_root.join("modules/research"))?;
        fs::write(stale_app_root.join("index.html"), "<!doctype html>")?;
        fs::write(release_app_root.join("index.html"), "<!doctype html>")?;
        fs::write(
            stale_app_root.join("modules/ctox/module.json"),
            r#"{"id":"ctox","title":"CTOX","entry":"modules/ctox/index.html","install_scope":"core"}"#,
        )?;
        fs::write(
            release_app_root.join("modules/ctox/module.json"),
            r#"{"id":"ctox","title":"CTOX","entry":"modules/ctox/index.html","install_scope":"core"}"#,
        )?;
        fs::write(
            release_app_root.join("modules/research/module.json"),
            r#"{"id":"research","title":"Web Research","entry":"modules/research/index.html","install_scope":"store"}"#,
        )?;

        let catalog = module_catalog_for_rxdb(&runtime_root)?;
        let marketplace = catalog
            .get("marketplace")
            .and_then(Value::as_array)
            .context("catalog marketplace")?;
        assert!(marketplace
            .iter()
            .any(|module| module.get("id").and_then(Value::as_str) == Some("research")));
        Ok(())
    }

    #[test]
    fn module_catalog_read_does_not_write_legacy_grants_or_release_records() -> anyhow::Result<()> {
        let temp = tempdir()?;
        let root = temp.path();
        let app_root = root.join("src/apps/business-os");
        fs::create_dir_all(root.join("runtime"))?;
        let installed_app_root = resolve_business_os_installed_app_root(root);
        fs::create_dir_all(app_root.join("modules/ctox"))?;
        fs::create_dir_all(installed_app_root.join("installed-modules/legacy-preview"))?;
        fs::create_dir_all(installed_app_root.join("installed-modules/legacy-team"))?;
        fs::write(app_root.join("index.html"), "<!doctype html>")?;
        fs::write(
            app_root.join("modules/ctox/module.json"),
            r#"{"id":"ctox","title":"CTOX","entry":"modules/ctox/index.html","install_scope":"core"}"#,
        )?;
        fs::write(
            installed_app_root.join("installed-modules/legacy-preview/module.json"),
            r#"{"id":"legacy-preview","title":"Legacy Preview","version":"0.4.0","entry":"installed-modules/legacy-preview/index.html","install_scope":"installed","lifecycle":{"visibility_state":"preview","preview_user_ids":["preview-user"]}}"#,
        )?;
        fs::write(
            installed_app_root.join("installed-modules/legacy-team/module.json"),
            r#"{"id":"legacy-team","title":"Legacy Team","version":"1.2.0","entry":"installed-modules/legacy-team/index.html","install_scope":"installed"}"#,
        )?;

        let counts = |conn: &Connection| -> anyhow::Result<(i64, i64)> {
            Ok((
                conn.query_row(
                    "SELECT COUNT(*) FROM business_permission_grants",
                    [],
                    |row| row.get(0),
                )?,
                conn.query_row("SELECT COUNT(*) FROM business_module_releases", [], |row| {
                    row.get(0)
                })?,
            ))
        };
        let conn = open_store(root)?;
        let before = counts(&conn)?;
        drop(conn);

        let catalog = module_catalog_for_rxdb(root)?;

        let conn = open_store(root)?;
        let after = counts(&conn)?;
        assert_eq!(
            after, before,
            "a catalog read must not create authority or releases"
        );
        assert_eq!(
            catalog
                .pointer("/governance/can_manage_all")
                .and_then(Value::as_bool),
            Some(false)
        );
        assert_eq!(
            catalog.pointer("/governance/role").and_then(Value::as_str),
            Some("service")
        );
        assert_eq!(
            catalog
                .pointer("/governance/user_id")
                .and_then(Value::as_str),
            Some("service:module-catalog-projection")
        );
        Ok(())
    }

    #[test]
    fn explicit_migration_projects_runtime_app_lifecycle_authority() -> anyhow::Result<()> {
        let temp = tempdir()?;
        let root = temp.path();
        let app_root = root.join("src/apps/business-os");
        let bootstrap_conn = open_store(root)?;
        drop(bootstrap_conn);
        let installed_app_root = resolve_business_os_installed_app_root(root);
        fs::create_dir_all(app_root.join("modules/ctox"))?;
        fs::create_dir_all(installed_app_root.join("installed-modules/private-zero"))?;
        fs::create_dir_all(installed_app_root.join("installed-modules/legacy-preview-zero"))?;
        fs::create_dir_all(installed_app_root.join("installed-modules/modify-only-zero"))?;
        fs::create_dir_all(installed_app_root.join("installed-modules/team-one"))?;
        fs::create_dir_all(installed_app_root.join("installed-modules/missing-version"))?;
        fs::create_dir_all(installed_app_root.join("installed-modules/invalid-semver"))?;
        fs::create_dir_all(installed_app_root.join("installed-modules/restricted-team"))?;
        fs::write(app_root.join("index.html"), "<!doctype html>")?;
        fs::write(
            app_root.join("modules/ctox/module.json"),
            r#"{"id":"ctox","title":"CTOX","entry":"modules/ctox/index.html","install_scope":"core"}"#,
        )?;
        fs::write(
            installed_app_root.join("installed-modules/private-zero/module.json"),
            r#"{"id":"private-zero","title":"Private Zero","version":"0.2.0","entry":"installed-modules/private-zero/index.html","install_scope":"installed"}"#,
        )?;
        fs::write(
            installed_app_root.join("installed-modules/legacy-preview-zero/module.json"),
            r#"{"id":"legacy-preview-zero","title":"Legacy Preview Zero","version":"0.4.0","entry":"installed-modules/legacy-preview-zero/index.html","install_scope":"installed","lifecycle":{"visibility_state":"preview","preview_user_ids":["legacy-preview-user","legacy-preview-user","legacy-pregranted-user"]}}"#,
        )?;
        fs::write(
            installed_app_root.join("installed-modules/modify-only-zero/module.json"),
            r#"{"id":"modify-only-zero","title":"Modify Only Zero","version":"0.3.0","entry":"installed-modules/modify-only-zero/index.html","install_scope":"installed"}"#,
        )?;
        fs::write(
            installed_app_root.join("installed-modules/team-one/module.json"),
            r#"{"id":"team-one","title":"Team One","version":"1.0.0","entry":"installed-modules/team-one/index.html","install_scope":"installed","lifecycle":{"preview_user_ids":["team-preview-user"]}}"#,
        )?;
        fs::write(
            installed_app_root.join("installed-modules/missing-version/module.json"),
            r#"{"id":"missing-version","title":"Missing Version","entry":"installed-modules/missing-version/index.html","install_scope":"installed","lifecycle":{"preview_user_ids":["missing-preview-user"]}}"#,
        )?;
        fs::write(
            installed_app_root.join("installed-modules/invalid-semver/module.json"),
            r#"{"id":"invalid-semver","title":"Invalid Semver","version":"v1.0.0","entry":"installed-modules/invalid-semver/index.html","install_scope":"installed","lifecycle":{"preview_user_ids":["invalid-preview-user"]}}"#,
        )?;
        fs::write(
            installed_app_root.join("installed-modules/restricted-team/module.json"),
            r#"{"id":"restricted-team","title":"Restricted Team","version":"1.1.0","entry":"installed-modules/restricted-team/index.html","install_scope":"installed","lifecycle":{"visibility_state":"restricted","audience":"restricted","preview_user_ids":["restricted-preview-user"]}}"#,
        )?;
        seed_module_founder_acl(root, "private-zero", "app-owner")?;
        seed_permission_grant(
            root,
            "grant_preview_private_zero",
            "user",
            "preview-user",
            BusinessOsPermission::AppsView,
            "module",
            "private-zero",
        )?;
        seed_permission_grant(
            root,
            "grant_modify_only_zero",
            "user",
            "modify-user",
            BusinessOsPermission::AppsModify,
            "module",
            "modify-only-zero",
        )?;
        seed_permission_grant(
            root,
            "grant_existing_legacy_preview",
            "user",
            "legacy-pregranted-user",
            BusinessOsPermission::AppsView,
            "module",
            "legacy-preview-zero",
        )?;

        let conn = open_store(root)?;
        let now = now_ms() as i64;
        conn.execute(
            "INSERT INTO business_module_versions
                (version_id, module_id, seq, origin, label, bundle_sha256, files_json,
                 sealed, created_by, created_at_ms, updated_at_ms)
             VALUES ('modver_private_zero_1', 'private-zero', 1, 'install', 'Installed',
                 'sha-private-zero', '[]', 1, 'creator-user', ?1, ?1)",
            params![now],
        )?;
        conn.execute(
            "INSERT INTO business_module_releases
                (version_id, module_id, version, status, manifest_json, snapshot_json,
                 created_by, created_at_ms, notes)
             VALUES ('modrel_team_one_1', 'team-one', 1, 'released',
                 '{\"id\":\"team-one\"}', '{}', 'release-owner', ?1, 'test release')",
            params![now + 1],
        )?;
        drop(conn);

        let migration = migrate_legacy_module_lifecycle_authority(root, &chef_session(), false)?;
        assert_eq!(
            migration.get("status").and_then(Value::as_str),
            Some("applied")
        );
        assert_eq!(
            migration
                .pointer("/evidence/preview_grants_inserted")
                .and_then(Value::as_u64),
            Some(2)
        );
        assert_eq!(
            migration
                .pointer("/evidence/release_records_inserted")
                .and_then(Value::as_u64),
            Some(1)
        );

        let catalog = module_catalog_for_rxdb(root)?;
        let modules = catalog
            .get("modules")
            .and_then(Value::as_array)
            .context("catalog modules")?;
        let module = |id: &str| -> &Value {
            modules
                .iter()
                .find(|module| module.get("id").and_then(Value::as_str) == Some(id))
                .unwrap_or_else(|| panic!("missing module {id}"))
        };

        assert_eq!(
            module("ctox")
                .pointer("/lifecycle/visibility_state")
                .and_then(Value::as_str),
            Some("packaged")
        );
        assert_eq!(
            module("private-zero")
                .pointer("/lifecycle/visibility_state")
                .and_then(Value::as_str),
            Some("preview")
        );
        assert_eq!(
            module("private-zero")
                .pointer("/lifecycle/current_semver")
                .and_then(Value::as_str),
            Some("0.2.0")
        );
        assert_eq!(
            module("private-zero")
                .pointer("/lifecycle/creator_user_id")
                .and_then(Value::as_str),
            Some("creator-user")
        );
        assert!(module("private-zero")
            .pointer("/lifecycle/responsible_user_ids")
            .and_then(Value::as_array)
            .unwrap()
            .iter()
            .any(|user| user.as_str() == Some("app-owner")));
        assert!(module("private-zero")
            .pointer("/lifecycle/preview_grant_ids")
            .and_then(Value::as_array)
            .unwrap()
            .iter()
            .any(|grant| grant.as_str() == Some("grant_preview_private_zero")));
        assert!(module("private-zero")
            .pointer("/lifecycle/preview_user_ids")
            .and_then(Value::as_array)
            .unwrap()
            .iter()
            .any(|user| user.as_str() == Some("preview-user")));
        let legacy_preview_grant_id =
            legacy_preview_audience_grant_id("legacy-preview-zero", "legacy-preview-user");
        assert_eq!(
            module("legacy-preview-zero")
                .pointer("/lifecycle/visibility_state")
                .and_then(Value::as_str),
            Some("preview")
        );
        assert!(module("legacy-preview-zero")
            .pointer("/lifecycle/preview_grant_ids")
            .and_then(Value::as_array)
            .unwrap()
            .iter()
            .any(|grant| grant.as_str() == Some(legacy_preview_grant_id.as_str())));
        assert_eq!(
            module("legacy-preview-zero")
                .pointer("/lifecycle/preview_user_ids")
                .and_then(Value::as_array)
                .unwrap()
                .iter()
                .filter(|user| user.as_str() == Some("legacy-preview-user"))
                .count(),
            1
        );
        assert!(module("legacy-preview-zero")
            .pointer("/lifecycle/preview_user_ids")
            .and_then(Value::as_array)
            .unwrap()
            .iter()
            .any(|user| user.as_str() == Some("legacy-pregranted-user")));
        assert!(module("legacy-preview-zero")
            .pointer("/lifecycle/preview_grant_ids")
            .and_then(Value::as_array)
            .unwrap()
            .iter()
            .any(|grant| grant.as_str() == Some("grant_existing_legacy_preview")));
        assert_eq!(
            module("modify-only-zero")
                .pointer("/lifecycle/visibility_state")
                .and_then(Value::as_str),
            Some("private")
        );
        assert!(module("modify-only-zero")
            .pointer("/lifecycle/preview_grant_ids")
            .and_then(Value::as_array)
            .unwrap()
            .is_empty());
        assert!(module("modify-only-zero")
            .pointer("/lifecycle/preview_user_ids")
            .and_then(Value::as_array)
            .unwrap()
            .is_empty());
        assert_eq!(
            module("team-one")
                .pointer("/lifecycle/visibility_state")
                .and_then(Value::as_str),
            Some("team")
        );
        assert_eq!(
            module("team-one")
                .pointer("/lifecycle/public")
                .and_then(Value::as_bool),
            Some(true)
        );
        assert_eq!(
            module("team-one")
                .pointer("/lifecycle/last_release_id")
                .and_then(Value::as_str),
            Some("modrel_team_one_1")
        );
        assert_eq!(
            module("missing-version")
                .pointer("/lifecycle/visibility_state")
                .and_then(Value::as_str),
            Some("private")
        );
        assert_eq!(
            module("missing-version")
                .pointer("/lifecycle/warning_code")
                .and_then(Value::as_str),
            Some("invalid_semver")
        );
        assert!(module("missing-version")
            .pointer("/lifecycle/preview_grant_ids")
            .and_then(Value::as_array)
            .unwrap()
            .is_empty());
        assert_eq!(
            module("invalid-semver")
                .pointer("/lifecycle/visibility_state")
                .and_then(Value::as_str),
            Some("private")
        );
        assert_eq!(
            module("invalid-semver").pointer("/lifecycle/current_semver"),
            Some(&Value::Null)
        );
        assert_eq!(
            module("invalid-semver")
                .pointer("/lifecycle/warning_code")
                .and_then(Value::as_str),
            Some("invalid_semver")
        );
        assert!(module("invalid-semver")
            .pointer("/lifecycle/preview_grant_ids")
            .and_then(Value::as_array)
            .unwrap()
            .is_empty());
        assert_eq!(
            module("restricted-team")
                .pointer("/lifecycle/visibility_state")
                .and_then(Value::as_str),
            Some("restricted")
        );
        assert_eq!(
            module("restricted-team")
                .pointer("/lifecycle/public")
                .and_then(Value::as_bool),
            Some(false)
        );
        let restricted_preview_grant_id =
            legacy_preview_audience_grant_id("restricted-team", "restricted-preview-user");
        assert!(module("restricted-team")
            .pointer("/lifecycle/preview_grant_ids")
            .and_then(Value::as_array)
            .unwrap()
            .iter()
            .any(|grant| grant.as_str() == Some(restricted_preview_grant_id.as_str())));
        assert_eq!(
            catalog
                .pointer("/governance/lifecycle/team-one/visibility_state")
                .and_then(Value::as_str),
            Some("team")
        );
        let conn = open_store(root)?;
        let legacy_grants: i64 = conn.query_row(
            "SELECT COUNT(*)
             FROM business_permission_grants
             WHERE grant_id = ?1
               AND subject_type = 'user'
               AND subject_id = 'legacy-preview-user'
               AND permission = ?2
               AND scope_type = 'module'
               AND scope_id = 'legacy-preview-zero'
               AND active = 1",
            params![
                legacy_preview_grant_id.as_str(),
                BusinessOsPermission::AppsView.as_str()
            ],
            |row| row.get(0),
        )?;
        assert_eq!(legacy_grants, 1);
        let generated_pregranted_grants: i64 = conn.query_row(
            "SELECT COUNT(*)
             FROM business_permission_grants
             WHERE grant_id = ?1",
            params![legacy_preview_audience_grant_id(
                "legacy-preview-zero",
                "legacy-pregranted-user"
            )],
            |row| row.get(0),
        )?;
        assert_eq!(generated_pregranted_grants, 0);
        let unexpected_legacy_visibility_grants: i64 = conn.query_row(
            "SELECT COUNT(*)
             FROM business_permission_grants
             WHERE permission = ?1
               AND scope_type = 'module'
               AND scope_id IN ('missing-version', 'invalid-semver', 'team-one')",
            params![BusinessOsPermission::AppsView.as_str()],
            |row| row.get(0),
        )?;
        assert_eq!(unexpected_legacy_visibility_grants, 0);
        let unexpected_data_grants: i64 = conn.query_row(
            "SELECT COUNT(*)
             FROM business_permission_grants
             WHERE permission IN (?1, ?2)",
            params![
                BusinessOsPermission::DataRead.as_str(),
                BusinessOsPermission::DataWrite.as_str()
            ],
            |row| row.get(0),
        )?;
        assert_eq!(unexpected_data_grants, 0);
        drop(conn);

        let _catalog_again = module_catalog_for_rxdb(root)?;
        let conn = open_store(root)?;
        let legacy_grants_after_second_projection: i64 = conn.query_row(
            "SELECT COUNT(*)
             FROM business_permission_grants
             WHERE subject_type = 'user'
               AND subject_id = 'legacy-preview-user'
               AND permission = ?1
               AND scope_type = 'module'
               AND scope_id = 'legacy-preview-zero'",
            params![BusinessOsPermission::AppsView.as_str()],
            |row| row.get(0),
        )?;
        assert_eq!(legacy_grants_after_second_projection, 1);
        let restricted_grants_after_second_projection: i64 = conn.query_row(
            "SELECT COUNT(*)
             FROM business_permission_grants
             WHERE grant_id = ?1
               AND subject_type = 'user'
               AND subject_id = 'restricted-preview-user'
               AND permission = ?2
               AND scope_type = 'module'
               AND scope_id = 'restricted-team'
               AND active = 1",
            params![
                restricted_preview_grant_id.as_str(),
                BusinessOsPermission::AppsView.as_str()
            ],
            |row| row.get(0),
        )?;
        assert_eq!(restricted_grants_after_second_projection, 1);
        drop(conn);
        let migration_again =
            migrate_legacy_module_lifecycle_authority(root, &chef_session(), false)?;
        assert_eq!(
            migration_again.get("status").and_then(Value::as_str),
            Some("already_applied")
        );
        assert_eq!(
            migration_again
                .pointer("/evidence/preview_grants_inserted")
                .and_then(Value::as_u64),
            Some(2)
        );
        Ok(())
    }

    #[test]
    fn module_catalog_projects_release_state_data_access_and_rollback_target() -> anyhow::Result<()>
    {
        let temp = tempdir()?;
        let root = temp.path();
        seed_test_business_os_app_root(root)?;
        let app_root = root.join("src/apps/business-os");
        fs::create_dir_all(root.join("runtime"))?;
        let installed_app_root = resolve_business_os_installed_app_root(root);
        write_installed_inventory_module(&installed_app_root, "0.9.0")?;

        let first_release = record_module_release(
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
        let first_release_id = first_release
            .get("version_id")
            .and_then(Value::as_str)
            .context("first release id")?
            .to_owned();

        let second_release = record_module_release(
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
                notes: "Second team release".to_owned(),
            },
        )?;
        let second_release_id = second_release
            .get("version_id")
            .and_then(Value::as_str)
            .context("second release id")?
            .to_owned();

        let projected_catalog =
            load_rxdb_collection_record(root, "business_module_catalog", "module-catalog")?
                .context("release must immediately write the RxDB module catalog")?;
        assert_eq!(
            projected_catalog
                .pointer("/modules")
                .and_then(Value::as_array)
                .and_then(|modules| modules.iter().find(|module| {
                    module.get("id").and_then(Value::as_str) == Some("inventory")
                }))
                .and_then(|module| module.get("version"))
                .and_then(Value::as_str),
            Some("1.1.0"),
            "terminal release state must not race a stale RxDB catalog projection",
        );

        let catalog = module_catalog_for_rxdb(root)?;
        let modules = catalog
            .get("modules")
            .and_then(Value::as_array)
            .context("catalog modules")?;
        let inventory = modules
            .iter()
            .find(|module| module.get("id").and_then(Value::as_str) == Some("inventory"))
            .context("inventory module projection")?;

        assert_eq!(
            inventory
                .pointer("/lifecycle/release_status")
                .and_then(Value::as_str),
            Some("released")
        );
        assert_eq!(
            inventory
                .pointer("/lifecycle/release_state/current/version_id")
                .and_then(Value::as_str),
            Some(second_release_id.as_str())
        );
        assert_eq!(
            inventory
                .pointer("/lifecycle/release_state/current/target_version")
                .and_then(Value::as_str),
            Some("1.1.0")
        );
        assert_eq!(
            inventory
                .pointer("/lifecycle/rollback_target/version_id")
                .and_then(Value::as_str),
            Some(first_release_id.as_str())
        );
        assert_eq!(
            inventory
                .pointer("/lifecycle/data_access/status")
                .and_then(Value::as_str),
            Some("reviewed")
        );
        assert_eq!(
            inventory
                .pointer("/lifecycle/data_access/areas/0/read")
                .and_then(Value::as_str),
            Some("locked")
        );
        assert_eq!(
            inventory
                .pointer("/lifecycle/data_access/areas/0/write")
                .and_then(Value::as_str),
            Some("locked")
        );
        assert_eq!(
            inventory
                .pointer("/lifecycle/data_access/locked_collection_ids/0")
                .and_then(Value::as_str),
            Some("inventory_items")
        );
        assert_eq!(
            catalog
                .pointer("/governance/lifecycle/inventory/release_state/current/version_id")
                .and_then(Value::as_str),
            Some(second_release_id.as_str())
        );

        rollback_module_release(
            root,
            &app_root,
            &chef_session(),
            ModuleRollbackRequest {
                module_id: "inventory".to_owned(),
                version_id: first_release_id.clone(),
            },
        )?;

        let catalog_after_rollback = module_catalog_for_rxdb(root)?;
        let modules_after_rollback = catalog_after_rollback
            .get("modules")
            .and_then(Value::as_array)
            .context("catalog modules after rollback")?;
        let inventory_after_rollback = modules_after_rollback
            .iter()
            .find(|module| module.get("id").and_then(Value::as_str) == Some("inventory"))
            .context("inventory module projection after rollback")?;
        assert_eq!(
            inventory_after_rollback
                .pointer("/lifecycle/release_state/current/version_id")
                .and_then(Value::as_str),
            Some(first_release_id.as_str())
        );
        assert_eq!(
            inventory_after_rollback
                .pointer("/lifecycle/release_state/current/target_version")
                .and_then(Value::as_str),
            Some("1.0.0")
        );
        assert_eq!(
            inventory_after_rollback
                .pointer("/lifecycle/rollback_target/version_id")
                .and_then(Value::as_str),
            Some(second_release_id.as_str())
        );
        assert_eq!(
            inventory_after_rollback
                .pointer("/lifecycle/current_semver")
                .and_then(Value::as_str),
            Some("1.0.0")
        );

        Ok(())
    }

    /// A module may state its restriction through `audience` alone, without
    /// `visibility_state`. The backfill test covers only the case where both
    /// are set, so it stays green even if this half of the check is deleted —
    /// measured, not assumed. `public` is the assertion that matters: the MCP
    /// visibility gate returns allow on `public` before it consults the policy,
    /// so a restricted module projected as public is reachable without a
    /// permission decision.
    #[test]
    fn audience_only_restriction_is_not_projected_as_public() -> anyhow::Result<()> {
        let temp = tempdir()?;
        let root = temp.path();
        let app_root = root.join("src/apps/business-os");
        drop(open_store(root)?);
        let installed_app_root = resolve_business_os_installed_app_root(root);
        fs::create_dir_all(app_root.join("modules/ctox"))?;
        fs::create_dir_all(installed_app_root.join("installed-modules/audience-restricted"))?;
        fs::write(app_root.join("index.html"), "<!doctype html>")?;
        fs::write(
            app_root.join("modules/ctox/module.json"),
            r#"{"id":"ctox","title":"CTOX","entry":"modules/ctox/index.html","install_scope":"core"}"#,
        )?;
        fs::write(
            installed_app_root.join("installed-modules/audience-restricted/module.json"),
            r#"{"id":"audience-restricted","title":"Audience Restricted","version":"1.1.0","entry":"installed-modules/audience-restricted/index.html","install_scope":"installed","lifecycle":{"audience":"restricted"}}"#,
        )?;

        let catalog = module_catalog_for_rxdb(root)?;
        let module = catalog
            .get("modules")
            .and_then(Value::as_array)
            .context("catalog modules")?
            .iter()
            .find(|module| module.get("id").and_then(Value::as_str) == Some("audience-restricted"))
            .context("audience-restricted module projection")?
            .clone();

        assert_eq!(
            module.pointer("/lifecycle/public").and_then(Value::as_bool),
            Some(false),
            "a module restricted by audience must not be projected public"
        );
        assert_eq!(
            module
                .pointer("/lifecycle/visibility_state")
                .and_then(Value::as_str),
            Some("restricted")
        );

        Ok(())
    }
}
