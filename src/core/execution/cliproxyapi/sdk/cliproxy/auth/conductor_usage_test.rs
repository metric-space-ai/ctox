// ref: sdk/cliproxy/auth/conductor_usage_test.go @ a88197f845c979132c8978ea223c6af05cc81536
// Port-Status: adapted_to_ctox
// License: MIT (upstream); modifications AGPL-3.0-only

use crate::sdk::cliproxy::executor::Options;
use crate::sdk::cliproxy::usage::UsageContext;

use super::usage_context_with_requested_model_alias;

#[test]
fn requested_alias_includes_reasoning_service_tier_and_generate() {
    let mut options = Options::default();
    options.metadata.requested_model = Some("client-model".to_owned());
    options.metadata.reasoning_effort = Some("medium".to_owned());
    options.metadata.service_tier = Some("auto".to_owned());
    options.metadata.generate = Some(false);
    let context = usage_context_with_requested_model_alias(
        UsageContext::default(),
        &options,
        "fallback-model",
    );
    assert_eq!(context.requested_model_alias(), "client-model");
    assert_eq!(context.reasoning_effort(), "medium");
    assert_eq!(context.service_tier(), "auto");
    assert!(!context.generate());
}

#[test]
fn existing_generate_false_is_preserved() {
    let mut options = Options::default();
    options.metadata.requested_model = Some("client-model".to_owned());
    let context = usage_context_with_requested_model_alias(
        UsageContext::default().with_generate(false),
        &options,
        "fallback-model",
    );
    assert!(!context.generate());
}
