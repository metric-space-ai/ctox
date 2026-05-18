//! One-shot migration: consolidate the historical `cto_agent.db` and
//! `ctox_lcm.db` files into `runtime/ctox.db`.
//!
//! Idempotent. Runs at service start and exits fast when there is nothing to
//! do (either `ctox.db` already exists or neither legacy file is present).
//! When a merge is needed:
//!
//!   1. Create a fresh `ctox.db.migrating` next to the target.
//!   2. For each legacy file, `ATTACH` it and walk its `sqlite_master`:
//!      recreate tables/virtual tables via their original DDL, copy rows
//!      (FTS5 virtual tables are populated via the VT interface), then
//!      recreate indexes/triggers/views.
//!   3. `rename(ctox.db.migrating → ctox.db)` — atomic on POSIX filesystems.
//!   4. Move the legacy `.db` files (plus any `-wal`/`-shm` side files) into
//!      `runtime/backup/<ISO8601>/` so a manual rollback stays possible.

use anyhow::{Context, Result};
use chrono::Utc;
use rusqlite::Connection;
use std::fs;
use std::path::Path;

use crate::paths;

pub fn run_if_needed(root: &Path) -> Result<()> {
    let core = paths::core_db(root);
    if core.exists() {
        return Ok(());
    }
    let legacy_mission = paths::legacy_mission_db(root);
    let legacy_lcm = paths::legacy_lcm_db(root);
    let have_mission = legacy_mission.exists();
    let have_lcm = legacy_lcm.exists();
    if !have_mission && !have_lcm {
        return Ok(());
    }

    let runtime = paths::runtime_dir(root);
    fs::create_dir_all(&runtime)
        .with_context(|| format!("failed to create {}", runtime.display()))?;
    let tmp = runtime.join("ctox.db.migrating");
    let _ = fs::remove_file(&tmp);

    {
        let conn = Connection::open(&tmp)
            .with_context(|| format!("failed to open {} for migration", tmp.display()))?;
        let _ = conn.pragma_update(None, "journal_mode", "WAL");
        let _ = conn.pragma_update(None, "foreign_keys", "ON");
        if have_mission {
            attach_and_copy(&conn, &legacy_mission, "legacy_mission")
                .context("failed to copy legacy mission tables")?;
        }
        if have_lcm {
            attach_and_copy(&conn, &legacy_lcm, "legacy_lcm")
                .context("failed to copy legacy lcm tables")?;
        }
    }

    fs::rename(&tmp, &core)
        .with_context(|| format!("failed to move {} to {}", tmp.display(), core.display()))?;

    let stamp = Utc::now().format("%Y%m%dT%H%M%SZ").to_string();
    let backup_root = paths::backup_dir(root).join(&stamp);
    fs::create_dir_all(&backup_root)
        .with_context(|| format!("failed to create {}", backup_root.display()))?;
    for legacy in [legacy_mission.as_path(), legacy_lcm.as_path()] {
        if !legacy.exists() {
            continue;
        }
        let file_name = legacy
            .file_name()
            .context("legacy db path has no file name")?
            .to_owned();
        let dest = backup_root.join(&file_name);
        fs::rename(legacy, &dest).with_context(|| {
            format!("failed to move {} to {}", legacy.display(), dest.display())
        })?;
        for suffix in ["-wal", "-shm"] {
            let mut side_name = file_name.clone();
            side_name.push(suffix);
            let side_path = legacy.with_file_name(&side_name);
            if side_path.exists() {
                let side_dest = backup_root.join(&side_name);
                let _ = fs::rename(&side_path, &side_dest);
            }
        }
    }
    eprintln!(
        "ctox db migration: consolidated legacy databases into {}; originals archived under {}",
        core.display(),
        backup_root.display()
    );
    Ok(())
}

fn attach_and_copy(conn: &Connection, legacy_path: &Path, alias: &str) -> Result<()> {
    let escaped = legacy_path.to_string_lossy().replace('\'', "''");
    conn.execute_batch(&format!("ATTACH DATABASE '{escaped}' AS {alias};"))?;

    let tables: Vec<(String, String)> = {
        let mut stmt = conn.prepare(&format!(
            "SELECT name, sql FROM {alias}.sqlite_master \
             WHERE type = 'table' AND name NOT LIKE 'sqlite_%' AND sql IS NOT NULL \
             ORDER BY rowid"
        ))?;
        let rows = stmt.query_map([], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        })?;
        rows.collect::<rusqlite::Result<Vec<_>>>()?
    };
    for (name, sql) in &tables {
        if is_fts_shadow_name(name) {
            continue;
        }
        conn.execute_batch(sql)
            .with_context(|| format!("failed to recreate {name} from {alias}"))?;
    }

    for (name, sql) in &tables {
        if is_fts_shadow_name(name) {
            continue;
        }
        let is_fts = sql.to_ascii_uppercase().contains("USING FTS5");
        if is_fts {
            let cols: Vec<String> = {
                let mut cstmt = conn.prepare(
                    "SELECT name FROM pragma_table_info(?1) WHERE name NOT LIKE 'sqlite_%'",
                )?;
                let rows = cstmt.query_map([name.as_str()], |row| row.get::<_, String>(0))?;
                rows.collect::<rusqlite::Result<Vec<_>>>()?
            };
            if cols.is_empty() {
                continue;
            }
            let col_list = cols
                .iter()
                .map(|c| format!("\"{}\"", c.replace('"', "\"\"")))
                .collect::<Vec<_>>()
                .join(", ");
            conn.execute_batch(&format!(
                "INSERT INTO \"{name}\"(rowid, {col_list}) SELECT rowid, {col_list} FROM {alias}.\"{name}\";"
            ))
            .with_context(|| format!("failed to copy FTS rows of {name} from {alias}"))?;
        } else {
            conn.execute_batch(&format!(
                "INSERT INTO \"{name}\" SELECT * FROM {alias}.\"{name}\";"
            ))
            .with_context(|| format!("failed to copy rows of {name} from {alias}"))?;
        }
    }

    let aux: Vec<(String, String)> = {
        let mut stmt = conn.prepare(&format!(
            "SELECT name, sql FROM {alias}.sqlite_master \
             WHERE type IN ('index', 'trigger', 'view') AND name NOT LIKE 'sqlite_%' AND sql IS NOT NULL \
             ORDER BY CASE type WHEN 'view' THEN 0 WHEN 'trigger' THEN 1 ELSE 2 END, rowid"
        ))?;
        let rows = stmt.query_map([], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        })?;
        rows.collect::<rusqlite::Result<Vec<_>>>()?
    };
    for (name, sql) in &aux {
        conn.execute_batch(sql)
            .with_context(|| format!("failed to recreate aux object {name} from {alias}"))?;
    }

    conn.execute_batch(&format!("DETACH DATABASE {alias};"))?;
    Ok(())
}

