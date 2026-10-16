// ref: sdk/cliproxy/service_codex_executor_binding_test.go @ a88197f845c979132c8978ea223c6af05cc81536
// Port-Status: adapted_to_ctox
// License: MIT (upstream); modifications AGPL-3.0-only

use std::sync::Arc;

use super::service_test_support::{auth, runtime_fixture, TestPluginRuntime};

#[test]
fn codex_binding_is_stable_in_normal_mode_and_replaceable_explicitly() {
    let fixture = runtime_fixture(None);
    let auth = auth("codex-auth", "codex");
    fixture
        .runtime
        .ensure_executors_for_auth(&auth, false)
        .unwrap();
    let first = fixture
        .runtime
        .auth_manager()
        .executors()
        .get("codex")
        .expect("codex registration");

    fixture
        .runtime
        .ensure_executors_for_auth(&auth, false)
        .unwrap();
    let stable = fixture
        .runtime
        .auth_manager()
        .executors()
        .get("codex")
        .unwrap();
    assert!(Arc::ptr_eq(&first, &stable));

    fixture
        .runtime
        .ensure_executors_for_auth(&auth, true)
        .unwrap();
    let replaced = fixture
        .runtime
        .auth_manager()
        .executors()
        .get("codex")
        .unwrap();
    assert!(!Arc::ptr_eq(&first, &replaced));
}

#[test]
fn xai_binding_is_stable_in_normal_mode_and_replaceable_explicitly() {
    let fixture = runtime_fixture(None);
    let auth = auth("xai-auth", "xai");
    fixture
        .runtime
        .ensure_executors_for_auth(&auth, false)
        .unwrap();
    let first = fixture
        .runtime
        .auth_manager()
        .executors()
        .get("xai")
        .unwrap();
    fixture
        .runtime
        .ensure_executors_for_auth(&auth, false)
        .unwrap();
    let stable = fixture
        .runtime
        .auth_manager()
        .executors()
        .get("xai")
        .unwrap();
    assert!(Arc::ptr_eq(&first, &stable));
    fixture
        .runtime
        .ensure_executors_for_auth(&auth, true)
        .unwrap();
    let replaced = fixture
        .runtime
        .auth_manager()
        .executors()
        .get("xai")
        .unwrap();
    assert!(!Arc::ptr_eq(&first, &replaced));
}
