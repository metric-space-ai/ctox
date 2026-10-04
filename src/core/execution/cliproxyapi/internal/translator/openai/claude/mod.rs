// Origin: CTOX
// License: AGPL-3.0-only
//
// Local module wiring for the openai → claude translator leaf. The leaf
// follows the same shape as the worked example in
// `claude/openai/chat-completions/mod.rs`: production code, registration, and
// test modules all live under this directory and are re-exported so that any
// future parent module can opt in via a single `pub mod claude;` line.

mod init;
mod openai_claude_request;
mod openai_claude_response;

#[cfg(test)]
mod openai_claude_compat_test;
#[cfg(test)]
mod openai_claude_request_test;
#[cfg(test)]
mod openai_claude_response_test;

pub use init::register_openai_claude;
pub use openai_claude_request::{
    convert_claude_request_to_openai, convert_claude_request_to_openai_with_compat,
};
pub use openai_claude_response::{
    claude_token_count, convert_openai_response_to_claude,
    convert_openai_response_to_claude_non_stream, OpenAIToClaudeStreamState,
};
