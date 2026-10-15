// ref: internal/auth/empty/token.go @ a88197f845c979132c8978ea223c6af05cc81536
// Port-Status: adapted_to_ctox
// License: MIT (upstream); modifications AGPL-3.0-only

use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::internal::auth::models::{TokenStorage, TokenStorageError};

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub struct EmptyStorage {
    #[serde(rename = "type")]
    pub storage_type: String,
}

impl TokenStorage for EmptyStorage {
    fn save_token_to_file(&mut self, _auth_file_path: &Path) -> Result<(), TokenStorageError> {
        self.storage_type = "empty".to_owned();
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::{EmptyStorage, TokenStorage};
}
