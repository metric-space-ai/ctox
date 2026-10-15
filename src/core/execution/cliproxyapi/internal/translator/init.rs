// ref: internal/translator/init.go @ a88197f845c979132c8978ea223c6af05cc81536
// Port-Status: adapted_to_ctox
// License: MIT (upstream); modifications AGPL-3.0-only

use crate::sdk::translator::Registry;

/// Explicit, dependency-injected replacement for Go package `init()` imports.
/// Only translator packages already active in the Rust module graph are listed;
/// later ports extend this function when their own gates close.
pub fn register_all(registry: &Registry) {
    super::claude::gemini::register_gemini_claude(registry);
    super::claude::interactions::register_interactions_claude(registry);
    super::claude::openai::chat_completions::register_openai_chat_claude_request(registry);
    super::claude::openai::responses::register_openai_responses_claude(registry);

    super::codex::claude::register_claude_codex(registry);
    super::codex::gemini::register_gemini_codex(registry);
    super::codex::interactions::register_interactions_codex(registry);
    super::codex::openai::chat_completions::register_openai_chat_codex(registry);
    super::codex::openai::responses::register_openai_responses_codex(registry);

    super::gemini::claude::register_claude_gemini(registry);
    super::gemini::interactions::register_gemini_interactions(registry);
    super::gemini::passthrough::register_gemini_passthrough(registry);
    super::gemini::openai::chat_completions::register_openai_chat_gemini(registry);
    super::gemini::openai::responses::register_openai_responses_gemini_request(registry);

    super::interactions::claude::register_claude_interactions(registry);

    super::openai::interactions::chat_completions::register_openai_chat_interactions(registry);
    super::openai::interactions::responses::register_openai_responses_interactions(registry);
    super::openai::claude::register_openai_claude(registry);
    super::openai::gemini::register_openai_gemini(registry);
    super::openai::passthrough::chat_completions::register_openai_chat_passthrough(registry);
    super::openai::passthrough::responses::register_openai_responses_chat_completions(registry);

    super::antigravity::claude::register_claude_antigravity(registry);
    super::antigravity::gemini::register_gemini_antigravity(registry);
    super::antigravity::interactions::register_interactions_antigravity(registry);
    super::antigravity::openai::chat_completions::register_openai_chat_antigravity(registry);
    super::antigravity::openai::responses::register_openai_responses_antigravity(registry);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sdk::translator::{claude, codex, interactions, openai, openai_response};

}
