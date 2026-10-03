// ref: sdk/cliproxy/auth/api_key_model_compat_test.go @ 2044a01f
// License: MIT (upstream); modifications AGPL-3.0-only

use std::sync::Arc;

use crate::internal::config::{CodexKey, CodexModel, OpenAiCompatibility, ProviderCompatConfig};
use crate::internal::modelconfig::ModelInfo;
use crate::sdk::cliproxy::auth::api_key_model_capabilities_test::{auth, manager, register};
use crate::sdk::cliproxy::auth::conductor_execution::selected_executor_request;
use crate::sdk::cliproxy::executor::Request;
use crate::sdk::pluginapi::ExecutorRequest;

fn model(is_compat: bool) -> CodexModel {
    CodexModel {
        name: "shared".into(),
        alias: "public".into(),
        is_compat,
        ..CodexModel::default()
    }
}

#[test]
fn candidate_typed_compatibility_defaults_false_and_uses_the_upstream_wire_key() {
    let ordinary: CodexModel =
        serde_json::from_str(r#"{"name":"shared","alias":"public"}"#).unwrap();
    assert!(!ordinary.is_compat);
    assert!(serde_json::to_value(&ordinary)
        .unwrap()
        .get("is-compat")
        .is_none());
    let compatible: CodexModel =
        serde_json::from_str(r#"{"name":"shared","alias":"public","is-compat":true}"#).unwrap();
    assert!(compatible.is_compat);
    assert_eq!(
        serde_json::to_value(&compatible).unwrap()["is-compat"],
        true
    );
    assert!(serde_json::from_str::<CodexModel>(
        r#"{"name":"shared","alias":"public","is-compat":"true"}"#
    )
    .is_err());
}

#[test]
fn candidate_configured_compatibility_propagates_for_all_six_api_key_families() {
    for provider in [
        "gemini",
        "gemini-interactions",
        "claude",
        "codex",
        "xai",
        "openai-compat:test",
    ] {
        let manager = manager();
        let key = CodexKey {
            api_key: "test-only-secret".into(),
            models: vec![model(true)],
            ..CodexKey::default()
        };
        let mut config = ProviderCompatConfig::default();
        match provider {
            "gemini" => config.gemini_api_key.push(key),
            "gemini-interactions" => config.interactions_api_key.push(key),
            "claude" => config.claude_api_key.push(key),
            "codex" => config.codex_api_key.push(key),
            "xai" => config.xai_api_key.push(key),
            _ => config.openai_compatibility.push(OpenAiCompatibility {
                name: "test".into(),
                models: key.models,
                ..OpenAiCompatibility::default()
            }),
        }
        manager.set_provider_config(&config);
        let mut configured = auth(provider, "test-only-secret");
        configured.provider = provider.into();
        configured
            .attributes
            .insert("config_index".into(), "0".into());
        configured
            .attributes
            .insert("compat_name".into(), "test".into());
        let configured = register(&manager, configured);
        let selected = manager.attach_resolved_api_key_model_info(
            Request::default(),
            &configured,
            "tenant/public",
            "shared",
        );
        let info = selected
            .metadata
            .resolved_api_key_model_info
            .expect(provider);
        assert!(info.is_compat, "{provider}");
        assert_eq!(info.id, "shared");
    }
}

#[test]
fn candidate_selected_attempt_binds_exact_credential_alias_suffix_and_clears_stale_authority() {
    let manager = manager();
    manager.set_provider_config(&ProviderCompatConfig {
        claude_api_key: vec![
            CodexKey {
                api_key: "same-test-secret".into(),
                models: vec![model(true)],
                ..CodexKey::default()
            },
            CodexKey {
                api_key: "same-test-secret".into(),
                models: vec![model(false)],
                ..CodexKey::default()
            },
        ],
        ..ProviderCompatConfig::default()
    });
    let mut compatible = auth("compatible", "same-test-secret");
    compatible
        .attributes
        .insert("config_index".into(), "0".into());
    let compatible = register(&manager, compatible);
    let mut ordinary = auth("ordinary", "same-test-secret");
    ordinary
        .attributes
        .insert("config_index".into(), "1".into());
    let ordinary = register(&manager, ordinary);

    let request = ExecutorRequest {
        model: "tenant/public(high)".into(),
        resolved_model_info: Some(Arc::new(ModelInfo {
            is_compat: true,
            ..ModelInfo::default()
        })),
        ..ExecutorRequest::default()
    };
    for (credential, expected) in [(&compatible, true), (&ordinary, false)] {
        let selected = selected_executor_request(&manager, &request, credential, "claude");
        assert_eq!(selected.model, "shared(high)");
        assert_eq!(selected.resolved_model_info.unwrap().is_compat, expected);
        assert_eq!(selected.auth_id, credential.id);
    }

    let unavailable = ExecutorRequest {
        model: "unconfigured".into(),
        ..request
    };
    let selected = selected_executor_request(&manager, &unavailable, &ordinary, "claude");
    assert!(selected.resolved_model_info.is_none());
}

#[test]
fn candidate_private_capability_is_absent_from_the_plugin_wire_in_both_directions() {
    let request: ExecutorRequest = serde_json::from_value(serde_json::json!({
        "Model": "shared",
        "ResolvedModelInfo": {"IsCompat": true},
        "Metadata": {"resolved_model_info": {"is_compat": true}},
        "AuthAttributes": {"is_compat": "true"}
    }))
    .unwrap();
    assert!(request.resolved_model_info.is_none());

    let native = ExecutorRequest {
        resolved_model_info: Some(Arc::new(ModelInfo {
            is_compat: true,
            ..ModelInfo::default()
        })),
        ..request
    };
    let wire = serde_json::to_value(&native).unwrap();
    assert!(wire.get("ResolvedModelInfo").is_none());
    assert!(wire.get("resolved_model_info").is_none());
    assert!(wire["Metadata"]["resolved_model_info"]["is_compat"]
        .as_bool()
        .unwrap());
}
