// ref: sdk/cliproxy/builder_weight_validation_test.go @ a88197f845c979132c8978ea223c6af05cc81536
// Port-Status: adapted_to_ctox
// License: MIT (upstream); modifications AGPL-3.0-only

use std::path::Path;
use std::sync::{Arc, Mutex};

use crate::internal::config::{
    ClaudeSubscriptionAccountConfig, CliproxyRuntimeConfig, RuntimeSecretRef,
};
use crate::internal::credentialweight::MAX_CREDENTIAL_WEIGHT;
use crate::sdk::api::options::apply_server_options;

use super::builder::*;

fn config(weight: i64) -> CliproxyRuntimeConfig {
    serde_json::from_value(serde_json::json!({
        "claude_accounts": [{
            "id": "claude-a",
            "weight": weight,
            "access_token_secret": {"scope": "cliproxy", "name": "claude-access"},
            "refresh_token_secret": {"scope": "cliproxy", "name": "claude-refresh"}
        }]
    }))
    .unwrap()
}

#[test]
fn builder_build_rejects_invalid_with_config_credential_weight() {
    let error = new_builder()
        .with_config(config(MAX_CREDENTIAL_WEIGHT + 1))
        .with_config_path("/runtime/config.yaml")
        .build()
        .unwrap_err();

    assert_eq!(error.kind, BuilderErrorKind::InvalidConfiguration);
    assert!(error
        .to_string()
        .contains("cliproxy: validate credential weights: claude-accounts[0].weight"));
}

#[test]
fn required_inputs_fail_in_upstream_order_before_dependency_resolution() {
    let missing_config = new_builder().build().unwrap_err();
    assert_eq!(missing_config.kind, BuilderErrorKind::ConfigurationRequired);
    let missing_path = new_builder().with_config(config(1)).build().unwrap_err();
    assert_eq!(missing_path.kind, BuilderErrorKind::ConfigPathRequired);
}

#[test]
fn valid_minimal_build_exposes_host_owned_requirements_instead_of_claiming_authority() {
    let mut assembly = new_builder()
        .with_config(config(1))
        .with_config_path("/runtime/config.yaml")
        .with_local_management_password("")
        .with_local_management_password("local-secret")
        .build()
        .unwrap();

    assert_eq!(assembly.config_path(), Path::new("/runtime/config.yaml"));
    assert_eq!(
        assembly.requirements(),
        [
            ServiceBindingRequirement::ApiKeyClientProvider,
            ServiceBindingRequirement::WatcherFactory,
            ServiceBindingRequirement::CoreAuthManager,
            ServiceBindingRequirement::PluginHost,
            ServiceBindingRequirement::PersistedAuthUpdateSink,
        ]
    );
    let options = apply_server_options(assembly.take_server_options());
    assert_eq!(options.local_management_password(), Some("local-secret"));
    assert!(assembly.take_server_options().is_empty());
    let debug = format!("{assembly:?}");
    assert!(!debug.contains("local-secret"));
}

struct RecordingPluginHost {
    calls: Mutex<Vec<&'static str>>,
}

impl PluginHost for RecordingPluginHost {
    fn apply_config(
        &self,
        _config: &crate::internal::config::ValidatedRuntimeConfig,
    ) -> Result<(), PluginHostError> {
        self.calls.lock().unwrap().push("apply");
        Ok(())
    }

    fn register_frontend_auth_providers(&self) -> Result<(), PluginHostError> {
        self.calls.lock().unwrap().push("register");
        Ok(())
    }

    fn access_providers(&self) -> Vec<crate::sdk::access::SharedProvider> {
        self.calls.lock().unwrap().push("providers");
        Vec::new()
    }
}

struct FailingPluginHost;

impl PluginHost for FailingPluginHost {
    fn apply_config(
        &self,
        _config: &crate::internal::config::ValidatedRuntimeConfig,
    ) -> Result<(), PluginHostError> {
        Err(PluginHostError::Configuration)
    }

    fn register_frontend_auth_providers(&self) -> Result<(), PluginHostError> {
        panic!("registration must not run after config failure")
    }

    fn access_providers(&self) -> Vec<crate::sdk::access::SharedProvider> {
        panic!("provider snapshot must not run after config failure")
    }
}

#[test]
fn lifecycle_hooks_remain_dormant_until_service_host_invokes_them() {
    let calls = Arc::new(Mutex::new(Vec::new()));
    let before = calls.clone();
    let after = calls.clone();
    let assembly = new_builder()
        .with_config(config(1))
        .with_config_path("/runtime/config.yaml")
        .with_hooks(Hooks {
            on_before_start: Some(Arc::new(move |_| before.lock().unwrap().push("before"))),
            on_after_start: Some(Arc::new(move |_| after.lock().unwrap().push("after"))),
        })
        .build()
        .unwrap();
    assert!(calls.lock().unwrap().is_empty());
    assembly.run_before_start();
    assembly.run_after_start();
    assert_eq!(*calls.lock().unwrap(), ["before", "after"]);
    assert!(!assembly.is_materializable());
}

#[test]
fn typed_config_fixture_retains_distinct_secret_handles() {
    let config = config(1);
    let ClaudeSubscriptionAccountConfig {
        access_token_secret,
        refresh_token_secret,
        ..
    } = &config.claude_accounts[0];
    assert_eq!(
        access_token_secret,
        &RuntimeSecretRef {
            scope: "cliproxy".into(),
            name: "claude-access".into()
        }
    );
    assert_ne!(access_token_secret, refresh_token_secret);
}
