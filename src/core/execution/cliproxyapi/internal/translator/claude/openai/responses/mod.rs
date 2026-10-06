// ref: internal/translator/claude/openai/responses @ a88197f845c979132c8978ea223c6af05cc81536
// Port-Status: adapted_to_ctox
// License: MIT (upstream); modifications AGPL-3.0-only

mod init;
#[path = "claude_openai-responses_request.rs"]
mod request;
#[path = "claude_openai-responses_response.rs"]
mod response;

#[cfg(test)]
mod claude_openai_responses_compat_test;
#[cfg(test)]
mod noop_optimization_test;
#[cfg(test)]
#[path = "claude_openai-responses_request_test.rs"]
mod request_test;
#[cfg(test)]
#[path = "claude_openai-responses_response_test.rs"]
mod response_test;

pub use init::register_openai_responses_claude;
pub use request::{
    convert_openai_responses_request_to_claude,
    convert_openai_responses_request_to_claude_with_compat,
};
pub use response::{
    convert_claude_response_to_openai_responses,
    convert_claude_response_to_openai_responses_non_stream, ClaudeResponsesStreamDecoder,
    ClaudeToResponsesState,
};