fn is_fts_shadow_name(name: &str) -> bool {
    for suffix in ["_data", "_idx", "_content", "_docsize", "_config"] {
        if name.ends_with(suffix) {
            return true;
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;
    use rusqlite::params;
    use tempfile::TempDir;

    fn seed_legacy_dbs(root: &Path) {
        let runtime = paths::runtime_dir(root);
        fs::create_dir_all(&runtime).unwrap();

        let mission = paths::legacy_mission_db(root);
        let conn = Connection::open(&mission).unwrap();
        conn.execute_batch(
            "CREATE TABLE queue_items (id INTEGER PRIMARY KEY, payload TEXT NOT NULL);
             CREATE INDEX idx_queue_items_payload ON queue_items(payload);",
        )
        .unwrap();
        conn.execute(
            "INSERT INTO queue_items (id, payload) VALUES (?1, ?2), (?3, ?4)",
            params![1, "alpha", 2, "beta"],
        )
        .unwrap();
        drop(conn);

        let lcm = paths::legacy_lcm_db(root);
        let conn = Connection::open(&lcm).unwrap();
        conn.execute_batch(
            "CREATE TABLE messages (
                 conversation_id INTEGER NOT NULL,
                 ordinal INTEGER NOT NULL,
                 content TEXT NOT NULL,
                 PRIMARY KEY(conversation_id, ordinal)
             );
             CREATE VIRTUAL TABLE messages_fts USING fts5(
                 content,
                 content='',
                 tokenize='unicode61'
             );",
        )
        .unwrap();
        conn.execute(
            "INSERT INTO messages (conversation_id, ordinal, content) VALUES (1, 1, 'hello world'), (1, 2, 'goodbye world')",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO messages_fts (rowid, content) VALUES (1, 'hello world'), (2, 'goodbye world')",
            [],
        )
        .unwrap();
    }

    #[test]
    fn merges_legacy_files_into_ctox_db() {
        let tmp = TempDir::new().unwrap();
        let root = tmp.path();
        seed_legacy_dbs(root);

        run_if_needed(root).unwrap();

        assert!(paths::core_db(root).exists());
        assert!(!paths::legacy_mission_db(root).exists());
        assert!(!paths::legacy_lcm_db(root).exists());

        let conn = Connection::open(paths::core_db(root)).unwrap();
        let queue_count: i64 = conn
            .query_row("SELECT COUNT(*) FROM queue_items", [], |r| r.get(0))
            .unwrap();
        assert_eq!(queue_count, 2);
        let msg_count: i64 = conn
            .query_row("SELECT COUNT(*) FROM messages", [], |r| r.get(0))
            .unwrap();
        assert_eq!(msg_count, 2);
        let fts_hits: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM messages_fts WHERE messages_fts MATCH 'hello'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(fts_hits, 1);
        let has_index: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type = 'index' AND name = 'idx_queue_items_payload'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(has_index, 1);

        let backup_root = paths::backup_dir(root);
        assert!(backup_root.exists());
        let mut backups = fs::read_dir(&backup_root).unwrap();
        let stamp_dir = backups.next().unwrap().unwrap().path();
        assert!(stamp_dir.join("cto_agent.db").exists());
        assert!(stamp_dir.join("ctox_lcm.db").exists());
    }

    #[test]
    fn noop_when_core_db_already_exists() {
        let tmp = TempDir::new().unwrap();
        let root = tmp.path();
        fs::create_dir_all(paths::runtime_dir(root)).unwrap();
        let conn = Connection::open(paths::core_db(root)).unwrap();
        conn.execute_batch("CREATE TABLE marker (id INTEGER);")
            .unwrap();
        conn.execute("INSERT INTO marker VALUES (42)", []).unwrap();
        drop(conn);

        run_if_needed(root).unwrap();

        let conn = Connection::open(paths::core_db(root)).unwrap();
        let marker: i64 = conn
            .query_row("SELECT id FROM marker", [], |r| r.get(0))
            .unwrap();
        assert_eq!(marker, 42);
    }

    #[test]
    fn noop_on_fresh_install() {
        let tmp = TempDir::new().unwrap();
        let root = tmp.path();
        fs::create_dir_all(paths::runtime_dir(root)).unwrap();

        run_if_needed(root).unwrap();

        assert!(!paths::core_db(root).exists());
    }
}
