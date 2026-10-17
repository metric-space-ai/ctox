// ref: internal/api/handlers/management/plugin_store_test.go @ a88197f845c979132c8978ea223c6af05cc81536
// Port-Status: adapted_to_ctox
// License: MIT (upstream); modifications AGPL-3.0-only

use std::sync::{Arc, Mutex};

use super::test_store_test::{Runtime, Store};
use super::{
    ManagementPluginInstallRequest, ManagementPluginInstallResult, ManagementPluginService,
    ManagementPluginStagedInstall, ManagementPluginStoreAuthority,
    ManagementPluginStoreAuthorityError, ManagementPluginStoreCatalog, ManagementPluginStoreEntry,
    ManagementPluginStoreError, ManagementPluginStoreService, ManagementPluginStoreSource,
};

#[derive(Debug)]
struct Authority {
    catalog: ManagementPluginStoreCatalog,
    staged: Mutex<Option<ManagementPluginStagedInstall>>,
    rolled_back: Mutex<Vec<String>>,
    committed: Mutex<Vec<String>>,
}

impl ManagementPluginStoreAuthority for Authority {
    fn catalog(&self) -> Result<ManagementPluginStoreCatalog, ManagementPluginStoreAuthorityError> {
        Ok(self.catalog.clone())
    }

    fn stage_install(
        &self,
        _: &ManagementPluginInstallRequest,
    ) -> Result<ManagementPluginStagedInstall, ManagementPluginStoreAuthorityError> {
        self.staged
            .lock()
            .unwrap()
            .take()
            .ok_or(ManagementPluginStoreAuthorityError)
    }

    fn commit_install(
        &self,
        staged: ManagementPluginStagedInstall,
    ) -> ManagementPluginInstallResult {
        self.committed.lock().unwrap().push(staged.operation_id);
        ManagementPluginInstallResult {
            source_id: staged.source_id,
            id: staged.id,
            version: staged.version,
            status: "installed".to_owned(),
            restart_required: false,
        }
    }

    fn rollback_install(&self, staged: ManagementPluginStagedInstall) {
        self.rolled_back.lock().unwrap().push(staged.operation_id);
    }
}

fn setup(
    staged: ManagementPluginStagedInstall,
) -> (Arc<Store>, Arc<Authority>, ManagementPluginStoreService) {
    let store = Arc::new(Store::default());
    let plugins = Arc::new(ManagementPluginService::new(
        store.clone(),
        Arc::new(Runtime::default()),
    ));
    let authority = Arc::new(Authority {
        catalog: ManagementPluginStoreCatalog::default(),
        staged: Mutex::new(Some(staged)),
        rolled_back: Mutex::new(Vec::new()),
        committed: Mutex::new(Vec::new()),
    });
    let service = ManagementPluginStoreService::new(plugins, authority.clone());
    (store, authority, service)
}

fn staged() -> ManagementPluginStagedInstall {
    ManagementPluginStagedInstall {
        operation_id: "opaque-operation".to_owned(),
        source_id: "official".to_owned(),
        id: "sample".to_owned(),
        version: "1.2.3".to_owned(),
        install_ref: "opaque-install-ref".to_owned(),
    }
}

fn request() -> ManagementPluginInstallRequest {
    ManagementPluginInstallRequest {
        source_id: "official".to_owned(),
        id: "sample".to_owned(),
        version: Some("v1.2.3".to_owned()),
        platform: "linux/amd64".to_owned(),
    }
}
