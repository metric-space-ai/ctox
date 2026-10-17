// ref: internal/runtime/executor/openai_compat_executor_tool_results_test.go @ a88197f845c979132c8978ea223c6af05cc81536
// Port-Status: adapted_to_ctox
// License: MIT (upstream); modifications AGPL-3.0-only

use serde_json::Value;

use super::openai_compat_executor::{
    normalize_openai_tool_results_text_only, should_normalize_openai_tool_results_for_model,
    OpenAiCompatibility, OpenAiCompatibilityModel,
};
