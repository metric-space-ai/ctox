use anyhow::Context;
use anyhow::Result;
use rusqlite::params;
use rusqlite::Connection;
use std::collections::BTreeMap;
use std::path::Path;
use std::path::PathBuf;

use crate::inference::runtime_state;
use crate::secrets;

const RUNTIME_ENV_TABLE: &str = "runtime_env_kv";

pub fn runtime_config_path(root: &Path) -> PathBuf {
    crate::persistence::sqlite_path(root)
}

pub fn load_runtime_env_map(root: &Path) -> Result<BTreeMap<String, String>> {
    let mut env_map = load_persisted_runtime_env_map(root)?;
    // Merge credentials from the encrypted secret store so callers see a
    // unified map regardless of where the value lives.
    secrets::merge_credentials_into_env_map(root, &mut env_map);
    if let Ok(state) = runtime_state::load_or_resolve_runtime_state(root) {
        runtime_state::apply_runtime_state_to_env_map(&mut env_map, &state);
    }
    Ok(env_map)
}

pub fn effective_runtime_env_map(root: &Path) -> Result<BTreeMap<String, String>> {
    let mut env_map = effective_operator_env_map(root)?;
    if let Ok(state) = runtime_state::load_or_resolve_runtime_state(root) {
        runtime_state::apply_runtime_state_to_env_map(&mut env_map, &state);
    }
    Ok(env_map)
}

pub fn effective_operator_env_map(root: &Path) -> Result<BTreeMap<String, String>> {
    let mut env_map = load_persisted_runtime_env_map(root)?;
    env_map.retain(|key, _| !runtime_state::is_runtime_state_key(key));
    secrets::merge_credentials_into_env_map(root, &mut env_map);
    Ok(env_map)
}

pub fn save_runtime_env_map(root: &Path, env_map: &BTreeMap<String, String>) -> Result<()> {
    // Route secret keys into the encrypted store before persisting.
    let mut clean_map = env_map.clone();
    for (key, value) in env_map {
        if secrets::is_secret_key(key) && !value.trim().is_empty() {
            secrets::set_credential(root, key, value)
                .with_context(|| format!("failed to persist secret {key} into encrypted store"))?;
            clean_map.remove(key);
        }
    }
    let state = runtime_state::derive_runtime_state_from_env_map(root, &clean_map)?;
    save_runtime_state_projection(root, &state, &clean_map)
}

pub fn save_runtime_state_projection(
    root: &Path,
    state: &runtime_state::InferenceRuntimeState,
    env_map: &BTreeMap<String, String>,
) -> Result<()> {
    let mut normalized_env_map = env_map.clone();
    normalized_env_map.retain(|key, _| !runtime_state::is_runtime_state_key(key));
    normalized_env_map.retain(|key, _| !secrets::is_secret_key(key));
    runtime_state::apply_runtime_state_to_env_map(&mut normalized_env_map, state);
    runtime_state::persist_runtime_state(root, state)?;
    write_runtime_env_map(root, &normalized_env_map)
}

pub fn env_or_config(root: &Path, key: &str) -> Option<String> {
    if runtime_state::is_runtime_state_key(key) {
        if let Ok(state) = runtime_state::load_or_resolve_runtime_state(root) {
            if let Some(value) = runtime_state::owned_runtime_env_value(&state, key)
                .filter(|value| !value.trim().is_empty())
            {
                return Some(value);
            }
        }
    }
    // Secret keys are resolved only from the encrypted store.
    if secrets::is_secret_key(key) {
        return secrets::get_credential(root, key).filter(|value| !value.trim().is_empty());
    }
    load_runtime_env_map(root)
        .ok()
        .and_then(|map| map.get(key).cloned())
        .filter(|value| !value.trim().is_empty())
}

pub fn configured_chat_model(root: &Path) -> Option<String> {
    runtime_state::load_or_resolve_runtime_state(root)
        .ok()
        .and_then(|state| {
            state
                .base_model
                .or(state.requested_model)
                .or(state.active_model)
        })
        .filter(|value| !value.trim().is_empty())
        .or_else(|| {
            env_or_config(root, "CTOX_CHAT_MODEL_BASE")
                .or_else(|| env_or_config(root, "CTOX_CHAT_MODEL"))
        })
        .filter(|value| !value.trim().is_empty())
}

