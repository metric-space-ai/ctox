use std::env;
use std::sync::{Arc, Mutex, MutexGuard, OnceLock, RwLock};

use rxdb::plugins::storage_memory::index_mod::get_rx_storage_memory;
use rxdb::plugins::utils::utils_string::random_token;
use rxdb::types::RxStorage;

#[derive(Clone)]
pub struct TestStorageConfig {
    pub name: String,
    pub storage: Arc<dyn RxStorage>,
    pub has_encryption: Option<Arc<dyn Fn() -> String + Send + Sync>>,
}

impl TestStorageConfig {
    pub fn new(name: impl Into<String>, storage: Arc<dyn RxStorage>) -> Self {
        Self {
            name: name.into(),
            storage,
            has_encryption: None,
        }
    }

    pub fn with_encryption_password(
        mut self,
        password: impl Fn() -> String + Send + Sync + 'static,
    ) -> Self {
        self.has_encryption = Some(Arc::new(password));
        self
    }

    pub fn has_encryption(&self) -> bool {
        self.has_encryption.is_some()
    }
}

#[derive(Clone)]
pub struct TestConfig {
    pub storage: TestStorageConfig,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EnvVariables {
    pub default_storage: Option<String>,
    pub node_env: Option<String>,
}

static CONFIG: OnceLock<RwLock<Option<TestConfig>>> = OnceLock::new();
static INIT_DONE: OnceLock<()> = OnceLock::new();
static CONFIG_TEST_MUTEX: OnceLock<Mutex<()>> = OnceLock::new();

fn config_slot() -> &'static RwLock<Option<TestConfig>> {
    CONFIG.get_or_init(|| RwLock::new(None))
}

fn config_test_guard() -> MutexGuard<'static, ()> {
    CONFIG_TEST_MUTEX
        .get_or_init(|| Mutex::new(()))
        .lock()
        .expect("test config mutex poisoned")
}

pub fn is_deno() -> bool {
    false
}

pub fn is_bun() -> bool {
    false
}

pub fn is_node() -> bool {
    true
}

pub fn get_env_variables() -> EnvVariables {
    EnvVariables {
        default_storage: env::var("DEFAULT_STORAGE").ok(),
        node_env: env::var("NODE_ENV").ok(),
    }
}

pub fn default_storage() -> Option<String> {
    get_env_variables().default_storage
}

pub fn is_fast_mode() -> bool {
    get_env_variables().node_env.as_deref() == Some("fast")
}

pub fn init_test_environment() {
    INIT_DONE.get_or_init(|| ());
}

pub fn set_config(new_config: TestConfig) {
    let mut guard = config_slot().write().expect("test config lock poisoned");
    *guard = Some(new_config);
}

pub fn get_config() -> Result<TestConfig, String> {
    init_test_environment();
    config_slot()
        .read()
        .expect("test config lock poisoned")
        .clone()
        .ok_or_else(|| "testConfig not set".to_string())
}

pub fn get_encrypted_storage(
    base_storage: Option<Arc<dyn RxStorage>>,
) -> Result<Arc<dyn RxStorage>, String> {
    let storage = match base_storage {
        Some(storage) => storage,
        None => get_config()?.storage.storage,
    };

    Ok(storage)
}

pub fn is_not_one_of_these_storages(storage_names: &[&str]) -> Result<bool, String> {
    let storage_name = get_config()?.storage.name;
    Ok(!storage_names.iter().any(|name| *name == storage_name))
}

pub fn get_password() -> Result<String, String> {
    let storage = get_config()?.storage;
    Ok(match storage.has_encryption {
        Some(password_factory) => password_factory(),
        None => format!("test-password-{}", random_token(Some(10))),
    })
}

#[test]
fn runtime_detection_matches_rust_conformance_process() {
    assert!(!is_deno());
    assert!(!is_bun());
    assert!(is_node());
    assert_eq!(
        is_fast_mode(),
        env::var("NODE_ENV").ok().as_deref() == Some("fast")
    );
    assert_eq!(default_storage(), env::var("DEFAULT_STORAGE").ok());
    assert_eq!(
        get_env_variables(),
        EnvVariables {
            default_storage: env::var("DEFAULT_STORAGE").ok(),
            node_env: env::var("NODE_ENV").ok(),
        }
    );
}

#[test]
fn global_config_round_trips_storage_and_name() {
    let _guard = config_test_guard();
    let storage: Arc<dyn RxStorage> = get_rx_storage_memory(());
    set_config(TestConfig {
        storage: TestStorageConfig::new("memory", Arc::clone(&storage)),
    });

    let config = get_config().expect("configured test storage");
    assert_eq!(config.storage.name, "memory");
    assert_eq!(config.storage.storage.name(), "memory");
    assert!(!config.storage.has_encryption());

    let resolved_storage = get_encrypted_storage(None).expect("storage");
    assert_eq!(resolved_storage.name(), storage.name());
}

#[test]
fn storage_name_filter_matches_upstream_helper() {
    let _guard = config_test_guard();
    let storage: Arc<dyn RxStorage> = get_rx_storage_memory(());
    set_config(TestConfig {
        storage: TestStorageConfig::new("memory", storage),
    });

    assert!(!is_not_one_of_these_storages(&["memory", "sqlite"]).unwrap());
    assert!(is_not_one_of_these_storages(&["sqlite"]).unwrap());
}

#[test]
fn password_uses_encryption_provider_or_random_fallback() {
    let _guard = config_test_guard();
    let storage: Arc<dyn RxStorage> = get_rx_storage_memory(());
    set_config(TestConfig {
        storage: TestStorageConfig::new("encrypted-memory", Arc::clone(&storage))
            .with_encryption_password(|| "configured-secret".to_string()),
    });
    assert!(get_config().unwrap().storage.has_encryption());
    assert_eq!(get_password().unwrap(), "configured-secret");

    set_config(TestConfig {
        storage: TestStorageConfig::new("memory", storage),
    });
    let password = get_password().unwrap();
    assert!(password.starts_with("test-password-"));
    assert_eq!(password.len(), "test-password-".len() + 10);
}
