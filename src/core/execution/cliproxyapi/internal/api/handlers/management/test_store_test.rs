// ref: internal/api/handlers/management/test_store_test.go @ a88197f845c979132c8978ea223c6af05cc81536
// Port-Status: adapted_to_ctox
// License: MIT (upstream); modifications AGPL-3.0-only

use std::sync::Mutex;

use super::{
    ManagementPluginConfigStore, ManagementPluginConfigStoreError, ManagementPluginError,
    ManagementPluginRuntimeRecord, ManagementPluginRuntimeSource, ManagementPluginSnapshot,
};

#[derive(Debug, Default)]
pub(super) struct Store(pub(super) Mutex<ManagementPluginSnapshot>);

impl ManagementPluginConfigStore for Store {
    fn load(&self) -> Result<ManagementPluginSnapshot, ManagementPluginConfigStoreError> {
        Ok(self.0.lock().unwrap().clone())
    }

    fn replace(
        &self,
        expected_revision: u64,
        snapshot: &ManagementPluginSnapshot,
    ) -> Result<(), ManagementPluginConfigStoreError> {
        let mut current = self.0.lock().unwrap();
        if current.revision != expected_revision {
            return Err(ManagementPluginConfigStoreError::Conflict);
        }
        *current = snapshot.clone();
        Ok(())
    }
}

#[derive(Debug, Default)]
pub(super) struct Runtime(pub(super) Mutex<Vec<ManagementPluginRuntimeRecord>>);

impl ManagementPluginRuntimeSource for Runtime {
    fn snapshot(&self) -> Result<Vec<ManagementPluginRuntimeRecord>, ManagementPluginError> {
        Ok(self.0.lock().unwrap().clone())
    }
}
