// ref: internal/api/handlers/management/plugins_test.go @ a88197f845c979132c8978ea223c6af05cc81536
// Port-Status: adapted_to_ctox
// License: MIT (upstream); modifications AGPL-3.0-only

use std::collections::BTreeMap;
use std::sync::Arc;

use serde_json::json;

use super::test_store_test::{Runtime, Store};
use super::{
    ManagementPluginConfig, ManagementPluginConfigPatch, ManagementPluginError,
    ManagementPluginRuntimeRecord, ManagementPluginService,
};

fn service() -> (Arc<Store>, Arc<Runtime>, ManagementPluginService) {
    let store = Arc::new(Store::default());
    let runtime = Arc::new(Runtime::default());
    let service = ManagementPluginService::new(store.clone(), runtime.clone());
    (store, runtime, service)
}
