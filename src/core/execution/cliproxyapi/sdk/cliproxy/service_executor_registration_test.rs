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

#[test]
fn openai_compatibility_uses_namespaced_provider_key_without_colliding_with_native() {
    for native_first in [true, false] {
        let fixture = runtime_fixture(None);
        let native = auth("native-kimi", "kimi");
        let mut compatibility = auth("compat-kimi", "openai-compatibility");
        compatibility.label = "kimi".into();
        compatibility
            .attributes
            .insert("compat_name".into(), "kimi".into());
        compatibility
            .attributes
            .insert("provider_key".into(), "kimi".into());
        let auths = if native_first {
            vec![native, compatibility]
        } else {
            vec![compatibility, native]
        };
        fixture
            .runtime
            .register_executors_for_auths(&auths, true)
            .unwrap();
        assert!(fixture
            .runtime
            .auth_manager()
            .executors()
            .get("kimi")
            .is_some());
        assert!(fixture
            .runtime
            .auth_manager()
            .executors()
            .get("openai-compatible-kimi")
            .is_some());
    }
}

// ref: sdk/cliproxy/service_executors.go:204-326 @ d7914afdedca7af95ee974a42453dc49fc1388ce
#[test]
fn candidate_v8_service_baseline_has_complete_upstream_provider_inventory() {
    let expected = [
        "codex",
        "claude",
        "gemini",
        "gemini-interactions",
        "vertex",
        "aistudio",
        "antigravity",
        "kimi",
        "kimi-ai",
        "kimi.ai",
        "xai",
        "devin",
        "meta",
        "openai-compatibility",
    ];
    let baseline = super::service_executors::baseline_executor_auths();
    assert_eq!(baseline.len(), expected.len());
    for (auth, provider) in baseline.iter().zip(expected) {
        assert_eq!(auth.provider, provider);
        assert_eq!(auth.id, provider);
        assert!(!auth.disabled);
        if provider == "openai-compatibility" {
            assert_eq!(auth.attributes["compat_name"], provider);
        } else {
            assert!(auth.attributes.is_empty());
        }
    }
    let fixture = runtime_fixture(None);
    fixture
        .runtime
        .register_available_executors(ExecutorRegistrationOptions {
            include_baseline: true,
            ..Default::default()
        })
        .unwrap();
    assert_eq!(fixture.factory.calls(), expected);
    for provider in expected {
        assert!(fixture
            .runtime
            .auth_manager()
            .executors()
            .get(provider)
            .is_some());
    }
}
#[test]
fn candidate_v8_service_native_providers_rebind_instead_of_generic_sdk_preservation() {
    for provider in ["kimi-ai", "kimi.ai", "kimi.com", "devin", "meta"] {
        let fixture = runtime_fixture(None);
        let previous = registration(provider);
        fixture
            .runtime
            .auth_manager()
            .register_executor(previous.clone());
        let account = auth("enabled-native", provider);
        fixture
            .runtime
            .ensure_executors_for_auth(&account, false)
            .unwrap();
        let rebound = fixture
            .runtime
            .auth_manager()
            .executors()
            .get(provider)
            .unwrap();
        assert!(
            !Arc::ptr_eq(&previous, &rebound),
            "native provider={provider}"
        );
        assert_eq!(fixture.factory.calls(), [provider]);
    }
}
#[test]
fn candidate_v8_service_disabled_native_auth_cannot_replace_enabled_registration() {
    for provider in ["kimi-ai", "kimi.ai", "kimi.com", "devin", "meta"] {
        let fixture = runtime_fixture(None);
        let enabled = registration(provider);
        fixture
            .runtime
            .auth_manager()
            .register_executor(enabled.clone());
        let mut disabled = auth("removed-native", provider);
        disabled.disabled = true;
        fixture
            .runtime
            .ensure_executors_for_auth(&disabled, true)
            .unwrap();
        let retained = fixture
            .runtime
            .auth_manager()
            .executors()
            .get(provider)
            .unwrap();
        assert!(Arc::ptr_eq(&enabled, &retained));
        assert!(fixture.factory.calls().is_empty());
    }
}
#[test]
fn candidate_v8_service_compatibility_named_devin_does_not_take_native_binding() {
    for native_first in [true, false] {
        let fixture = runtime_fixture(None);
        let native = auth("native-devin", "devin");
        let mut compatible = auth("compatible-devin", "openai-compatibility");
        compatible.label = "devin".into();
        compatible
            .attributes
            .insert("compat_name".into(), "devin".into());
        compatible
            .attributes
            .insert("provider_key".into(), "devin".into());
        let auths = if native_first {
            vec![native, compatible]
        } else {
            vec![compatible, native]
        };
        fixture
            .runtime
            .register_executors_for_auths(&auths, true)
            .unwrap();
        assert!(fixture
            .runtime
            .auth_manager()
            .executors()
            .get("devin")
            .is_some());
        assert!(fixture
            .runtime
            .auth_manager()
            .executors()
            .get("openai-compatible-devin")
            .is_some());
        let calls = fixture.factory.calls();
        assert_eq!(
            calls,
            if native_first {
                ["devin", "openai-compatible-devin"]
            } else {
                ["openai-compatible-devin", "devin"]
            }
        );
    }
}
