// Origin: CTOX
// Port-Status: adapted_to_ctox
// License: AGPL-3.0-only

mod claude_openai_request;
mod claude_openai_response;
mod init;

#[cfg(test)]
mod claude_openai_compat_test;
#[cfg(test)]
mod claude_openai_request_test;
#[cfg(test)]
mod claude_openai_response_test;
#[cfg(test)]
mod noop_optimization_test;

pub use claude_openai_request::{
    convert_openai_chat_request_to_claude, convert_openai_chat_request_to_claude_with_compat,
};
pub use claude_openai_response::{
    convert_claude_response_to_openai_chat_non_stream,
    convert_claude_response_to_openai_chat_stream, ClaudeToChatStreamState,
};
pub use init::register_openai_chat_claude_request;
