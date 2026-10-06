// ref: sdk/cliproxy/auth/api_key_model_capabilities.go:286-328 @ d7914afd
// ref: sdk/cliproxy/auth/home_v8_model_capabilities_test.go @ d7914afd
// Port-Status: adapted_to_ctox
// License: MIT (upstream); modifications AGPL-3.0-only

use super::super::api_key_model_capabilities_test::{auth, register};
use super::super::home_execution_paths_test::{request, runtime, TestExecutor, TestHomeTransport};
use super::*;
use crate::internal::config::{CodexKey, CodexModel, ProviderCompatConfig};

fn selected_auth(raw: Option<serde_json::Value>) -> Auth {
    let mut selected = auth("legacy-home", "legacy-home-key");
    selected.provider = "codex".into();
    selected
        .attributes
        .insert("config_index".into(), "0".into());
    if let Some(raw) = raw {
        selected.metadata.insert("credential_options".into(), raw);
    }
    selected
}

fn options_cases() -> Vec<(Option<serde_json::Value>, Option<bool>)> {
    vec![
        (None, None),
        (Some(serde_json::json!({"weight":2})), None),
        (Some(serde_json::json!({"models":"invalid"})), None),
        (Some(serde_json::json!({"models":null})), Some(false)),
        (Some(serde_json::json!({"models":[]})), Some(false)),
        (
            Some(serde_json::json!({"models":[{"name":"other","is-compat":true}]})),
            Some(false),
        ),
        (
            Some(serde_json::json!({"models":[{"name":"upstream","is-compat":true}]})),
            Some(true),
        ),
        (
            Some(serde_json::json!({"models":[{"name":"upstream","is-compat":false}]})),
            Some(false),
        ),
    ]
}

#[test]
fn candidate_home_legacy_wire_and_option_presence_preserve_upstream_authority() {
    for wire_id in [None, Some(""), Some("  "), Some("upstream")] {
        for (raw, expected_option) in options_cases() {
            let wire = wire_id.map(|id| HomeDispatchModelInfo {
                id: id.into(),
                ..HomeDispatchModelInfo::default()
            });
            let mut original = request("upstream");
            original.resolved_model_info = Some(Arc::new(ModelInfo {
                id: "upstream".into(),
                is_compat: true,
                ..ModelInfo::default()
            }));
            let bound = attach_home_model_info(
                original,
                &selected_auth(raw.clone()),
                "upstream",
                wire.as_ref(),
            );
            assert_eq!(
                bound
                    .resolved_home_model_options
                    .as_ref()
                    .map(|options| options.is_compat),
                expected_option,
                "{wire_id:?}/{raw:?}",
            );
            let central = wire_id.is_some_and(|id| !id.trim().is_empty());
            let info = bound.resolved_model_info.unwrap();
            assert_eq!(info.id, "upstream");
            assert_eq!(
                info.is_compat,
                if central {
                    expected_option.unwrap_or(false)
                } else {
                    true
                }
            );
        }
    }
}

#[tokio::test]
async fn candidate_home_legacy_without_model_info_binds_options_on_all_execution_paths() {
    for path in ["execute", "count", "stream"] {
        for (raw, expected_option) in options_cases() {
            let transport = TestHomeTransport::with_auth_ids(&[]);
            let executor = TestExecutor::failing(0);
            let (runtime, _) = runtime(transport.clone(), executor.clone());
            let manager = runtime.manager();
            manager.set_provider_config(&ProviderCompatConfig {
                codex_api_key: vec![CodexKey {
                    api_key: "legacy-home-key".into(),
                    prefix: "tenant".into(),
                    models: vec![CodexModel {
                        name: "upstream".into(),
                        alias: "public".into(),
                        is_compat: true,
                        ..CodexModel::default()
                    }],
                    ..CodexKey::default()
                }],
                ..ProviderCompatConfig::default()
            });
            let selected = register(&manager, selected_auth(raw.clone()));
            transport.push_dispatch(serde_json::json!({
                "provider":"codex", "model":"upstream(high)",
                "auth_index":selected.index, "auth":selected
            }));
            let mut original = request("tenant/public(high)");
            original.resolved_model_info = Some(Arc::new(ModelInfo {
                id: "forged-caller".into(),
                ..ModelInfo::default()
            }));
            original.resolved_home_model_options = Some(HomeModelOptions {
                is_compat: false,
                ..HomeModelOptions::default()
            });
            if path == "stream" {
                let mut stream = runtime.execute_home_stream(original, "").await.unwrap();
                while let Some(chunk) = stream.chunks.recv().await {
                    assert!(chunk.error.is_none());
                }
            } else {
                runtime
                    .execute_home(original, "", path == "count")
                    .await
                    .unwrap();
            }
            let seen = executor.seen();
            assert_eq!(seen.len(), 1);
            assert_eq!(seen[0].model, "upstream(high)");
            assert_eq!(seen[0].resolved_model_info.as_ref().unwrap().id, "upstream");
            assert!(seen[0].resolved_model_info.as_ref().unwrap().is_compat);
            assert_eq!(
                seen[0]
                    .resolved_home_model_options
                    .as_ref()
                    .map(|options| options.is_compat),
                expected_option,
                "{path}/{raw:?}",
            );
            let effective = seen[0]
                .resolved_home_model_options
                .as_ref()
                .map(|options| options.is_compat)
                .unwrap_or_else(|| seen[0].resolved_model_info.as_ref().unwrap().is_compat);
            assert_eq!(effective, expected_option.unwrap_or(true));
        }
    }
}

#[test]
fn candidate_home_route_prefix_preserves_dispatch_prefix_exactly() {
    let mut selected = selected_auth(Some(serde_json::json!({"models":[
        {"name":"upstream","alias":"other","is-compat":false},
        {"name":"upstream","alias":"chosen","is-compat":true}
    ]})));
    selected.prefix = " /tenant/ ".into();
    assert!(
        home_model_options(&selected, "upstream(high)", "/tenant//chosen(high)")
            .unwrap()
            .is_compat
    );
    // Prefix slash normalization would incorrectly resolve this different route.
    assert!(
        !home_model_options(&selected, "upstream(high)", "tenant/chosen(high)")
            .unwrap()
            .is_compat
    );
}
