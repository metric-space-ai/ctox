// ref: sdk/cliproxy/auth/home_selection_test.go @ a88197f845c979132c8978ea223c6af05cc81536
// Port-Status: adapted_to_ctox
// Port-Note: instance-owned scope, resource teardown and retained route adaptation
// License: MIT (upstream); modifications AGPL-3.0-only

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

use crate::sdk::cliproxy::executionregistry::{Registry, ScopeSpec};
use crate::sdk::pluginapi::ProviderExecutor;

use super::{Auth, HomeDispatchSelection};

pub(super) fn selection() -> (Arc<HomeDispatchSelection>, Arc<Registry>) {
    let registry = Arc::new(Registry::new());
    let pending = registry.begin_dispatch().unwrap();
    let scope = registry
        .install(
            &pending,
            ScopeSpec {
                request_id: "request".into(),
                credential_id: "auth".into(),
                model: "gpt-5".into(),
                ..ScopeSpec::default()
            },
        )
        .unwrap();
    let executor: Arc<dyn ProviderExecutor> =
        super::home_execution_paths_test::TestExecutor::failing(0);
    let mut auth = Auth::default();
    auth.id = "auth".into();
    auth.index = "auth".into();
    auth.provider = "codex".into();
    auth.attributes
        .insert("home_upstream_model".into(), "gpt-5(high)".into());
    auth.attributes
        .insert("home_force_mapping".into(), "true".into());
    (
        HomeDispatchSelection::new(auth, executor, " CoDeX ", scope).unwrap(),
        registry,
    )
}
