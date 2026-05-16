use anyhow::Context;
use anyhow::Result;
use include_dir::Dir;
use rusqlite::Connection;
use rusqlite::OptionalExtension;
use rusqlite::params;
use serde::Deserialize;
use std::collections::BTreeMap;
use std::collections::BTreeSet;
use std::fs;
use std::path::Path;
use std::path::PathBuf;
use std::time::Duration;
use std::time::SystemTime;
use std::time::UNIX_EPOCH;

use crate::skills::model::SkillMetadata;
use ctox_protocol::protocol::SkillScope;
use sha2::Digest;
use sha2::Sha256;

#[cfg(test)]
use std::collections::hash_map::DefaultHasher;
#[cfg(test)]
use std::hash::Hash;
#[cfg(test)]
use std::hash::Hasher;

const SKILL_BUNDLES_TABLE: &str = "ctox_skill_bundles";
const SKILL_FILES_TABLE: &str = "ctox_skill_files";
const SYSTEM_SKILL_VIRTUAL_ROOT: &str = "/__ctox_system_skills__";
const SYSTEM_SOURCE_PREFIX: &str = "embedded:skills/system/";
const DEFAULT_SQLITE_RELATIVE_PATH: &str = "runtime/ctox.sqlite3";

// The repo-root `skills/system` tree is the single source of truth for bundled
// system skills. Core embeds that tree directly instead of routing through a
// separate wrapper crate.
const SYSTEM_SKILLS_DIR: Dir =
    include_dir::include_dir!("$CARGO_MANIFEST_DIR/../../../skills/system");

const SYSTEM_SKILLS_DIR_NAME: &str = ".system";
const SKILLS_DIR_NAME: &str = "skills";

#[derive(Debug, Default, Deserialize)]
struct SystemSkillFrontmatter {
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    description: Option<String>,
    #[serde(default)]
    cluster: Option<String>,
}

pub(crate) fn system_cache_root_dir(codex_home: &Path) -> PathBuf {
    codex_home
        .join(SKILLS_DIR_NAME)
        .join(SYSTEM_SKILLS_DIR_NAME)
}

pub(crate) fn install_system_skills(codex_home: &Path) {
    // CTOX-managed system skills are no longer exposed as editable files under
    // `$CODEX_HOME/skills/.system`. Remove stale materializations best-effort;
    // the SQLite system skill store is bootstrapped per workspace in
    // `load_system_skills_from_store`.
    uninstall_system_skills(codex_home);
}

pub(crate) fn uninstall_system_skills(codex_home: &Path) {
    let system_skills_dir = system_cache_root_dir(codex_home);
    let _ = std::fs::remove_dir_all(&system_skills_dir);
}

pub(crate) fn load_system_skills_from_store(cwd: &Path) -> Result<Vec<SkillMetadata>> {
    bootstrap_system_skill_store(cwd)?;
    let conn = open_system_skill_db(cwd)?;
    let mut statement = conn.prepare(&format!(
        "SELECT skill_id, skill_name, description
         FROM {SKILL_BUNDLES_TABLE}
         WHERE class = 'ctox_core' AND source_path LIKE ?1
         ORDER BY cluster ASC, skill_name ASC, skill_id ASC"
    ))?;
    let rows = statement.query_map(params![format!("{SYSTEM_SOURCE_PREFIX}%")], |row| {
        let skill_id: String = row.get(0)?;
        let skill_name: String = row.get(1)?;
        Ok(SkillMetadata {
            name: skill_name,
            description: row.get(2)?,
            short_description: None,
            interface: None,
            dependencies: None,
            policy: None,
            permission_profile: None,
            managed_network_override: None,
            path_to_skills_md: system_skill_virtual_path(&skill_id),
            scope: SkillScope::System,
        })
    })?;
    rows.collect::<rusqlite::Result<Vec<_>>>()
        .map_err(anyhow::Error::from)
}

pub(crate) fn read_system_skill_body(cwd: &Path, virtual_path: &Path) -> Result<Option<String>> {
    let Some(skill_id) = skill_id_from_virtual_path(virtual_path) else {
        return Ok(None);
    };
    let conn = open_system_skill_db(cwd)?;
    let content: Option<Vec<u8>> = conn
        .query_row(
            &format!(
                "SELECT content_blob
                 FROM {SKILL_FILES_TABLE}
                 WHERE skill_id = ?1 AND relative_path = 'SKILL.md'
                 LIMIT 1"
            ),
            params![skill_id],
            |row| row.get(0),
        )
        .optional()
        .context("failed to load system skill body from sqlite")?;
    content
        .map(|bytes| String::from_utf8(bytes).context("system skill SKILL.md is not valid UTF-8"))
        .transpose()
}

