// ref: internal/api/handlers/management/vertex_import.go @ a88197f845c979132c8978ea223c6af05cc81536
// Port-Status: adapted_to_ctox
// License: MIT (upstream); modifications AGPL-3.0-only

use std::fmt;
use std::path::Path;
use std::sync::Arc;

use serde_json::{Map, Value};

use crate::internal::auth::models::TokenStorage;
use crate::internal::auth::vertex::{
    normalize_service_account_map, ServiceAccountNormalizeError, VertexCredentialError,
    VertexCredentialHandle, VertexCredentialSecretStore, VertexCredentialStorage,
};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ManagementVertexImportResult {
    pub auth_id: String,
    pub project_id: String,
    pub email: String,
    pub location: String,
    pub label: String,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ManagementVertexImportError {
    InvalidJson,
    InvalidServiceAccount(ServiceAccountNormalizeError),
    ProjectIdMissing,
    Credential(VertexCredentialError),
    Store,
}

impl fmt::Display for ManagementVertexImportError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::InvalidJson => "vertex import JSON is invalid",
            Self::InvalidServiceAccount(_) => "vertex service account is invalid",
            Self::ProjectIdMissing => "vertex project_id is missing",
            Self::Credential(_) => "vertex credential handle is invalid",
            Self::Store => "vertex credential could not be stored",
        })
    }
}
impl std::error::Error for ManagementVertexImportError {}

/// Validates and persists an uploaded service-account payload exclusively
/// through the injected CTOX secret store. No filesystem path or secret bytes
/// are returned to the management transport.
pub fn import_vertex_credential(
    raw: &[u8],
    location: &str,
    store: Arc<dyn VertexCredentialSecretStore>,
) -> Result<ManagementVertexImportResult, ManagementVertexImportError> {
    let payload: Map<String, Value> =
        serde_json::from_slice(raw).map_err(|_| ManagementVertexImportError::InvalidJson)?;
    let service_account = normalize_service_account_map(&payload)
        .map_err(ManagementVertexImportError::InvalidServiceAccount)?;
    let project_id = string_value(&service_account, "project_id");
    if project_id.is_empty() {
        return Err(ManagementVertexImportError::ProjectIdMissing);
    }
    let email = string_value(&service_account, "client_email");
    let location = match location.trim() {
        "" => "us-central1".to_owned(),
        location => location.to_owned(),
    };
    let auth_id = format!("vertex-{}.json", sanitize_vertex_file_part(&project_id));
    let handle = VertexCredentialHandle::new(format!("vertex/{auth_id}"), "service-account")
        .map_err(ManagementVertexImportError::Credential)?;
    let mut storage =
        VertexCredentialStorage::new(service_account, location.clone(), "", store, handle)
            .map_err(ManagementVertexImportError::Credential)?;
    storage
        .save_token_to_file(Path::new(""))
        .map_err(|_| ManagementVertexImportError::Store)?;
    Ok(ManagementVertexImportResult {
        auth_id,
        project_id: project_id.clone(),
        email: email.clone(),
        location,
        label: label_for_vertex(&project_id, &email),
    })
}

fn string_value(values: &Map<String, Value>, key: &str) -> String {
    values
        .get(key)
        .and_then(Value::as_str)
        .unwrap_or_default()
        .trim()
        .to_owned()
}

#[must_use]
pub fn sanitize_vertex_file_part(value: &str) -> String {
    let sanitized = value
        .trim()
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() || matches!(character, '-' | '.') {
                character
            } else {
                '_'
            }
        })
        .collect::<String>();
    if sanitized.is_empty() {
        "vertex".into()
    } else {
        sanitized
    }
}

#[must_use]
pub fn label_for_vertex(project_id: &str, email: &str) -> String {
    match (project_id.trim(), email.trim()) {
        ("", "") => "vertex".into(),
        (project, "") => project.into(),
        ("", email) => email.into(),
        (project, email) => format!("{project} ({email})"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
}
