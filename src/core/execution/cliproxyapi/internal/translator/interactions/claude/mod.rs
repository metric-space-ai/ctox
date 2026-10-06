// ref: internal/translator/interactions/claude @ a88197f845c979132c8978ea223c6af05cc81536
// Port-Status: adapted_to_ctox
// License: MIT (upstream); modifications AGPL-3.0-only

mod init;
mod interactions_claude_request;
mod interactions_claude_response;

#[cfg(test)]
mod interactions_claude_compat_test;
#[cfg(test)]
mod interactions_claude_test;

pub use init::register_claude_interactions;
pub use interactions_claude_request::{
    convert_claude_request_to_interactions, convert_claude_request_to_interactions_with_compat,
};
pub use interactions_claude_response::{
    convert_interactions_response_to_claude, convert_interactions_response_to_claude_non_stream,
    InteractionsToClaudeStreamState,
};