pub(crate) fn is_system_skill_virtual_path(path: &Path) -> bool {
    skill_id_from_virtual_path(path).is_some()
}

fn system_skill_virtual_path(skill_id: &str) -> PathBuf {
    PathBuf::from(SYSTEM_SKILL_VIRTUAL_ROOT)
        .join(skill_id)
        .join("SKILL.md")
}

fn skill_id_from_virtual_path(path: &Path) -> Option<String> {
    let text = path.to_string_lossy().replace('\\', "/");
    let rest = text.strip_prefix(SYSTEM_SKILL_VIRTUAL_ROOT)?;
    let mut parts = rest.trim_start_matches('/').split('/');
    let skill_id = parts.next()?.to_string();
    let file = parts.next()?;
    (file == "SKILL.md").then_some(skill_id)
}

fn bootstrap_system_skill_store(cwd: &Path) -> Result<()> {
    let mut conn = open_system_skill_db(cwd)?;
    ensure_schema_on_conn(&conn)?;
    let tx = conn
        .transaction()
        .context("failed to open system skill transaction")?;

    let mut active_skill_ids = BTreeSet::new();
    let mut embedded = Vec::new();
    collect_embedded_skill_dirs(&SYSTEM_SKILLS_DIR, "", &mut embedded);
    for (skill_dir, path_in_system) in embedded {
        let Some((skill_id, skill_name, description, cluster, files)) =
            embedded_skill_record(skill_dir, &path_in_system)
        else {
            continue;
        };
        active_skill_ids.insert(skill_id.clone());
        let source_path = format!("{SYSTEM_SOURCE_PREFIX}{path_in_system}");
        let now = now_epoch_string();
        tx.execute(
            &format!(
                "INSERT INTO {SKILL_BUNDLES_TABLE}
                 (skill_id, skill_name, class, state, description, source_path, cluster, updated_at)
                 VALUES (?1, ?2, 'ctox_core', 'stable', ?3, ?4, ?5, ?6)
                 ON CONFLICT(skill_id) DO UPDATE SET
                   skill_name = excluded.skill_name,
                   class = excluded.class,
                   state = excluded.state,
                   description = excluded.description,
                   source_path = excluded.source_path,
                   cluster = excluded.cluster,
                   updated_at = excluded.updated_at"
            ),
            params![skill_id, skill_name, description, source_path, cluster, now],
        )?;
        tx.execute(
            &format!("DELETE FROM {SKILL_FILES_TABLE} WHERE skill_id = ?1"),
            params![skill_id],
        )?;
        for (relative_path, content) in files {
            tx.execute(
                &format!(
                    "INSERT INTO {SKILL_FILES_TABLE} (skill_id, relative_path, content_blob)
                     VALUES (?1, ?2, ?3)"
                ),
                params![skill_id, relative_path, content],
            )?;
        }
    }

    let mut stale_statement = tx.prepare(&format!(
        "SELECT skill_id FROM {SKILL_BUNDLES_TABLE}
         WHERE class = 'ctox_core' AND source_path LIKE ?1"
    ))?;
    let stale_ids = stale_statement
        .query_map(params![format!("{SYSTEM_SOURCE_PREFIX}%")], |row| {
            row.get::<_, String>(0)
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    drop(stale_statement);
    for skill_id in stale_ids {
        if !active_skill_ids.contains(&skill_id) {
            tx.execute(
                &format!("DELETE FROM {SKILL_FILES_TABLE} WHERE skill_id = ?1"),
                params![skill_id],
            )?;
            tx.execute(
                &format!("DELETE FROM {SKILL_BUNDLES_TABLE} WHERE skill_id = ?1"),
                params![skill_id],
            )?;
        }
    }

    tx.commit()
        .context("failed to commit system skill transaction")?;
    Ok(())
}

fn embedded_skill_record(
    skill_dir: &Dir<'_>,
    path_in_system: &str,
) -> Option<(String, String, String, String, BTreeMap<String, Vec<u8>>)> {
    let mut files = BTreeMap::new();
    let root_len = skill_dir.path().to_string_lossy().len();
    collect_embedded_files(skill_dir, root_len, &mut files);
    let skill_body = String::from_utf8_lossy(files.get("SKILL.md")?).to_string();
    let frontmatter = extract_frontmatter(&skill_body)
        .and_then(|frontmatter| serde_yaml::from_str::<SystemSkillFrontmatter>(&frontmatter).ok())
        .unwrap_or_default();
    let skill_name = frontmatter
        .name
        .as_deref()
        .map(sanitize_single_line)
        .filter(|value| !value.is_empty())
        .or_else(|| {
            skill_dir
                .path()
                .file_name()
                .map(|value| sanitize_single_line(&value.to_string_lossy()))
        })?;
    let description = frontmatter
        .description
        .as_deref()
        .map(sanitize_single_line)
        .unwrap_or_default();
    let cluster = frontmatter.cluster.unwrap_or_else(|| {
        path_in_system
            .split('/')
            .next()
            .filter(|value| !value.is_empty() && *value != skill_name)
            .unwrap_or_default()
            .to_string()
    });
    let skill_id = stable_skill_id(&skill_name);
    Some((skill_id, skill_name, description, cluster, files))
}

fn collect_embedded_skill_dirs<'a>(
    dir: &'a Dir<'a>,
    path_prefix: &str,
    out: &mut Vec<(&'a Dir<'a>, String)>,
) {
    let has_skill_md = dir.files().any(|file| {
        file.path()
            .file_name()
            .and_then(|name| name.to_str())
            .map(|name| name == "SKILL.md")
            .unwrap_or(false)
    });
    if has_skill_md {
        out.push((dir, path_prefix.to_string()));
        return;
    }
    for entry in dir.entries() {
        if let include_dir::DirEntry::Dir(subdir) = entry {
            let segment = subdir
                .path()
                .file_name()
                .map(|name| name.to_string_lossy().to_string())
                .unwrap_or_default();
            let next_prefix = if path_prefix.is_empty() {
                segment
            } else {
                format!("{path_prefix}/{segment}")
            };
            collect_embedded_skill_dirs(subdir, &next_prefix, out);
        }
    }
}

fn collect_embedded_files(
    dir: &Dir<'_>,
    root_path_len: usize,
    out: &mut BTreeMap<String, Vec<u8>>,
) {
    for entry in dir.entries() {
        match entry {
            include_dir::DirEntry::File(file) => {
                let full = file.path().to_string_lossy();
                let relative = full[root_path_len..].trim_start_matches('/').to_string();
                out.insert(relative, file.contents().to_vec());
            }
            include_dir::DirEntry::Dir(subdir) => {
                collect_embedded_files(subdir, root_path_len, out);
            }
        }
    }
}

fn open_system_skill_db(cwd: &Path) -> Result<Connection> {
    let path = system_skill_db_path(cwd);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("failed to create skill db dir {}", parent.display()))?;
    }
    let conn = Connection::open(&path)
        .with_context(|| format!("failed to open skill db {}", path.display()))?;
    conn.busy_timeout(sqlite_busy_timeout_duration())
        .context("failed to configure skill db busy_timeout")?;
    ensure_schema_on_conn(&conn)?;
    Ok(conn)
}

