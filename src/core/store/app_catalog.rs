//! Read-only questions about the committed Business App catalog.
//!
//! CTOX Sync v4 moves projection logic out of the core and into the apps
//! (owner decision E17). While both exist, the core and an app must not write
//! the same app collections. An app version opts out of every core projection
//! into its collections by declaring `"core_projection": false` in its
//! manifest; every core projection path asks [`core_projection_enabled`]
//! before it writes.
//!
//! The answer is read from the committed store on every call through a fresh
//! read-only connection: activating a version takes effect immediately, in the
//! daemon and in separate CLI processes, without a restart. There is
//! deliberately no in-memory cache.
use std::path::Path;
use std::time::Duration;

use rusqlite::{Connection, OpenFlags, OptionalExtension};
use serde_json::Value;

use crate::change_log::identifier;

const APPS: &str = "business_apps";
const VERSIONS: &str = "business_app_versions";
/// WAL readers almost never wait; a bounded wait only covers WAL recovery.
const READ_WAIT: Duration = Duration::from_millis(250);

/// Whether the core may still project into the collections of `app_id`.
///
/// Returns `false` only when the currently active version of the app
/// (`business_apps.active_ver`) has a committed `business_app_versions`
/// document whose `manifest.core_projection` is the JSON literal `false`.
/// Everything else keeps today's behaviour and returns `true`: field missing
/// or not a boolean, app not installed or not active, version document
/// missing, catalog collections absent (legacy store), store file missing or
/// unreadable.
///
/// Documents are matched by their `app_id`/`app_ver` fields, never by id: the
/// kernel writes version ids as `app@ver`, the base-release import as
/// `app:ver`.
pub fn core_projection_enabled(store: &Path, app_id: &str) -> bool {
    !matches!(
        active_manifest_declares_no_core_projection(store, app_id),
        Ok(true)
    )
}

fn active_manifest_declares_no_core_projection(
    store: &Path,
    app_id: &str,
) -> rusqlite::Result<bool> {
    // Read-only and without CREATE: a missing store is never created here.
    let conn = Connection::open_with_flags(
        store,
        OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )?;
    conn.busy_timeout(READ_WAIT)?;
    let Some(apps) = active_table(&conn, APPS)? else {
        return Ok(false);
    };
    let Some(versions) = active_table(&conn, VERSIONS)? else {
        return Ok(false);
    };
    // Kernel-written app documents carry no app_id field; their id is the app.
    let app_rows = documents(
        &conn,
        &format!(
            "SELECT data FROM {apps} WHERE deleted=0 AND
             (json_extract(data,'$.app_id')=?1
              OR (json_extract(data,'$.app_id') IS NULL AND id=?1))"
        ),
        &[app_id],
    )?;
    let mut active_versions = app_rows.iter().map(|app| {
        let installed = app
            .get("state")
            .map_or(true, |state| state.as_str() == Some("active"));
        app.get("active_ver")
            .and_then(Value::as_str)
            .filter(|ver| installed && !ver.is_empty())
    });
    // Exactly one consistent, installed active version; anything else is
    // ambiguous and keeps the core projection.
    let Some(Some(active_ver)) = active_versions.next() else {
        return Ok(false);
    };
    if active_versions.any(|other| other != Some(active_ver)) {
        return Ok(false);
    }
    let version_rows = documents(
        &conn,
        &format!(
            "SELECT data FROM {versions} WHERE deleted=0 AND
             json_extract(data,'$.app_id')=?1 AND json_extract(data,'$.app_ver')=?2"
        ),
        &[app_id, active_ver],
    )?;
    Ok(!version_rows.is_empty()
        && version_rows.iter().all(|version| {
            version.pointer("/manifest/core_projection") == Some(&Value::Bool(false))
        }))
}

fn active_table(conn: &Connection, collection: &str) -> rusqlite::Result<Option<String>> {
    let table: Option<String> = conn
        .query_row(
            "SELECT table_name FROM _collections WHERE name=?1 AND state='active'",
            [collection],
            |r| r.get(0),
        )
        .optional()?;
    table.map(|t| identifier(&t)).transpose()
}

fn documents(conn: &Connection, sql: &str, args: &[&str]) -> rusqlite::Result<Vec<Value>> {
    let mut stmt = conn.prepare(sql)?;
    let rows = stmt.query_map(rusqlite::params_from_iter(args), |r| r.get::<_, String>(0))?;
    rows.map(|row| {
        let data = row?;
        serde_json::from_str(&data).map_err(|e| {
            rusqlite::Error::FromSqlConversionFailure(0, rusqlite::types::Type::Text, Box::new(e))
        })
    })
    .collect()
}

#[cfg(test)]
#[path = "app_catalog_tests.rs"]
mod tests;
