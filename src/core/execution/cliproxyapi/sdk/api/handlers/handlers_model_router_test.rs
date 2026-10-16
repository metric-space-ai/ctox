// ref: sdk/api/handlers/handlers_model_router_test.go @ a88197f845c979132c8978ea223c6af05cc81536
// Port-Status: adapted_to_ctox
// License: MIT (upstream); modifications AGPL-3.0-only

use super::*;

#[test]
fn provider_preference_and_exclusion_are_stable_and_case_insensitive() {
    let providers = vec![
        "openai".to_owned(),
        "Gemini-Interactions".to_owned(),
        "claude".to_owned(),
    ];
    assert_eq!(
        prefer_execution_provider(&providers, "gemini-interactions"),
        ["Gemini-Interactions", "openai", "claude"]
    );
    assert_eq!(
        exclude_execution_provider(&providers, "OPENAI"),
        ["Gemini-Interactions", "claude"]
    );
}
