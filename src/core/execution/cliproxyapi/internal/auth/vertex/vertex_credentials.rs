// ref: internal/auth/vertex/vertex_credentials.go @ a88197f845c979132c8978ea223c6af05cc81536
// Port-Status: adapted_to_ctox
// License: MIT (upstream); modifications AGPL-3.0-only

use std::fmt;
use std::path::Path;
use std::sync::Arc;

use serde::Serialize;
use serde_json::{Map, Value};
use zeroize::Zeroizing;

use crate::internal::auth::models::{TokenStorage, TokenStorageError};

#[derive(Clone, Eq, PartialEq)]
pub struct VertexCredentialHandle {
    scope: String,
    name: String,
}

impl VertexCredentialHandle {
    pub fn new(
        scope: impl Into<String>,
        name: impl Into<String>,
    ) -> Result<Self, VertexCredentialError> {
        let scope = scope.into();
        let name = name.into();
        if scope.trim().is_empty() || name.trim().is_empty() {
            return Err(VertexCredentialError::InvalidHandle);
        }
        Ok(Self { scope, name })
    }

    pub fn scope(&self) -> &str {
        &self.scope
    }

    pub fn name(&self) -> &str {
        &self.name
    }
}

impl fmt::Debug for VertexCredentialHandle {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("VertexCredentialHandle")
            .field("scope", &self.scope)
            .field("name", &self.name)
            .finish()
    }
}

pub trait VertexCredentialSecretStore: Send + Sync {
    fn store_service_account(
        &self,
        handle: &VertexCredentialHandle,
        serialized_record: &[u8],
    ) -> Result<(), VertexCredentialError>;
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum VertexCredentialError {
    EmptyServiceAccount,
    InvalidHandle,
    Serialize,
    Store,
}

impl fmt::Display for VertexCredentialError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::EmptyServiceAccount => "vertex credential service account is empty",
            Self::InvalidHandle => "vertex credential secret handle is invalid",
            Self::Serialize => "vertex credential serialization failed",
            Self::Store => "vertex credential secret-store write failed",
        })
    }
}

impl std::error::Error for VertexCredentialError {}

/// Upstream's credential record with persistence adapted to CTOX's injected
/// secret store. The service account is never exposed via Debug and the path
/// supplied by the legacy `TokenStorage` contract grants no filesystem access.
pub struct VertexCredentialStorage {
    service_account: Map<String, Value>,
    pub project_id: String,
    pub email: String,
    pub location: String,
    pub prefix: String,
    store: Arc<dyn VertexCredentialSecretStore>,
    handle: VertexCredentialHandle,
}

impl VertexCredentialStorage {
    pub fn new(
        service_account: Map<String, Value>,
        location: impl Into<String>,
        prefix: impl Into<String>,
        store: Arc<dyn VertexCredentialSecretStore>,
        handle: VertexCredentialHandle,
    ) -> Result<Self, VertexCredentialError> {
        if service_account.is_empty() {
            return Err(VertexCredentialError::EmptyServiceAccount);
        }
        let project_id = string_field(&service_account, "project_id");
        let email = string_field(&service_account, "client_email");
        Ok(Self {
            service_account,
            project_id,
            email,
            location: location.into(),
            prefix: prefix.into(),
            store,
            handle,
        })
    }

    pub fn service_account(&self) -> &Map<String, Value> {
        &self.service_account
    }

    pub fn handle(&self) -> &VertexCredentialHandle {
        &self.handle
    }

    fn serialized_record(&self) -> Result<Zeroizing<Vec<u8>>, VertexCredentialError> {
        #[derive(Serialize)]
        struct Record<'a> {
            service_account: &'a Map<String, Value>,
            project_id: &'a str,
            email: &'a str,
            #[serde(skip_serializing_if = "str::is_empty")]
            location: &'a str,
            #[serde(rename = "type")]
            storage_type: &'static str,
            #[serde(skip_serializing_if = "str::is_empty")]
            prefix: &'a str,
        }

        serde_json::to_vec_pretty(&Record {
            service_account: &self.service_account,
            project_id: &self.project_id,
            email: &self.email,
            location: &self.location,
            storage_type: "vertex",
            prefix: &self.prefix,
        })
        .map(Zeroizing::new)
        .map_err(|_| VertexCredentialError::Serialize)
    }
}

impl fmt::Debug for VertexCredentialStorage {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("VertexCredentialStorage")
            .field("service_account", &"[REDACTED]")
            .field("project_id", &self.project_id)
            .field("email", &self.email)
            .field("location", &self.location)
            .field("prefix", &self.prefix)
            .field("store", &"[INJECTED]")
            .field("handle", &self.handle)
            .finish()
    }
}

impl TokenStorage for VertexCredentialStorage {
    fn save_token_to_file(&mut self, _auth_file_path: &Path) -> Result<(), TokenStorageError> {
        let record = self.serialized_record()?;
        self.store
            .store_service_account(&self.handle, &record)
            .map_err(|_| VertexCredentialError::Store)?;
        Ok(())
    }
}

fn string_field(map: &Map<String, Value>, key: &str) -> String {
    map.get(key)
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_owned()
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use super::*;

    #[derive(Default)]
    struct Store(Mutex<Vec<Vec<u8>>>);

    impl VertexCredentialSecretStore for Store {
        fn store_service_account(
            &self,
            _handle: &VertexCredentialHandle,
            serialized_record: &[u8],
        ) -> Result<(), VertexCredentialError> {
            self.0.lock().unwrap().push(serialized_record.to_vec());
            Ok(())
        }
    }
}
