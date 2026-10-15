// ref: sdk/cliproxy/service_executor_registration_test.go @ a88197f845c979132c8978ea223c6af05cc81536
// Port-Status: adapted_to_ctox
// License: MIT (upstream); modifications AGPL-3.0-only

use std::sync::Arc;

use super::service_executors::{ExecutorRegistrationOptions, BASELINE_EXECUTOR_PROVIDERS};
use super::service_test_support::{auth, registration, runtime_fixture, TestPluginRuntime};

#[test]
fn register_available_executors_keeps_baseline_then_plugin_binding() {
    let fixture = runtime_fixture(None);
    let plugin = Arc::new(TestPluginRuntime::default());
    plugin.add_registration(registration("plugin-provider"));
    fixture.runtime.set_plugin_runtime(Some(plugin));
    fixture
        .runtime
        .register_available_executors(ExecutorRegistrationOptions {
            include_baseline: true,
            include_plugins: true,
            ..ExecutorRegistrationOptions::default()
        })
        .unwrap();

    for provider in BASELINE_EXECUTOR_PROVIDERS
        .into_iter()
        .chain(["plugin-provider"])
    {
        assert!(
            fixture
                .runtime
                .auth_manager()
                .executors()
                .get(provider)
                .is_some(),
            "provider={provider}"
        );
    }
    assert_eq!(fixture.factory.calls(), BASELINE_EXECUTOR_PROVIDERS);
}

#[test]
fn sdk_executor_is_preserved_unless_force_replace_is_requested() {
    let fixture = runtime_fixture(None);
    let custom = registration("sdk-provider");
    fixture
        .runtime
        .auth_manager()
        .register_executor(custom.clone());
    let auth = auth("private-auth", "sdk-provider");

    fixture
        .runtime
        .ensure_executors_for_auth(&auth, false)
        .unwrap();
    let stable = fixture
        .runtime
        .auth_manager()
        .executors()
        .get("sdk-provider")
        .unwrap();
    assert!(Arc::ptr_eq(&custom, &stable));

    fixture
        .runtime
        .ensure_executors_for_auth(&auth, true)
        .unwrap();
    let replaced = fixture
        .runtime
        .auth_manager()
        .executors()
        .get("sdk-provider")
        .unwrap();
    assert!(!Arc::ptr_eq(&custom, &replaced));
}

// ref: sdk/cliproxy/service_executors.go:204-326 @ d7914afdedca7af95ee974a42453dc49fc1388ce
