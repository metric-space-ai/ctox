use anyhow::Context;
use anyhow::Result;
use std::collections::BTreeMap;
use std::path::Path;
use std::path::PathBuf;

use crate::inference::runtime_state;
use crate::secrets;

const DEFAULT_RUNTIME_CONFIG_RELATIVE_PATH: &str = "runtime/engine.env";

pub fn runtime_config_path(root: &Path) -> PathBuf {
    root.join(DEFAULT_RUNTIME_CONFIG_RELATIVE_PATH)
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
    for (key, value) in std::env::vars() {
        if !process_env_override_allowed(&key)
            || value.trim().is_empty()
            || runtime_state::is_runtime_state_key(&key)
        {
            continue;
        }
        env_map.insert(key, value);
    }
    Ok(env_map)
}

pub fn save_runtime_env_map(root: &Path, env_map: &BTreeMap<String, String>) -> Result<()> {
    // Route secret keys into the encrypted store before persisting.
    let mut clean_map = env_map.clone();
    for (key, value) in env_map {
        if secrets::is_secret_key(key) && !value.trim().is_empty() {
            if let Err(e) = secrets::set_credential(root, key, value) {
                eprintln!("[secrets] failed to encrypt {key}: {e:#} — keeping in engine.env");
                continue;
            }
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
    // For secret keys, check encrypted store first, then process env.
    if secrets::is_secret_key(key) {
        if let Some(value) = secrets::get_credential(root, key) {
            if !value.trim().is_empty() {
                return Some(value);
            }
        }
    }
    process_env_value(key).or_else(|| {
        load_runtime_env_map(root)
            .ok()
            .and_then(|map| map.get(key).cloned())
            .filter(|value| !value.trim().is_empty())
    })
}

fn process_env_value(key: &str) -> Option<String> {
    if !process_env_override_allowed(key) {
        return None;
    }
    std::env::var(key)
        .ok()
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

fn parse_env_map(raw: &str) -> BTreeMap<String, String> {
    let mut out = BTreeMap::new();
    for line in raw.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        let Some((key, value)) = trimmed.split_once('=') else {
            continue;
        };
        let normalized_key = key.trim();
        if normalized_key.is_empty() {
            continue;
        }
        out.insert(normalized_key.to_string(), unescape_env_value(value.trim()));
    }
    out
}

fn load_persisted_runtime_env_map(root: &Path) -> Result<BTreeMap<String, String>> {
    let path = runtime_config_path(root);
    if !path.exists() {
        return Ok(BTreeMap::new());
    }
    let raw = std::fs::read_to_string(&path)
        .with_context(|| format!("failed to read runtime config {}", path.display()))?;
    let mut env_map = parse_env_map(&raw);

    // One-time migration: move any plaintext secret keys from engine.env
    // into the encrypted store and rewrite engine.env without them.
    let migrated = secrets::migrate_secrets_from_env_map(root, &mut env_map);
    if migrated > 0 {
        // Rewrite engine.env without the migrated secret keys.
        if let Err(e) = write_runtime_env_map(root, &env_map) {
            eprintln!("[secrets] failed to rewrite engine.env after migration: {e:#}");
        }
    }
    Ok(env_map)
}

fn write_runtime_env_map(root: &Path, env_map: &BTreeMap<String, String>) -> Result<()> {
    let path = runtime_config_path(root);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("failed to create runtime config dir {}", parent.display()))?;
    }
    let mut output = String::new();
    for (key, value) in env_map {
        if key.trim().is_empty() {
            continue;
        }
        output.push_str(key);
        output.push('=');
        output.push_str(&escape_env_value(value));
        output.push('\n');
    }
    std::fs::write(&path, output)
        .with_context(|| format!("failed to write runtime config {}", path.display()))
}

