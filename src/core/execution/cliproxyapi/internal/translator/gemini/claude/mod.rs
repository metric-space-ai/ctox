// Origin: CTOX
// License: AGPL-3.0-only

mod gemini_claude_request;
mod gemini_claude_response;
mod init;

#[cfg(test)]
mod gemini_claude_compat_test;
#[cfg(test)]
mod gemini_claude_request_test;
#[cfg(test)]
mod gemini_claude_response_test;

pub use gemini_claude_request::{
    convert_claude_request_to_gemini, convert_claude_request_to_gemini_with_compat,
};
pub use gemini_claude_response::{
    convert_gemini_response_to_claude_non_stream, convert_gemini_response_to_claude_stream,
    gemini_claude_token_count, GeminiToClaudeStreamState,
};
pub use init::register_claude_gemini;
