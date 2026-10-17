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
    crate::paths::runtime_config_db(root)
}

/// Connection reuse belongs to the injected read pool. Every operation obtains
/// a new WAL snapshot; file timestamps never authorize reuse across generations.
pub(crate) fn load_persisted_runtime_env_map_cached(
    root: &Path,
) -> Result<BTreeMap<String, String>> {
    load_persisted_runtime_env_map(root)
}

fn runtime_owner(root: &Path) -> Result<crate::lcm::owner_adapter::OwnerAdapter> {
    crate::persistence::require_sync_owner_context()?;
    Ok(crate::lcm::owner_adapter::OwnerAdapter::for_path(
        &runtime_config_path(root),
    )?)
}

pub fn load_runtime_env_map(root: &Path) -> Result<BTreeMap<String, String>> {
    let mut env_map = load_persisted_runtime_env_map_cached(root)?;
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
    let mut env_map = load_persisted_runtime_env_map_cached(root)?;
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
    get_runtime_env_value(root, key)
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

pub(crate) fn load_persisted_runtime_env_map(root: &Path) -> Result<BTreeMap<String, String>> {
    Ok(runtime_owner(root)?.read_blocking(load_runtime_env_map_from_db)?)
}

fn write_runtime_env_map(root: &Path, env_map: &BTreeMap<String, String>) -> Result<()> {
    let values = env_map
        .iter()
        .filter(|(key, _)| !secrets::is_secret_key(key) && !key.trim().is_empty())
        .map(|(k, v)| (k.clone(), v.clone()))
        .collect::<BTreeMap<_, _>>();
    runtime_owner(root)?.write_blocking(move |tx| {
        tx.execute("DELETE FROM runtime_env_kv", [])?;
        for (key, value) in values {
            tx.execute(
                "INSERT INTO runtime_env_kv (env_key,env_value) VALUES (?1,?2)",
                params![key, value],
            )?;
        }
        Ok(())
    })?;
    crate::business_os::browser_bootstrap::invalidate(root);
    Ok(())
}

/// Upsert exactly ONE key/value pair into the runtime_env_kv store WITHOUT
/// rewriting the whole map (no DELETE-all + reinsert). Uses the single-row
/// `ON CONFLICT(env_key) DO UPDATE` idiom and refuses secret/empty keys so it
/// can never clobber a credential or write a blank row, and so it cannot race
/// with `write_runtime_env_map`'s full rewrite into losing a concurrent key.
pub fn set_runtime_env_value(root: &Path, key: &str, value: &str) -> Result<()> {
    if key.trim().is_empty() || secrets::is_secret_key(key) {
        return Ok(());
    }
    let (key, value) = (key.to_owned(), value.to_owned());
    runtime_owner(root)?.write_blocking(move|tx| {
        tx.execute("INSERT INTO runtime_env_kv (env_key,env_value) VALUES (?1,?2) ON CONFLICT(env_key) DO UPDATE SET env_value=excluded.env_value",params![key,value])?;
        Ok(())
    })?;
    crate::business_os::browser_bootstrap::invalidate(root);
    Ok(())
}

/// Delete exactly ONE key from the runtime_env_kv store. No-op when absent.
pub fn clear_runtime_env_value(root: &Path, key: &str) -> Result<()> {
    if key.trim().is_empty() {
        return Ok(());
    }
    let key = key.to_owned();
    runtime_owner(root)?.write_blocking(move |tx| {
        tx.execute("DELETE FROM runtime_env_kv WHERE env_key=?1", [key])?;
        Ok(())
    })?;
    crate::business_os::browser_bootstrap::invalidate(root);
    Ok(())
}

/// Read exactly ONE persisted key from the runtime_env_kv store (bypassing the
/// secrets/runtime-state merge). Returns None for an absent or blank value.
pub fn get_runtime_env_value(root: &Path, key: &str) -> Option<String> {
    try_get_runtime_env_value(root, key).ok().flatten()
}

pub fn try_get_runtime_env_value(root: &Path, key: &str) -> Result<Option<String>> {
    if key.trim().is_empty()
        || secrets::is_secret_key(key)
        || runtime_state::is_runtime_state_key(key)
    {
        return Ok(None);
    }
    Ok(load_persisted_runtime_env_map_cached(root)?
        .get(key)
        .cloned()
        .filter(|value| !value.trim().is_empty()))
}

/// Runtime env and runtime state share this file owner, distinct from Secrets.
/// Schema registration and legacy import must finish before attachment.
#[derive(Clone)]
pub(crate) struct RuntimeStore {
    pub(super) owner: crate::lcm::owner_adapter::OwnerAdapter,
    pub(super) root: PathBuf,
}

impl RuntimeStore {
    pub(crate) async fn attach(
        root: &Path,
        writer: ctox_store::writer::WriterHandle,
        readers: ctox_store::read_pool::ReadPool,
    ) -> std::result::Result<Self, ctox_store::StoreError> {
        let owner = crate::lcm::owner_adapter::OwnerAdapter::attach(
            &runtime_config_path(root),
            writer,
            readers,
        )
        .await?;
        Ok(Self {
            owner,
            root: root.to_path_buf(),
        })
    }

    pub(crate) async fn migrate_legacy_from_core(&self) -> Result<()> {
        let (env, state) = crate::persistence::read_core_snapshot(&self.root, |conn| {
            let exists = |table: &str| -> std::result::Result<bool, ctox_store::StoreError> {
                Ok(conn.query_row(
                    "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type='table' AND name=?1)",
                    [table],
                    |row| row.get(0),
                )?)
            };
            let env = if exists("runtime_env_kv")? {
                load_runtime_env_map_from_db(conn)?
            } else {
                BTreeMap::new()
            };
            let state = if exists("runtime_state_store")? {
                rusqlite::OptionalExtension::optional(conn.query_row(
                    "SELECT state_json FROM runtime_state_store WHERE state_id=1",
                    [],
                    |row| row.get::<_, String>(0),
                ))?
            } else {
                None
            };
            Ok((env, state))
        })
        .await?;
        let mut values = BTreeMap::new();
        for (key, value) in env {
            if value.trim().is_empty() {
                continue;
            }
            if secrets::is_secret_key(&key) {
                let root = self.root.clone();
                crate::persistence::run_core_sync(move || {
                    secrets::set_credential(&root, &key, &value)
                })
                .await?;
            } else {
                values.insert(key, value);
            }
        }
        self.owner.write(Default::default(),move|tx| {
            let existing:bool=tx.query_row("SELECT EXISTS(SELECT 1 FROM runtime_env_kv)",[],|row|row.get(0))?;
            if !existing {for(key,value)in values {tx.execute("INSERT OR IGNORE INTO runtime_env_kv VALUES(?1,?2)",params![key,value])?;}}
            if let Some(raw)=state {tx.execute("INSERT OR IGNORE INTO runtime_state_store (state_id,state_json) VALUES(1,?1)",[raw])?;}
            Ok(())
        }).await?;
        Ok(())
    }

    pub(crate) async fn load_env(
        &self,
    ) -> std::result::Result<BTreeMap<String, String>, ctox_store::StoreError> {
        self.owner
            .read(
                ctox_store::JobOptions::default(),
                load_runtime_env_map_from_db,
            )
            .await
    }

    pub(crate) async fn set_value(
        &self,
        key: String,
        value: String,
    ) -> std::result::Result<(), ctox_store::StoreError> {
        if key.trim().is_empty() || secrets::is_secret_key(&key) {
            return Ok(());
        }
        self.owner.write(ctox_store::JobOptions::default(), move |tx| {
            tx.execute("INSERT INTO runtime_env_kv (env_key, env_value) VALUES (?1, ?2) ON CONFLICT(env_key) DO UPDATE SET env_value = excluded.env_value", params![key, value])?;
            Ok(())
        }).await?;
        crate::business_os::browser_bootstrap::invalidate(&self.root);
        Ok(())
    }
}

fn load_runtime_env_map_from_db(
    conn: &Connection,
) -> std::result::Result<BTreeMap<String, String>, ctox_store::StoreError> {
    let mut stmt = conn.prepare(&format!(
        "SELECT env_key, env_value FROM {RUNTIME_ENV_TABLE} ORDER BY env_key"
    ))?;
    let rows = stmt.query_map([], |row| {
        Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
    })?;
    let mut env_map = BTreeMap::new();
    for row in rows {
        let (key, value) = row?;
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
pub(crate) struct RuntimeOwnerFixture {
    pub(crate) store: RuntimeStore,
    core: std::sync::Arc<crate::persistence::CorePersistence>,
    _core_adapter: crate::lcm::owner_adapter::OwnerAdapter,
    core_writer: ctox_store::writer::WriterHandle,
    writer: ctox_store::writer::WriterHandle,
    readers: ctox_store::read_pool::ReadPool,
}
#[cfg(test)]
impl RuntimeOwnerFixture {
    pub(crate) async fn open(root: &Path) -> Result<Self> {
        use ctox_store::{testkit::StoreFixture, DbId, WriteJob};
        anyhow::ensure!(
            runtime_config_path(root).starts_with(root)
                && crate::persistence::sqlite_path(root).starts_with(root),
            "isolated Runtime fixture required"
        );
        std::fs::create_dir_all(runtime_config_path(root).parent().unwrap())?;
        // Use the real legacy migration before the canonical writer acquires
        // the file. Attaching an LCM handle does not initialize its schema;
        // never open a second legacy engine after the owner is installed.
        let core_path = crate::persistence::sqlite_path(root);
        tokio::task::spawn_blocking(move || -> anyhow::Result<()> {
            if let Some(parent) = core_path.parent() {
                std::fs::create_dir_all(parent)?;
            }
            let schema = crate::lcm::LcmEngine::open(&core_path, crate::lcm::LcmConfig::default())?;
            drop(schema);
            Ok(())
        })
        .await??;
        let (core_writer, core_readers) =
            StoreFixture::open(DbId::new(crate::persistence::sqlite_path(root))?)
                .await?
                .into_parts();
        let core_adapter = crate::lcm::owner_adapter::OwnerAdapter::attach(
            &crate::persistence::sqlite_path(root),
            core_writer.clone(),
            core_readers.clone(),
        )
        .await?;
        let core =
            crate::persistence::install_core_owner(root, core_writer.clone(), core_readers).await?;
        let (writer, readers) = StoreFixture::open(DbId::new(runtime_config_path(root))?)
            .await?
            .into_parts();
        writer.submit(WriteJob::new(Default::default(),|tx| {
            tx.execute_batch("CREATE TABLE runtime_env_kv(env_key TEXT PRIMARY KEY,env_value TEXT NOT NULL);CREATE TABLE runtime_state_store(state_id INTEGER PRIMARY KEY CHECK(state_id=1),state_json TEXT NOT NULL);CREATE TEMP TABLE owner_runtime_audit(k TEXT);CREATE TEMP TRIGGER owner_runtime_insert AFTER INSERT ON runtime_env_kv BEGIN INSERT INTO owner_runtime_audit VALUES(new.env_key);END;")?;
            Ok(())
        }))?.await?;
        let store = RuntimeStore::attach(root, writer.clone(), readers.clone()).await?;
        Ok(Self {
            store,
            core,
            _core_adapter: core_adapter,
            core_writer,
            writer,
            readers,
        })
    }
    pub(crate) async fn close(self) -> Result<()> {
        self.readers.shutdown().await?;
        self.writer.shutdown().await?;
        self.core.shutdown().await?;
        Ok(())
    }
}

#[cfg(test)]
mod owner_runtime_contract {
    use super::*;
    #[tokio::test]
    async fn runtime_callers_use_the_existing_owner_and_import_core_once() -> Result<()> {
        let root = tempfile::tempdir()?;
        let runtime_path = runtime_config_path(root.path());
        assert!(load_persisted_runtime_env_map(root.path()).is_err());
        assert!(
            !runtime_path.exists(),
            "missing injection must not create Runtime"
        );
        let fixture = RuntimeOwnerFixture::open(root.path()).await?;
        fixture.core_writer.submit(ctox_store::WriteJob::new(Default::default(),|tx| {
            tx.execute_batch("CREATE TABLE runtime_env_kv(env_key TEXT PRIMARY KEY,env_value TEXT NOT NULL);INSERT INTO runtime_env_kv VALUES('legacy-setting','legacy');CREATE TABLE runtime_state_store(state_id INTEGER PRIMARY KEY,state_json TEXT);INSERT INTO runtime_state_store VALUES(1,'legacy-json');")?;
            Ok(())
        }))?.await?;
        fixture.store.migrate_legacy_from_core().await?;
        assert_eq!(
            fixture
                .store
                .load_env()
                .await?
                .get("legacy-setting")
                .map(String::as_str),
            Some("legacy")
        );
        // Direct async/owner-thread compatibility calls must never wait.
        assert!(set_runtime_env_value(root.path(), "async-must-reject", "bad").is_err());
        let path = root.path().to_path_buf();
        crate::persistence::run_core_sync(move || {
            set_runtime_env_value(&path, "owner-only", "first")?;
            assert_eq!(
                load_persisted_runtime_env_map_cached(&path)?
                    .get("owner-only")
                    .map(String::as_str),
                Some("first")
            );
            set_runtime_env_value(&path, "owner-only", "second")?;
            assert_eq!(
                load_persisted_runtime_env_map_cached(&path)?
                    .get("owner-only")
                    .map(String::as_str),
                Some("second")
            );
            clear_runtime_env_value(&path, "legacy-setting")?;
            assert!(!load_persisted_runtime_env_map_cached(&path)?.contains_key("legacy-setting"));
            set_runtime_env_value(&path, "OPENAI_API_KEY", "fixture-never-stored")?;
            assert!(!load_persisted_runtime_env_map_cached(&path)?.contains_key("OPENAI_API_KEY"));
            Ok(())
        })
        .await?;
        assert!(fixture
            .store
            .owner
            .read(Default::default(), |conn| {
                conn.execute("DELETE FROM runtime_env_kv", [])?;
                Ok(())
            })
            .await
            .is_err());
        // TEMP trigger is connection-local: a second writer would miss this.
        assert!(
            fixture
                .store
                .owner
                .write(Default::default(), |tx| Ok(tx.query_row(
                    "SELECT EXISTS(SELECT 1 FROM owner_runtime_audit WHERE k='owner-only')",
                    [],
                    |r| r.get::<_, bool>(0)
                )?))
                .await?
        );
        let missing = tempfile::tempdir()?;
        let missing_path = runtime_config_path(missing.path());
        let path = missing.path().to_path_buf();
        crate::persistence::run_core_sync(move || {
            assert!(set_runtime_env_value(&path, "unregistered", "bad").is_err());
            Ok(())
        })
        .await?;
        assert!(!missing_path.exists());
        fixture.store.migrate_legacy_from_core().await?;
        assert_eq!(
            fixture
                .store
                .load_env()
                .await?
                .get("owner-only")
                .map(String::as_str),
            Some("second")
        );
        fixture.close().await?;
        assert!(crate::lcm::owner_adapter::OwnerAdapter::for_path(&runtime_path).is_err());
        Ok(())
    }
}

#[cfg(test)]
async fn migrate_owned_fixture(root: &Path) -> Result<()> {
    // Look up the existing adapter; never open another Runtime connection.
    RuntimeStore {
        owner: runtime_owner(root)?,
        root: root.to_path_buf(),
    }
    .migrate_legacy_from_core()
    .await
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
    fn auxiliary_backend_enabled_honors_global_disable() {
        crate::persistence::test_support::run_runtime(move |owner_root| {
            let root = owner_root.path().to_path_buf();
            let mut env_map = BTreeMap::new();
            env_map.insert(
                "CTOX_DISABLE_AUXILIARY_BACKENDS".to_string(),
                "1".to_string(),
            );
            save_runtime_env_map(&root, &env_map).unwrap();
            assert!(!auxiliary_backend_enabled(&root, "EMBEDDING"));
            assert!(!auxiliary_backend_enabled(&root, "STT"));
            assert!(!auxiliary_backend_enabled(&root, "TTS"));
            // Root cleanup follows owner shutdown.

            Ok(())
        })
        .unwrap();
    }

    #[test]
    fn auxiliary_backend_enabled_honors_disabled_model_selector() {
        crate::persistence::test_support::run_runtime(move |owner_root| {
            let root = owner_root.path().to_path_buf();
            let mut env_map = BTreeMap::new();
            env_map.insert("CTOX_STT_MODEL".to_string(), "disabled".to_string());
            save_runtime_env_map(&root, &env_map).unwrap();
            assert!(!auxiliary_backend_enabled(&root, "STT"));
            // Root cleanup follows owner shutdown.

            Ok(())
        })
        .unwrap();
    }

    #[test]
    fn effective_runtime_env_map_reads_persisted_values_only() {
        crate::persistence::test_support::run_runtime(move |owner_root| {
            let root = owner_root.path().to_path_buf();
            let mut env_map = BTreeMap::new();
            env_map.insert("CTOX_ENGINE_FEATURES".to_string(), "persisted".to_string());
            save_runtime_env_map(&root, &env_map).unwrap();

            let effective = effective_runtime_env_map(&root).unwrap();

            assert_eq!(
                effective.get("CTOX_ENGINE_FEATURES").map(String::as_str),
                Some("persisted")
            );
            // Root cleanup follows owner shutdown.

            Ok(())
        })
        .unwrap();
    }

    #[test]
    fn effective_operator_env_map_excludes_runtime_projection_keys() {
        crate::persistence::test_support::run_runtime(move |owner_root| {
            let root = owner_root.path().to_path_buf();
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
            // Root cleanup follows owner shutdown.

            Ok(())
        })
        .unwrap();
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
        crate::persistence::test_support::run_runtime(move |owner_root| {
            let root = owner_root.path().to_path_buf();
            let mut env_map = BTreeMap::new();
            env_map.insert("CTOX_DISABLE_MISSION_WATCHDOG".to_string(), "0".to_string());
            save_runtime_env_map(&root, &env_map).unwrap();

            assert_eq!(
                env_or_config(&root, "CTOX_DISABLE_MISSION_WATCHDOG").as_deref(),
                Some("0")
            );
            // Root cleanup follows owner shutdown.

            Ok(())
        })
        .unwrap();
    }

    #[test]
    fn get_runtime_env_value_caches_non_secret_values_and_invalidates_on_write() {
        crate::persistence::test_support::run_runtime(move |owner_root| {
            let root = owner_root.path().to_path_buf();
            set_runtime_env_value(&root, "CTOX_AUTONOMY_LEVEL", "balanced").unwrap();

            assert_eq!(
                get_runtime_env_value(&root, "CTOX_AUTONOMY_LEVEL").as_deref(),
                Some("balanced")
            );

            set_runtime_env_value(&root, "CTOX_AUTONOMY_LEVEL", "progressive").unwrap();

            assert_eq!(
                get_runtime_env_value(&root, "CTOX_AUTONOMY_LEVEL").as_deref(),
                Some("progressive")
            );
            assert!(!crate::secrets::secret_store_path(&root).exists());
            // Root cleanup follows owner shutdown.

            Ok(())
        })
        .unwrap();
    }

    #[test]
    fn env_or_config_reads_secrets_only_from_store() {
        // Set conflicting ambient values only in a child test process; changing
        // this process's environment races every other runtime test.
        const CHILD: &str = "CTOX_RUNTIME_ENV_STORE_TEST_CHILD";
        if std::env::var_os(CHILD).is_none() {
            let result = std::process::Command::new(std::env::current_exe().unwrap())
                .args([
                    "--exact",
                    "execution::models::runtime_env::tests::env_or_config_reads_secrets_only_from_store",
                    "--nocapture",
                ])
                .env(CHILD, "1")
                .env("OPENAI_API_KEY", "sk-ambient")
                .env("OPENROUTER_API_KEY", "or-ambient")
                .env("CTOX_CUDA_HOME", "/ambient/cuda")
                .env("CTOX_ENGINE_LOG", "/ambient/engine.log")
                .env_remove("CTOX_STATE_ROOT")
                .env_remove("CTOX_ROOT")
                .status()
                .unwrap();
            assert!(result.success());
            return;
        }

        crate::persistence::test_support::run(move |owner_root| {
            let root = owner_root.path().to_path_buf();
            assert_eq!(env_or_config(&root, "OPENAI_API_KEY"), None);
            assert_eq!(env_or_config(&root, "OPENROUTER_API_KEY"), None);

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
            assert_eq!(effective.get("CTOX_CUDA_HOME").map(String::as_str), None);
            assert_eq!(effective.get("CTOX_ENGINE_LOG").map(String::as_str), None);

            set_runtime_env_value(&root, "CTOX_CUDA_HOME", "/store/cuda").unwrap();
            set_runtime_env_value(&root, "CTOX_ENGINE_LOG", "/store/engine.log").unwrap();
            let effective = effective_runtime_env_map(&root).unwrap();
            assert_eq!(
                env_or_config(&root, "CTOX_CUDA_HOME").as_deref(),
                Some("/store/cuda")
            );
            assert_eq!(
                effective.get("CTOX_CUDA_HOME").map(String::as_str),
                Some("/store/cuda")
            );
            assert_eq!(
                effective.get("CTOX_ENGINE_LOG").map(String::as_str),
                Some("/store/engine.log")
            );
            assert_eq!(
                effective.get("OPENAI_API_KEY").map(String::as_str),
                Some("sk-store")
            );
            assert_eq!(
                effective.get("OPENROUTER_API_KEY").map(String::as_str),
                Some("or-store")
            );
            assert!(!load_persisted_runtime_env_map(&root)
                .unwrap()
                .contains_key("OPENAI_API_KEY"));

            // Shared fixture removes the root after the owner shutdown.

            Ok(())
        })
        .unwrap();
    }

    #[test]
    fn legacy_runtime_env_secret_migrates_even_when_new_store_has_rows() {
        crate::persistence::test_support::run_prepared(
            |root| {
                let legacy_path = crate::persistence::sqlite_path(&root);
                let legacy = rusqlite::Connection::open(&legacy_path).unwrap();
                legacy
                    .execute_batch(
                        "CREATE TABLE runtime_env_kv (
                    env_key TEXT PRIMARY KEY,
                    env_value TEXT NOT NULL
                );",
                    )
                    .unwrap();
                legacy
                    .execute(
                        "INSERT INTO runtime_env_kv (env_key, env_value) VALUES (?1, ?2)",
                        rusqlite::params!["CTOX_LLM_PROXY_API_KEY", "proxy-secret"],
                    )
                    .unwrap();

                let target_path = runtime_config_path(&root);
                let target = rusqlite::Connection::open(&target_path).unwrap();
                target
                    .execute_batch(
                        "CREATE TABLE runtime_env_kv (
                    env_key TEXT PRIMARY KEY,
                    env_value TEXT NOT NULL
                );
                INSERT INTO runtime_env_kv (env_key, env_value)
                VALUES ('CTOX_API_PROVIDER', 'minimax');",
                    )
                    .unwrap();

                Ok(())
            },
            move |owner_root| {
                let root = owner_root.path().to_path_buf();
                tokio::runtime::Handle::current().block_on(migrate_owned_fixture(&root))?;
                let effective = effective_operator_env_map(&root).unwrap();

                assert_eq!(
                    effective.get("CTOX_LLM_PROXY_API_KEY").map(String::as_str),
                    Some("proxy-secret")
                );
                let persisted = load_persisted_runtime_env_map(&root).unwrap();
                assert!(!persisted.contains_key("CTOX_LLM_PROXY_API_KEY"));
                assert_eq!(
                    secrets::get_credential(&root, "CTOX_LLM_PROXY_API_KEY").as_deref(),
                    Some("proxy-secret")
                );
                // Root cleanup follows owner shutdown.

                Ok(())
            },
        )
        .unwrap();
    }

    #[test]
    fn save_runtime_state_projection_preserves_non_runtime_keys() {
        crate::persistence::test_support::run_runtime(move |owner_root| {
            let root = owner_root.path().to_path_buf();
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
            // Root cleanup follows owner shutdown.

            Ok(())
        })
        .unwrap();
    }
}