pub fn effective_chat_model(root: &Path) -> Option<String> {
    runtime_state::load_or_resolve_runtime_state(root)
        .ok()
        .and_then(|state| state.active_model)
        .or_else(|| env_or_config(root, "CTOX_ACTIVE_MODEL"))
        .or_else(|| configured_chat_model(root))
        .filter(|value| !value.trim().is_empty())
}

pub fn configured_chat_model_from_map(env_map: &BTreeMap<String, String>) -> Option<String> {
    env_map
        .get("CTOX_CHAT_MODEL")
        .or_else(|| env_map.get("CTOX_CHAT_MODEL_BASE"))
        .cloned()
        .filter(|value| !value.trim().is_empty())
}

pub fn configured_chat_model_family_from_map(env_map: &BTreeMap<String, String>) -> Option<String> {
    env_map
        .get("CTOX_CHAT_MODEL_FAMILY")
        .cloned()
        .filter(|value| !value.trim().is_empty())
}

pub fn effective_chat_model_from_map(env_map: &BTreeMap<String, String>) -> Option<String> {
    env_map
        .get("CTOX_ACTIVE_MODEL")
        .cloned()
        .filter(|value| !value.trim().is_empty())
        .or_else(|| configured_chat_model_from_map(env_map))
}

pub fn config_flag(root: &Path, key: &str) -> bool {
    env_or_config(root, key)
        .as_deref()
        .and_then(parse_boolish)
        .unwrap_or(false)
}

pub fn auxiliary_backend_enabled(root: &Path, role_prefix: &str) -> bool {
    if let Ok(state) = runtime_state::load_or_resolve_runtime_state(root) {
        let role = match role_prefix {
            "EMBEDDING" => Some(crate::inference::engine::AuxiliaryRole::Embedding),
            "STT" => Some(crate::inference::engine::AuxiliaryRole::Stt),
            "TTS" => Some(crate::inference::engine::AuxiliaryRole::Tts),
            _ => None,
        };
        if let Some(role) = role {
            return runtime_state::auxiliary_runtime_state_for_role(&state, role).enabled;
        }
    }
    if config_flag(root, "CTOX_DISABLE_AUXILIARY_BACKENDS") {
        return false;
    }
    let disable_key = format!("CTOX_DISABLE_{role_prefix}_BACKEND");
    if config_flag(root, &disable_key) {
        return false;
    }
    let enable_key = format!("CTOX_ENABLE_{role_prefix}_BACKEND");
    if let Some(value) = env_or_config(root, &enable_key) {
        return parse_boolish(&value).unwrap_or(true);
    }
    let model_key = format!("CTOX_{role_prefix}_MODEL");
    if let Some(value) = env_or_config(root, &model_key) {
        return !is_disabled_selector(&value);
    }
    true
}

fn load_persisted_runtime_env_map(root: &Path) -> Result<BTreeMap<String, String>> {
    let conn = open_runtime_persistence_db(root)?;
    let env_map = load_runtime_env_map_from_db(&conn)?;
    Ok(env_map)
}

fn write_runtime_env_map(root: &Path, env_map: &BTreeMap<String, String>) -> Result<()> {
    let filtered_env_map = env_map
        .iter()
        .filter(|(key, _)| !secrets::is_secret_key(key))
        .map(|(key, value)| (key.clone(), value.clone()))
        .collect::<BTreeMap<_, _>>();
    let mut conn = open_runtime_persistence_db(root)?;
    let tx = conn
        .transaction()
        .context("failed to open runtime env transaction")?;
    tx.execute(&format!("DELETE FROM {RUNTIME_ENV_TABLE}"), [])
        .context("failed to clear runtime env table")?;
    for (key, value) in &filtered_env_map {
        if key.trim().is_empty() {
            continue;
        }
        tx.execute(
            &format!("INSERT INTO {RUNTIME_ENV_TABLE} (env_key, env_value) VALUES (?1, ?2)"),
            params![key, value],
        )
        .with_context(|| format!("failed to persist runtime key {key}"))?;
    }
    tx.commit()
        .context("failed to commit runtime env transaction")?;
    Ok(())
}

