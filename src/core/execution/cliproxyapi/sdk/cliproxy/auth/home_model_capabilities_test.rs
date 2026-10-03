// ref: sdk/cliproxy/auth/home_v8_model_capabilities_test.go @ d7914afd
// ref: sdk/cliproxy/auth/home_configuration_update_test.go @ d7914afd
// Port-Status: adapted_to_ctox
// License: MIT (upstream); modifications AGPL-3.0-only

#[tokio::test]
async fn candidate_home_legacy_update_uses_exact_selected_codex_configuration() {
    use super::super::api_key_model_capabilities_test::{auth, register};
    use crate::internal::config::{CodexKey, CodexModel, ProviderCompatConfig};

    for enabled in [false, true] {
        let transport = TestHomeTransport::with_auth_ids(&[]);
        let executor = TestExecutor::failing(0);
        let (runtime, _) = runtime(transport.clone(), executor.clone());
        let manager = runtime.manager();
        manager.set_provider_config(&ProviderCompatConfig {
            codex_api_key: vec![CodexKey {
                api_key: "legacy-home-test-key".into(),
                models: vec![CodexModel {
                    name: "shared".into(),
                    alias: "public".into(),
                    support_configuration_update: enabled,
                    ..CodexModel::default()
                }],
                ..CodexKey::default()
            }],
            ..ProviderCompatConfig::default()
        });
        let mut account = auth("codex", "legacy-home-test-key");
        account.provider = "codex".into();
        account.attributes.insert("config_index".into(), "0".into());
        let account = register(&manager, account);
        transport.push_dispatch(serde_json::json!({
            "provider":"codex","model":"shared(high)","auth_index":account.index,
            "model_info":{"id":"shared","context_length":32768},
            "auth":account
        }));
        runtime
            .execute_home(request("tenant/public(high)"), "", false)
            .await
            .unwrap();
        let seen = executor.seen();
        assert_eq!(seen.len(), 1);
        assert_eq!(
            seen[0]
                .resolved_model_info
                .as_ref()
                .unwrap()
                .support_configuration_update,
            enabled
        );
        assert!(!seen[0].resolved_model_info.as_ref().unwrap().is_compat);
    }
}

use super::super::home_execution_paths_test::{request, runtime, TestExecutor, TestHomeTransport};
use super::*;

fn auth_with_options(raw: Option<&str>) -> Auth {
    let mut auth = Auth {
        prefix: "tenant".to_owned(),
        ..Auth::default()
    };
    if let Some(raw) = raw {
        auth.metadata.insert(
            "credential_options".to_owned(),
            serde_json::from_str(raw).unwrap(),
        );
    }
    auth
}