fn system_skill_db_path(cwd: &Path) -> PathBuf {
    if let Some(state_root) = std::env::var_os("CTOX_STATE_ROOT")
        .map(PathBuf::from)
        .filter(|path| !path.as_os_str().is_empty())
    {
        return state_root.join("ctox.sqlite3");
    }
    find_ctox_root(cwd)
        .unwrap_or_else(|| cwd.to_path_buf())
        .join(DEFAULT_SQLITE_RELATIVE_PATH)
}

fn find_ctox_root(cwd: &Path) -> Option<PathBuf> {
    for ancestor in cwd.ancestors() {
        if ancestor.join("skills/system").is_dir() && ancestor.join("Cargo.toml").is_file() {
            return Some(ancestor.to_path_buf());
        }
    }
    None
}

fn sqlite_busy_timeout_duration() -> Duration {
    let millis = std::env::var("CTOX_SQLITE_BUSY_TIMEOUT_MS")
        .ok()
        .and_then(|value| value.parse::<u64>().ok())
        .filter(|value| (1..=120_000).contains(value))
        .unwrap_or(30_000);
    Duration::from_millis(millis)
}

fn ensure_schema_on_conn(conn: &Connection) -> Result<()> {
    conn.execute_batch(&format!(
        "PRAGMA journal_mode=WAL;
         CREATE TABLE IF NOT EXISTS {SKILL_BUNDLES_TABLE} (
             skill_id TEXT PRIMARY KEY,
             skill_name TEXT NOT NULL,
             class TEXT NOT NULL,
             state TEXT NOT NULL,
             description TEXT NOT NULL,
             source_path TEXT,
             cluster TEXT NOT NULL DEFAULT '',
             updated_at TEXT NOT NULL
         );
         CREATE TABLE IF NOT EXISTS {SKILL_FILES_TABLE} (
             skill_id TEXT NOT NULL,
             relative_path TEXT NOT NULL,
             content_blob BLOB NOT NULL,
             PRIMARY KEY (skill_id, relative_path),
             FOREIGN KEY (skill_id) REFERENCES {SKILL_BUNDLES_TABLE}(skill_id) ON DELETE CASCADE
         );"
    ))
    .context("failed to initialize system skill store schema")?;
    ensure_column_on_conn(
        conn,
        SKILL_BUNDLES_TABLE,
        "source_path",
        "TEXT",
        "failed to add source_path column to skill bundles",
    )?;
    ensure_column_on_conn(
        conn,
        SKILL_BUNDLES_TABLE,
        "cluster",
        "TEXT NOT NULL DEFAULT ''",
        "failed to add cluster column to skill bundles",
    )?;
    Ok(())
}

