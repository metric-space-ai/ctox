// ref: cmd/server/main_test.go @ a88197f845c979132c8978ea223c6af05cc81536
// Port-Status: ported
// License: MIT (upstream); modifications AGPL-3.0-only

use super::*;

fn config(keys: &[&str]) -> ServerConfig {
    let mut config = ServerConfig::default();
    config.sdk.api_keys = keys.iter().map(|key| (*key).to_owned()).collect();
    config
}

#[test]
fn example_key_safe_mode_matches_upstream_matrix() {
    let example = config(&["real-key", " your-api-key-1 "]);
    let real = config(&["real-key"]);
    assert!(should_enable_example_api_key_safe_mode(
        Some(&example),
        false,
        false,
        false,
        false,
        false
    ));
    assert!(should_enable_example_api_key_safe_mode(
        Some(&example),
        false,
        true,
        true,
        false,
        false
    ));
    assert!(!should_enable_example_api_key_safe_mode(
        Some(&example),
        false,
        true,
        false,
        false,
        false
    ));
    assert!(!should_enable_example_api_key_safe_mode(
        Some(&example),
        true,
        false,
        false,
        false,
        false
    ));
    assert!(!should_enable_example_api_key_safe_mode(
        Some(&example),
        false,
        false,
        false,
        false,
        true
    ));
    assert!(!should_enable_example_api_key_safe_mode(
        Some(&example),
        false,
        false,
        false,
        true,
        false
    ));
    assert!(!should_enable_example_api_key_safe_mode(
        Some(&real),
        false,
        false,
        false,
        false,
        false
    ));
    assert!(!should_enable_example_api_key_safe_mode(
        None, false, false, false, false, false
    ));
}

#[test]
fn model_catalog_plan_matches_upstream_matrix() {
    assert_eq!(
        model_catalog_updater_plan(false, false),
        CatalogUpdaterPlan {
            start_models: true,
            start_codex_client: true
        }
    );
    assert_eq!(
        model_catalog_updater_plan(false, true),
        CatalogUpdaterPlan {
            start_models: false,
            start_codex_client: true
        }
    );
    assert_eq!(
        model_catalog_updater_plan(true, false),
        CatalogUpdaterPlan {
            start_models: false,
            start_codex_client: false
        }
    );
    assert_eq!(
        model_catalog_updater_plan(true, true),
        CatalogUpdaterPlan {
            start_models: false,
            start_codex_client: false
        }
    );
}

#[test]
fn bootstrap_path_uses_injected_working_directory() {
    let cwd = Path::new("/srv/ctox");
    assert_eq!(
        plugin_bootstrap_config_path(&[], None, cwd),
        cwd.join("config.yaml")
    );
    assert_eq!(
        plugin_bootstrap_config_path(&["--config=custom.yaml".to_owned()], None, cwd),
        PathBuf::from("custom.yaml")
    );
    assert_eq!(
        plugin_bootstrap_config_path(&["--".to_owned(), "--config=x".to_owned()], None, cwd),
        cwd.join("config.yaml")
    );
}