fn process_env_override_allowed(key: &str) -> bool {
    if key.starts_with("OPENAI_")
        || key.starts_with("OPENROUTER_")
        || key.starts_with("ANTHROPIC_")
        || key.starts_with("MINIMAX_")
        || key.starts_with("CODEX_")
        || key.starts_with("CTO_")
    {
        return true;
    }
    matches!(
        key,
        "HF_TOKEN"
            | "HF_HOME"
            | "HUGGINGFACE_HUB_TOKEN"
            | "PATH"
            | "LD_LIBRARY_PATH"
            | "DYLD_LIBRARY_PATH"
            | "LIBRARY_PATH"
            | "CPATH"
            | "CPLUS_INCLUDE_PATH"
            | "CUDA_HOME"
            | "CUDA_PATH"
            | "CUDA_ROOT"
            | "CUDA_TOOLKIT_ROOT_DIR"
            | "CUDA_BIN_PATH"
            | "CUDARC_CUDA_VERSION"
            | "NVCC"
            | "CUDACXX"
            | "CTOX_ENV"
            | "CTOX_ENGINE_ENV_FILE"
            | "CTOX_ENGINE_BINARY"
            | "CTOX_ENGINE_LOG"
            | "CTOX_CHAT_SKILL_PRESET"
            | "CTOX_CUDA_HOME"
            | "CTOX_CUDA_PATH"
            | "CTOX_CUDA_ROOT"
            | "CTOX_CUDA_BIN_PATH"
            | "CTOX_RESOURCE_SNAPSHOT_JSON"
            | "CTOX_TEST_GPU_TOTALS_MB"
            // Phase 1/2 refactor: direct-session gate + compact policy knobs.
            | "CTOX_USE_DIRECT_SESSION"
            | "CTOX_DEBUG_DIRECT_SESSION"
            | "CTOX_COMPACT_TRIGGER"
            | "CTOX_COMPACT_MODE"
            | "CTOX_COMPACT_FIXED_INTERVAL"
            | "CTOX_COMPACT_ADAPTIVE_THRESHOLD"
    )
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

fn escape_env_value(value: &str) -> String {
    if value.is_empty()
        || value.chars().any(|ch| {
            !(ch.is_ascii_alphanumeric()
                || matches!(ch, '_' | '-' | '.' | '/' | ':' | ',' | '@' | '%' | '+'))
        })
    {
        format!("\"{}\"", value.replace('\\', "\\\\").replace('"', "\\\""))
    } else {
        value.to_string()
    }
}

fn unescape_env_value(value: &str) -> String {
    let trimmed = value.trim();
    if trimmed.starts_with('"') && trimmed.ends_with('"') && trimmed.len() >= 2 {
        let inner = &trimmed[1..trimmed.len() - 1];
        let mut output = String::new();
        let mut chars = inner.chars();
        while let Some(ch) = chars.next() {
            if ch == '\\' {
                if let Some(next) = chars.next() {
                    output.push(next);
                }
            } else {
                output.push(ch);
            }
        }
        output
    } else {
        trimmed.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;
    use std::time::{SystemTime, UNIX_EPOCH};

    static ENV_LOCK: Mutex<()> = Mutex::new(());

    struct ScopedEnvVar {
        key: String,
        previous: Option<String>,
    }

    impl ScopedEnvVar {
        fn set(key: &str, value: &str) -> Self {
            let previous = std::env::var(key).ok();
            std::env::set_var(key, value);
            Self {
                key: key.to_string(),
                previous,
            }
        }
    }

    impl Drop for ScopedEnvVar {
        fn drop(&mut self) {
            if let Some(previous) = self.previous.as_ref() {
                std::env::set_var(&self.key, previous);
            } else {
                std::env::remove_var(&self.key);
            }
        }
    }

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
    fn effective_runtime_env_map_ignores_process_runtime_overrides() {
        let _guard = ENV_LOCK.lock().unwrap();
        let root = make_temp_root();
        let mut env_map = BTreeMap::new();
        env_map.insert("CTOX_ENGINE_FEATURES".to_string(), "persisted".to_string());
        save_runtime_env_map(&root, &env_map).unwrap();

        let _override = ScopedEnvVar::set("CTOX_ENGINE_FEATURES", "process");
        let effective = effective_runtime_env_map(&root).unwrap();

        assert_eq!(
            effective.get("CTOX_ENGINE_FEATURES").map(String::as_str),
            Some("persisted")
        );
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn effective_operator_env_map_excludes_runtime_projection_keys() {
        let _guard = ENV_LOCK.lock().unwrap();
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
    fn env_or_config_ignores_process_runtime_overrides() {
        let _guard = ENV_LOCK.lock().unwrap();
        let root = make_temp_root();
        let mut env_map = BTreeMap::new();
        env_map.insert("CTOX_DISABLE_MISSION_WATCHDOG".to_string(), "0".to_string());
        save_runtime_env_map(&root, &env_map).unwrap();

        let _override = ScopedEnvVar::set("CTOX_DISABLE_MISSION_WATCHDOG", "1");
        assert_eq!(
            env_or_config(&root, "CTOX_DISABLE_MISSION_WATCHDOG").as_deref(),
            Some("0")
        );
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn env_or_config_keeps_process_secrets_and_bootstrap_keys() {
        let _guard = ENV_LOCK.lock().unwrap();
        let root = make_temp_root();

        let _openai = ScopedEnvVar::set("OPENAI_API_KEY", "sk-test");
        let _openrouter = ScopedEnvVar::set("OPENROUTER_API_KEY", "or-test");
        let _cuda = ScopedEnvVar::set("CTOX_CUDA_HOME", "/opt/cuda");
        let _engine_log = ScopedEnvVar::set("CTOX_ENGINE_LOG", "/tmp/ctox-engine.log");

        assert_eq!(
            env_or_config(&root, "OPENAI_API_KEY").as_deref(),
            Some("sk-test")
        );
        assert_eq!(
            env_or_config(&root, "OPENROUTER_API_KEY").as_deref(),
            Some("or-test")
        );
        assert_eq!(
            env_or_config(&root, "CTOX_CUDA_HOME").as_deref(),
            Some("/opt/cuda")
        );
        assert_eq!(
            env_or_config(&root, "CTOX_ENGINE_LOG").as_deref(),
            Some("/tmp/ctox-engine.log")
        );

        let effective = effective_runtime_env_map(&root).unwrap();
        assert_eq!(
            effective.get("OPENAI_API_KEY").map(String::as_str),
            Some("sk-test")
        );
        assert_eq!(
            effective.get("OPENROUTER_API_KEY").map(String::as_str),
            Some("or-test")
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
            realized_context_tokens: None,
            proxy_host: runtime_state::default_proxy_host().to_string(),
            proxy_port: runtime_state::default_proxy_port(),
            upstream_base_url: runtime_state::default_api_upstream_base_url().to_string(),
            local_preset: None,
            boost: runtime_state::BoostRuntimeState::default(),
            adapter_tuning: runtime_state::AdapterRuntimeTuning::default(),
            embedding: runtime_state::AuxiliaryRuntimeState::default(),
            transcription: runtime_state::AuxiliaryRuntimeState::default(),
            speech: runtime_state::AuxiliaryRuntimeState::default(),
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