#[test]
fn candidate_home_v8_and_legacy_wire_preserve_central_capabilities() {
    for raw in [
        r#"{"id":" upstream ","type":" codex ","inputTokenLimit":30000,"outputTokenLimit":4000,"context_length":32768,"max_completion_tokens":4096,"thinking":{"levels":["high","none"],"zero_allowed":true},"native_capabilities":{"web_search":false},"support_configuration_update":true,"user_defined":false,"max_context_length":99999,"is_compat":false}"#,
        r#"{"id":" upstream ","type":" codex ","inputTokenLimit":30000,"outputTokenLimit":4000,"context_length":32768,"max_completion_tokens":4096,"thinking":{"levels":["high","none"],"zero_allowed":true},"native_capabilities":{"web_search":false},"support_configuration_update":true,"user_defined":true}"#,
    ] {
        let wire: HomeDispatchModelInfo = serde_json::from_str(raw).unwrap();
        let auth = auth_with_options(Some(
            r#"{"models":[{"name":"upstream","alias":"alias","is-compat":true}]}"#,
        ));
        let bound = attach_home_model_info(request("alias"), &auth, "alias", Some(&wire));
        let info = bound.resolved_model_info.unwrap();
        assert_eq!(info.id, "upstream");
        assert_eq!(info.provider_type, "codex");
        assert_eq!(info.context_length, 32768);
        assert_eq!(info.input_token_limit, 30000);
        assert_eq!(info.output_token_limit, 4000);
        assert_eq!(info.max_completion_tokens, 4096);
        assert_eq!(info.user_defined, wire.user_defined);
        assert!(info.is_compat);
        assert!(info.support_configuration_update);
        assert!(info.thinking.as_ref().unwrap().zero_allowed);
        assert_eq!(
            info.native_capabilities.as_ref().unwrap().web_search,
            Some(false)
        );
        assert!(!wire.model_info().unwrap().is_compat);
    }
    let legacy: HomeDispatchModelInfo =
        serde_json::from_str(r#"{"id":"old","context_length":16384,"user_defined":true}"#).unwrap();
    let info = attach_home_model_info(request("old"), &Auth::default(), "old", Some(&legacy))
        .resolved_model_info
        .unwrap();
    assert_eq!(info.context_length, 16384);
    assert!(info.user_defined);
    assert!(!info.is_compat);
    assert!(!info.support_configuration_update);
    assert!(info.native_capabilities.is_none());
}

#[test]
fn candidate_home_options_follow_selected_upstream_suffix_and_alias() {
    let cases = [
        (
            r#"{"models":[{"name":"model","is-compat":true}]}"#,
            "model",
            "",
            true,
        ),
        (
            r#"{"models":[{"name":"model","is-compat":false}]}"#,
            "model",
            "",
            false,
        ),
        (r#"{"models":[{"name":"model"}]}"#, "model", "", false),
        (r#"{"models":null}"#, "model", "", false),
        (r#"{"models":[]}"#, "model", "", false),
        (
            r#"{"models":[{"name":"other","is-compat":true}]}"#,
            "model",
            "",
            false,
        ),
        (
            r#"{"models":[{"name":"other","alias":"model","is-compat":true},{"name":"model","is-compat":false}]}"#,
            "model",
            "",
            false,
        ),
        (
            r#"{"models":[{"name":"upstream","alias":"model","is-compat":true}]}"#,
            "model",
            "",
            true,
        ),
        (
            r#"{"models":[{"name":"model","is-compat":true}]}"#,
            "model(high)",
            "",
            true,
        ),
        (
            r#"{"models":[{"name":"model","is-compat":true},{"name":"model(high)","is-compat":false}]}"#,
            "model(high)",
            "",
            false,
        ),
        (
            r#"{"models":[{"name":"model","alias":"other","is-compat":false},{"name":"model","alias":"chosen","is-compat":true}]}"#,
            "model(high)",
            "tenant/chosen(high)",
            true,
        ),
        (
            r#"{"models":[{"name":"other","alias":"chosen","is-compat":false},{"name":"model","alias":"chosen","is-compat":true}]}"#,
            "model",
            "tenant/chosen",
            true,
        ),
        (
            r#"{"models":[{"alias":"model","is-compat":true}]}"#,
            "model",
            "",
            true,
        ),
    ];
    for (raw, upstream, route, expected) in cases {
        let auth = auth_with_options(Some(raw));
        let selected = home_model_options(&auth, upstream, route).unwrap();
        assert_eq!(selected.is_compat, expected, "{raw}, {upstream}, {route}");
    }
}

#[test]
fn candidate_home_missing_invalid_options_never_inherit_local_compatibility() {
    let wire: HomeDispatchModelInfo = serde_json::from_str(r#"{"id":"model"}"#).unwrap();
    for raw in [
        None,
        Some(r#"{"weight":2}"#),
        Some(r#"{"models":"invalid"}"#),
        Some(r#"{"models":null}"#),
        Some(r#"{"models":[]}"#),
        Some(r#"{"models":[{"name":"other","is-compat":true}]}"#),
    ] {
        let local = Arc::new(ModelInfo {
            id: "model".to_owned(),
            is_compat: true,
            ..ModelInfo::default()
        });
        let mut original = request("alias");
        original.resolved_model_info = Some(local.clone());
        let bound = attach_home_model_info(original, &auth_with_options(raw), "alias", Some(&wire));
        assert!(
            !bound.resolved_model_info.as_ref().unwrap().is_compat,
            "{raw:?}"
        );
        assert!(
            !bound
                .resolved_home_model_options
                .as_ref()
                .unwrap()
                .is_compat
        );
        assert!(local.is_compat);
    }
}

#[test]
fn candidate_home_configuration_update_is_tri_state_and_same_model_only() {
    for (explicit, local_id, local_support, expected) in [
        (Some(true), "model", false, true),
        (Some(false), "model", true, false),
        (None, "model(high)", true, true),
        (None, "other", true, false),
        (None, "model", false, false),
    ] {
        let wire = HomeDispatchModelInfo {
            id: "model".to_owned(),
            support_configuration_update: explicit,
            ..HomeDispatchModelInfo::default()
        };
        let local = Arc::new(ModelInfo {
            id: local_id.to_owned(),
            support_configuration_update: local_support,
            ..ModelInfo::default()
        });
        let mut original = request("model");
        original.resolved_model_info = Some(local.clone());
        let bound = attach_home_model_info(original, &Auth::default(), "model", Some(&wire));
        assert_eq!(
            bound
                .resolved_model_info
                .unwrap()
                .support_configuration_update,
            expected
        );
        assert_eq!(local.support_configuration_update, local_support);
        assert_eq!(wire.support_configuration_update, explicit);
    }
}

#[test]
fn candidate_home_private_options_and_capability_cannot_enter_plugin_json() {
    let mut original = request("model");
    original.resolved_model_info = Some(Arc::new(ModelInfo {
        id: "model".to_owned(),
        is_compat: true,
        ..ModelInfo::default()
    }));
    original.resolved_home_model_options = Some(HomeModelOptions {
        name: "model".to_owned(),
        is_compat: true,
        ..HomeModelOptions::default()
    });
    let mut wire = serde_json::to_value(&original).unwrap();
    assert!(wire.get("ResolvedModelInfo").is_none());
    assert!(wire.get("ResolvedHomeModelOptions").is_none());
    wire["ResolvedModelInfo"] = serde_json::json!({"id":"forged","is_compat":true});
    wire["ResolvedHomeModelOptions"] = serde_json::json!({"name":"forged","is-compat":true});
    let decoded: ExecutorRequest = serde_json::from_value(wire).unwrap();
    assert!(decoded.resolved_model_info.is_none());
    assert!(decoded.resolved_home_model_options.is_none());
    let cleared = super::super::conductor_home_execution::prepare_executor_request(
        &original,
        &Auth::default(),
        "codex",
    );
    assert!(cleared.resolved_model_info.is_none());
    assert!(cleared.resolved_home_model_options.is_none());
}

fn dispatch(enabled: bool) -> serde_json::Value {
    serde_json::json!({
        "provider":"codex","model":"upstream(high)","auth_index":"home-model-account",
        "force_mapping":true,"original_alias":"tenant/alias(high)",
        "model_info":{"id":"upstream","context_length":32768,"thinking":{"levels":["high"]},
                      "native_capabilities":{"web_search":true},"support_configuration_update":false},
        "auth":{"id":"home-model-account","index":"home-model-account","provider":"codex","prefix":"tenant",
                "metadata":{"credential_options":{"models":[
                    {"name":"upstream","alias":"other","is-compat":!enabled},
                    {"name":"upstream","alias":"alias","is-compat":enabled}]}}}
    })
}

#[tokio::test]
async fn candidate_home_execute_count_stream_bind_dispatch_and_selected_options() {
    for path in ["execute", "count", "stream"] {
        for enabled in [false, true] {
            let transport = TestHomeTransport::with_auth_ids(&[]);
            transport.push_dispatch(dispatch(enabled));
            let executor = TestExecutor::failing(0);
            let (runtime, _) = runtime(transport, executor.clone());
            let mut original = request("tenant/alias(high)");
            original.resolved_model_info = Some(Arc::new(ModelInfo {
                id: "unrelated-local".to_owned(),
                is_compat: !enabled,
                ..ModelInfo::default()
            }));
            original.resolved_home_model_options = Some(HomeModelOptions {
                is_compat: !enabled,
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
            assert_eq!(seen[0].auth_id, "home-model-account");
            assert_eq!(seen[0].auth_attributes["home_force_mapping"], "true");
            let info = seen[0].resolved_model_info.as_ref().unwrap();
            assert_eq!(info.id, "upstream");
            assert_eq!(info.context_length, 32768);
            assert_eq!(info.is_compat, enabled, "{path}");
            assert!(!info.support_configuration_update);
            assert_eq!(
                info.native_capabilities.as_ref().unwrap().web_search,
                Some(true)
            );
            assert_eq!(
                seen[0].resolved_home_model_options.as_ref().unwrap().alias,
                "alias"
            );
        }
    }
}

#[tokio::test]
async fn candidate_home_retained_session_keeps_its_dispatch_capability() {
    let transport = TestHomeTransport::with_auth_ids(&[]);
    transport.push_dispatch(dispatch(true));
    let executor = TestExecutor::failing(0);
    let (runtime, _) = runtime(transport.clone(), executor.clone());
    for _ in 0..2 {
        runtime
            .execute_home(request("tenant/alias(high)"), "same-session", false)
            .await
            .unwrap();
    }
    assert_eq!(transport.requests().len(), 1);
    let seen = executor.seen();
    assert_eq!(seen.len(), 2);
    for execution in seen {
        assert_eq!(execution.auth_id, "home-model-account");
        assert_eq!(execution.model, "upstream(high)");
        assert!(execution.resolved_model_info.unwrap().is_compat);
        assert_eq!(
            execution.resolved_home_model_options.unwrap().alias,
            "alias"
        );
    }
    assert_eq!(runtime.close_execution_session("same-session"), 1);
}
