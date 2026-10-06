// ref: internal/translator/codex/claude @ a88197f845c979132c8978ea223c6af05cc81536
// Port-Status: adapted_to_ctox
// License: MIT (upstream); modifications AGPL-3.0-only

mod codex_claude_request;
mod codex_claude_response;
mod codex_claude_response_web_search;
mod init;

#[cfg(test)]
mod codex_claude_compat_test;
#[cfg(test)]
mod codex_claude_parallel_function_calls_test;
#[cfg(test)]
mod codex_claude_request_benchmark_test;
#[cfg(test)]
mod codex_claude_request_test;
#[cfg(test)]
mod codex_claude_response_test;
#[cfg(test)]
mod noop_optimization_test;

pub use codex_claude_request::{
    convert_claude_request_to_codex, convert_claude_request_to_codex_with_compat,
};
pub use codex_claude_response::{
    claude_token_count, convert_codex_response_to_claude_non_stream,
    convert_codex_response_to_claude_stream, deterministic_claude_message_id,
    CodexToClaudeStreamState,
};
pub use init::register_claude_codex;

pub use super::interactions::{
    convert_codex_response_to_interactions_non_stream,
    convert_codex_response_to_interactions_stream, CodexToInteractionsState,
};