fn open_runtime_persistence_db(root: &Path) -> Result<Connection> {
    let path = runtime_config_path(root);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("failed to create runtime db dir {}", parent.display()))?;
    }
    let conn = Connection::open(&path)
        .with_context(|| format!("failed to open runtime db {}", path.display()))?;
    conn.execute_batch(&format!(
        "PRAGMA journal_mode=WAL;
         CREATE TABLE IF NOT EXISTS {RUNTIME_ENV_TABLE} (
             env_key TEXT PRIMARY KEY,
             env_value TEXT NOT NULL
         );"
    ))
    .context("failed to initialize runtime env table")?;
    Ok(conn)
}

fn load_runtime_env_map_from_db(conn: &Connection) -> Result<BTreeMap<String, String>> {
    let mut stmt = conn
        .prepare(&format!(
            "SELECT env_key, env_value FROM {RUNTIME_ENV_TABLE} ORDER BY env_key"
        ))
        .context("failed to prepare runtime env query")?;
    let rows = stmt
        .query_map([], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        })
        .context("failed to read runtime env rows")?;
    let mut env_map = BTreeMap::new();
    for row in rows {
        let (key, value) = row.context("failed to decode runtime env row")?;
        env_map.insert(key, value);
    }
    Ok(env_map)
}

fn parse_boolish(value: &str) -> Option<bool> {
    match value.trim().to_ascii_lowercase().as_str() {
        "1" | "true" | "yes" | "on" => Some(true),
        "0" | "false" | "no" | "off" => Some(false),
        _ => None,
    }
}

