// ref: internal/store/postgres_cooldown_store_test.go @ a88197f845c979132c8978ea223c6af05cc81536
// Port-Status: adapted_to_ctox
// License: MIT (upstream); modifications AGPL-3.0-only

use std::collections::BTreeMap;
use std::fs;
use std::path::Path;
use std::sync::{Arc, Mutex};

use chrono::{DateTime, Utc};
use serde_json::{json, Value};
use tempfile::TempDir;

use crate::sdk::cliproxy::auth::{Auth, CooldownStateRecord};

use super::objectstore::{ObjectBackend, ObjectEntry, ObjectStoreConfig, ObjectTokenStore};
use super::postgres_cooldown_store::PostgresCooldownStateStore;
use super::postgresstore::{
    CooldownMutation, CooldownRow, PostgresBackend, PostgresStore, PostgresStoreConfig, StoreError,
    StoredAuthRecord,
};

#[derive(Default)]
struct MemoryPostgres {
    config: Mutex<BTreeMap<String, Vec<u8>>>,
    auth: Mutex<BTreeMap<String, StoredAuthRecord>>,
    cooldown: Mutex<BTreeMap<(String, String), CooldownRow>>,
}

impl PostgresBackend for MemoryPostgres {
    fn ensure_schema(&self, _: &PostgresStoreConfig) -> Result<(), StoreError> {
        Ok(())
    }

    fn load_config(&self, key: &str) -> Result<Option<Vec<u8>>, StoreError> {
        Ok(self.config.lock().unwrap().get(key).cloned())
    }

    fn put_config(&self, key: &str, content: &[u8]) -> Result<(), StoreError> {
        self.config
            .lock()
            .unwrap()
            .insert(key.to_owned(), content.to_vec());
        Ok(())
    }

    fn delete_config(&self, key: &str) -> Result<(), StoreError> {
        self.config.lock().unwrap().remove(key);
        Ok(())
    }

    fn list_auth(&self) -> Result<Vec<StoredAuthRecord>, StoreError> {
        Ok(self.auth.lock().unwrap().values().cloned().collect())
    }

    fn put_auth(&self, id: &str, content: &[u8]) -> Result<(), StoreError> {
        let now = DateTime::<Utc>::from(std::time::UNIX_EPOCH);
        self.auth.lock().unwrap().insert(
            id.to_owned(),
            StoredAuthRecord {
                id: id.to_owned(),
                content: content.to_vec(),
                created_at: now,
                updated_at: now,
            },
        );
        Ok(())
    }

    fn delete_auth(&self, id: &str) -> Result<(), StoreError> {
        self.auth.lock().unwrap().remove(id);
        Ok(())
    }

    fn load_cooldowns(&self) -> Result<Vec<CooldownRow>, StoreError> {
        Ok(self.cooldown.lock().unwrap().values().cloned().collect())
    }

    fn apply_cooldown_mutations(&self, mutations: &[CooldownMutation]) -> Result<(), StoreError> {
        let mut rows = self.cooldown.lock().unwrap();
        for mutation in mutations {
            let key = (mutation.row.auth_id.clone(), mutation.row.model.clone());
            match mutation.delete_if_not_newer_than_ms {
                Some(observed) => {
                    let may_delete = rows
                        .get(&key)
                        .is_some_and(|row| !row.deleted && row.updated_at_ms <= observed);
                    if may_delete {
                        rows.insert(key, mutation.row.clone());
                    }
                }
                None => {
                    let may_write = rows
                        .get(&key)
                        .is_none_or(|row| row.updated_at_ms <= mutation.row.updated_at_ms);
                    if may_write {
                        rows.insert(key, mutation.row.clone());
                    }
                }
            }
        }
        Ok(())
    }
}

fn postgres_store(root: &Path, backend: Arc<MemoryPostgres>) -> PostgresStore {
    PostgresStore::new(
        PostgresStoreConfig {
            schema: "cliproxy".to_owned(),
            config_table: String::new(),
            auth_table: String::new(),
            cooldown_table: String::new(),
            spool_dir: root.to_path_buf(),
        },
        backend,
    )
    .unwrap()
}

fn cooldown(auth_id: &str, model: Option<&str>, updated_at_ms: i64) -> CooldownStateRecord {
    CooldownStateRecord {
        provider: "codex".to_owned(),
        auth_id: auth_id.to_owned(),
        model: model.map(str::to_owned),
        status: "cooling".to_owned(),
        next_retry_after_ms: Some(updated_at_ms + 1_000),
        reason: "rate_limit".to_owned(),
        quota: Default::default(),
        last_error: None,
        updated_at_ms,
    }
}

#[derive(Default)]
struct MemoryObjects(Mutex<BTreeMap<String, Vec<u8>>>);

impl ObjectBackend for MemoryObjects {
    fn ensure_bucket(&self, _: &str, _: &str) -> Result<(), StoreError> {
        Ok(())
    }

    fn get(&self, _: &str, key: &str) -> Result<Option<Vec<u8>>, StoreError> {
        Ok(self.0.lock().unwrap().get(key).cloned())
    }

    fn list(&self, _: &str, prefix: &str) -> Result<Vec<ObjectEntry>, StoreError> {
        Ok(self
            .0
            .lock()
            .unwrap()
            .iter()
            .filter(|(key, _)| key.starts_with(prefix))
            .map(|(key, value)| ObjectEntry {
                key: key.clone(),
                content: value.clone(),
            })
            .collect())
    }

    fn put(&self, _: &str, key: &str, content: &[u8], _: &str) -> Result<(), StoreError> {
        self.0
            .lock()
            .unwrap()
            .insert(key.to_owned(), content.to_vec());
        Ok(())
    }

    fn delete(&self, _: &str, key: &str) -> Result<(), StoreError> {
        self.0.lock().unwrap().remove(key);
        Ok(())
    }
}
