use anyhow::Context;
use anyhow::Result;
use rusqlite::Connection;
use std::collections::BTreeMap;
use std::path::Path;
use std::path::PathBuf;

const DEFAULT_RUNTIME_CONFIG_RELATIVE_PATH: &str = "runtime/ctox.sqlite3";

pub fn runtime_config_path(root: &Path) -> PathBuf {
    root.join(DEFAULT_RUNTIME_CONFIG_RELATIVE_PATH)
}

pub fn get(root: &Path, key: &str) -> Option<String> {
    load_runtime_env_map(root)
        .ok()
        .and_then(|map| map.get(key).cloned())
        .filter(|value| !value.trim().is_empty())
}

fn load_runtime_env_map(root: &Path) -> Result<BTreeMap<String, String>> {
    let path = runtime_config_path(root);
    if !path.exists() {
        return Ok(BTreeMap::new());
    }
    let conn = Connection::open(&path)
        .with_context(|| format!("failed to open runtime config {}", path.display()))?;
    let mut stmt = conn
        .prepare("SELECT env_key, env_value FROM runtime_env_kv ORDER BY env_key")
        .context("failed to prepare runtime config query")?;
    let rows = stmt
        .query_map([], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        })
        .context("failed to query runtime config rows")?;
    let mut out = BTreeMap::new();
    for row in rows {
        let (key, value) = row.context("failed to decode runtime config row")?;
        out.insert(key, value);
    }
    Ok(out)
}
