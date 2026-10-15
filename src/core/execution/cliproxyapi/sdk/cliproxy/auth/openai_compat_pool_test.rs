// ref: sdk/cliproxy/auth/openai_compat_pool_test.go @ a88197f845c979132c8978ea223c6af05cc81536
// Port-Status: adapted_to_ctox
// Port-Note: alias-pool order/dedup and selected force-mapping result
// License: MIT (upstream); modifications AGPL-3.0-only

use crate::internal::config::CodexModel;

use super::{
    resolve_model_alias_pool_from_config_models, resolve_model_alias_result_from_config_models,
};

fn model(name: &str, alias: &str, force_mapping: bool) -> CodexModel {
    CodexModel {
        name: name.into(),
        alias: alias.into(),
        force_mapping,
        ..Default::default()
    }
}
