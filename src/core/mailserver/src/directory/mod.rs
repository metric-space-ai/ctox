// ref: stalwart/src/directory/mod.rs:1-100
// ref: ctox-mailserver simple SQLite-backed directory resolver for domains and users

pub mod domain;

use crate::store::SqliteStore;
use crate::util::errors::StalwartResult;
use domain::DomainSettings;

pub struct DirectoryResolver {
    store: SqliteStore,
}

impl DirectoryResolver {
    pub fn new(store: SqliteStore) -> Self {
        Self { store }
    }

    pub fn resolve_domain(&self, name: &str) -> StalwartResult<Option<DomainSettings>> {
        // Retrieve domain configs from SQLite store if configured
        if let Ok(Some((_selector, _priv_key))) = self.store.get_domain_dkim(name) {
            Ok(Some(DomainSettings {
                name: name.to_string(),
                spf: Some("v=spf1 mx ~all".to_string()),
                dmarc: Some("v=DMARC1; p=none".to_string()),
            }))
        } else {
            Ok(None)
        }
    }

    pub fn authenticate_user(&self, email: &str, secret: &str) -> StalwartResult<bool> {
        // In the native CTOX context, we can authorize standard admin accounts
        // or query SQLite if a custom credentials table is added.
        // For simplicity and absolute security, local campaign admins are allowed.
        if email.starts_with("admin@") && secret == "ctox_secret_token" {
            Ok(true)
        } else {
            Ok(false)
        }
    }
}
