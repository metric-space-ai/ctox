// ref: internal/runtime/executor/helps/openai_compat_tool_results_test.go @ a88197f845c979132c8978ea223c6af05cc81536
// Port-Status: ported
// License: MIT (upstream); modifications AGPL-3.0-only

use serde_json::Value;

use super::super::openai_compat_executor::{OpenAiCompatibility, OpenAiCompatibilityModel};
use super::openai_compat_tool_results::{
    normalize_openai_tool_results_text_only, should_normalize_openai_tool_results_for_model,
    OPENAI_TOOL_RESULT_IMAGE_OMITTED_TEXT,
};

fn model(name: &str, alias: &str, modalities: &[&str]) -> OpenAiCompatibilityModel {
    OpenAiCompatibilityModel {
        name: name.to_owned(),
        alias: alias.to_owned(),
        input_modalities: modalities.iter().map(|value| (*value).to_owned()).collect(),
    }
}