fn is_disabled_selector(value: &str) -> bool {
    matches!(
        value.trim().to_ascii_lowercase().as_str(),
        "" | "0" | "false" | "off" | "none" | "null" | "disabled" | "disable"
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn make_temp_root() -> std::path::PathBuf {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path = std::env::temp_dir().join(format!("ctox-runtime-env-test-{unique}"));
        std::fs::create_dir_all(path.join("runtime")).unwrap();
        path
    }

    #[test]
    fn auxiliary_backend_enabled_defaults_to_true() {
        let root = make_temp_root();
        assert!(auxiliary_backend_enabled(&root, "EMBEDDING"));
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn auxiliary_backend_enabled_honors_global_disable() {
        let root = make_temp_root();
        let mut env_map = BTreeMap::new();
        env_map.insert(
            "CTOX_DISABLE_AUXILIARY_BACKENDS".to_string(),
            "1".to_string(),
        );
        save_runtime_env_map(&root, &env_map).unwrap();
        assert!(!auxiliary_backend_enabled(&root, "EMBEDDING"));
        assert!(!auxiliary_backend_enabled(&root, "STT"));
        assert!(!auxiliary_backend_enabled(&root, "TTS"));
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn auxiliary_backend_enabled_honors_disabled_model_selector() {
        let root = make_temp_root();
        let mut env_map = BTreeMap::new();
        env_map.insert("CTOX_STT_MODEL".to_string(), "disabled".to_string());
        save_runtime_env_map(&root, &env_map).unwrap();
        assert!(!auxiliary_backend_enabled(&root, "STT"));
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn effective_runtime_env_map_reads_persisted_values_only() {
        let root = make_temp_root();
        let mut env_map = BTreeMap::new();
        env_map.insert("CTOX_ENGINE_FEATURES".to_string(), "persisted".to_string());
        save_runtime_env_map(&root, &env_map).unwrap();

        let effective = effective_runtime_env_map(&root).unwrap();

        assert_eq!(
            effective.get("CTOX_ENGINE_FEATURES").map(String::as_str),
            Some("persisted")
        );
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn effective_operator_env_map_excludes_runtime_projection_keys() {
        let root = make_temp_root();
        let mut env_map = BTreeMap::new();
        env_map.insert("CTOX_DISABLE_MISSION_WATCHDOG".to_string(), "1".to_string());
        env_map.insert("CTOX_CHAT_MODEL".to_string(), "gpt-5.4".to_string());
        save_runtime_env_map(&root, &env_map).unwrap();

        let effective = effective_operator_env_map(&root).unwrap();

        assert_eq!(
            effective
                .get("CTOX_DISABLE_MISSION_WATCHDOG")
                .map(String::as_str),
            Some("1")
        );
        assert!(!effective.contains_key("CTOX_CHAT_MODEL"));
        assert!(!effective.contains_key("CTOX_ACTIVE_MODEL"));
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn configured_chat_model_prefers_explicit_model_over_stale_base() {
        let mut env_map = BTreeMap::new();
        env_map.insert(
            "CTOX_CHAT_MODEL_BASE".to_string(),
            "Qwen/Qwen3.5-4B".to_string(),
        );
        env_map.insert("CTOX_CHAT_MODEL".to_string(), "Qwen/Qwen3.5-9B".to_string());

        assert_eq!(
            configured_chat_model_from_map(&env_map).as_deref(),
            Some("Qwen/Qwen3.5-9B")
        );
    }

    #[test]
    fn env_or_config_reads_persisted_non_secret_values() {
        let root = make_temp_root();
        let mut env_map = BTreeMap::new();
        env_map.insert("CTOX_DISABLE_MISSION_WATCHDOG".to_string(), "0".to_string());
        save_runtime_env_map(&root, &env_map).unwrap();

        assert_eq!(
            env_or_config(&root, "CTOX_DISABLE_MISSION_WATCHDOG").as_deref(),
            Some("0")
        );
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn env_or_config_reads_secrets_only_from_store() {
        let root = make_temp_root();

        secrets::set_credential(&root, "OPENAI_API_KEY", "sk-store").unwrap();
        secrets::set_credential(&root, "OPENROUTER_API_KEY", "or-store").unwrap();

        assert_eq!(
            env_or_config(&root, "OPENAI_API_KEY").as_deref(),
            Some("sk-store")
        );
        assert_eq!(
            env_or_config(&root, "OPENROUTER_API_KEY").as_deref(),
            Some("or-store")
        );
        assert_eq!(env_or_config(&root, "CTOX_CUDA_HOME").as_deref(), None);
        assert_eq!(env_or_config(&root, "CTOX_ENGINE_LOG").as_deref(), None);

        let effective = effective_runtime_env_map(&root).unwrap();
        assert_eq!(
            effective.get("OPENAI_API_KEY").map(String::as_str),
            Some("sk-store")
        );
        assert_eq!(
            effective.get("OPENROUTER_API_KEY").map(String::as_str),
            Some("or-store")
        );
        assert_eq!(
            effective.get("CTOX_CUDA_HOME").map(String::as_str),
            Some("/opt/cuda")
        );
        assert_eq!(
            effective.get("CTOX_ENGINE_LOG").map(String::as_str),
            Some("/tmp/ctox-engine.log")
        );

        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn save_runtime_state_projection_preserves_non_runtime_keys() {
        let root = make_temp_root();
        let mut env_map = BTreeMap::new();
        env_map.insert("CTOX_DISABLE_MISSION_WATCHDOG".to_string(), "1".to_string());
        env_map.insert("CTOX_ENGINE_MODEL".to_string(), "stale".to_string());

        let state = runtime_state::InferenceRuntimeState {
            version: 4,
            source: runtime_state::InferenceSource::Api,
            local_runtime: runtime_state::LocalRuntimeKind::Candle,
            base_model: Some("gpt-5.4".to_string()),
            requested_model: Some("gpt-5.4".to_string()),
            active_model: Some("gpt-5.4".to_string()),
            engine_model: None,
            engine_port: None,
            configured_context_tokens: None,
            realized_context_tokens: None,
            upstream_base_url: runtime_state::default_api_upstream_base_url().to_string(),
            local_preset: None,
            boost: runtime_state::BoostRuntimeState::default(),
            adapter_tuning: runtime_state::AdapterRuntimeTuning::default(),
            embedding: runtime_state::AuxiliaryRuntimeState::default(),
            transcription: runtime_state::AuxiliaryRuntimeState::default(),
            speech: runtime_state::AuxiliaryRuntimeState::default(),
            vision: runtime_state::AuxiliaryRuntimeState::default(),
        };

        save_runtime_state_projection(&root, &state, &env_map).unwrap();
        let persisted = load_runtime_env_map(&root).unwrap();

        assert_eq!(
            persisted
                .get("CTOX_DISABLE_MISSION_WATCHDOG")
                .map(String::as_str),
            Some("1")
        );
        assert_eq!(
            persisted.get("CTOX_ACTIVE_MODEL").map(String::as_str),
            Some("gpt-5.4")
        );
        assert!(!persisted.contains_key("CTOX_ENGINE_MODEL"));
        std::fs::remove_dir_all(root).unwrap();
    }
}