fn ensure_column_on_conn(
    conn: &Connection,
    table_name: &str,
    column_name: &str,
    column_definition: &str,
    context: &'static str,
) -> Result<()> {
    let has_column = conn
        .prepare(&format!("PRAGMA table_info({table_name})"))?
        .query_map([], |row| row.get::<_, String>(1))?
        .filter_map(|result| result.ok())
        .any(|name| name == column_name);
    if !has_column {
        conn.execute(
            &format!("ALTER TABLE {table_name} ADD COLUMN {column_name} {column_definition}"),
            [],
        )
        .context(context)?;
    }
    Ok(())
}

fn stable_skill_id(skill_name: &str) -> String {
    let digest = Sha256::digest(skill_name.trim().as_bytes());
    format!("skill-{:x}", digest)[..18].to_string()
}

fn extract_frontmatter(body: &str) -> Option<String> {
    let rest = body.strip_prefix("---\n")?;
    let end = rest.find("\n---")?;
    Some(rest[..end].to_string())
}

fn sanitize_single_line(value: &str) -> String {
    value.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn now_epoch_string() -> String {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
        .to_string()
}

#[cfg(test)]
fn collect_fingerprint_items(dir: &Dir<'_>, items: &mut Vec<(String, Option<u64>)>) {
    for entry in dir.entries() {
        match entry {
            include_dir::DirEntry::Dir(subdir) => {
                items.push((subdir.path().to_string_lossy().to_string(), None));
                collect_fingerprint_items(subdir, items);
            }
            include_dir::DirEntry::File(file) => {
                let mut file_hasher = DefaultHasher::new();
                file.contents().hash(&mut file_hasher);
                items.push((
                    file.path().to_string_lossy().to_string(),
                    Some(file_hasher.finish()),
                ));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::SYSTEM_SKILLS_DIR;
    use super::collect_fingerprint_items;
    use super::ensure_schema_on_conn;
    use rusqlite::Connection;

    #[test]
    fn fingerprint_traverses_nested_entries() {
        let mut items = Vec::new();
        collect_fingerprint_items(&SYSTEM_SKILLS_DIR, &mut items);
        let mut paths: Vec<String> = items.into_iter().map(|(path, _)| path).collect();
        paths.sort_unstable();

        assert!(
            paths
                .binary_search_by(|probe| probe.as_str().cmp("skill_meta/skill-creator/SKILL.md"))
                .is_ok()
        );
        assert!(
            paths
                .binary_search_by(|probe| probe
                    .as_str()
                    .cmp("skill_meta/skill-creator/scripts/init_skill.py"))
                .is_ok()
        );
    }

    #[test]
    fn schema_init_migrates_existing_skill_bundle_columns() {
        let conn = Connection::open_in_memory().expect("open sqlite");
        conn.execute_batch(
            "CREATE TABLE ctox_skill_bundles (
                 skill_id TEXT PRIMARY KEY,
                 skill_name TEXT NOT NULL,
                 class TEXT NOT NULL,
                 state TEXT NOT NULL,
                 description TEXT NOT NULL,
                 updated_at TEXT NOT NULL
             );
             CREATE TABLE ctox_skill_files (
                 skill_id TEXT NOT NULL,
                 relative_path TEXT NOT NULL,
                 content_blob BLOB NOT NULL,
                 PRIMARY KEY (skill_id, relative_path)
             );",
        )
        .expect("create old schema");

        ensure_schema_on_conn(&conn).expect("migrate schema");
        let columns: Vec<String> = conn
            .prepare("PRAGMA table_info(ctox_skill_bundles)")
            .expect("prepare table_info")
            .query_map([], |row| row.get::<_, String>(1))
            .expect("query columns")
            .collect::<rusqlite::Result<Vec<_>>>()
            .expect("collect columns");

        assert!(columns.iter().any(|column| column == "source_path"));
        assert!(columns.iter().any(|column| column == "cluster"));
    }
}
