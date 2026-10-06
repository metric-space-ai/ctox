// Origin: CTOX
// License: AGPL-3.0-only

mod antigravity_claude_request;
mod antigravity_claude_response;
mod init;
mod signature_validation;
mod web_search;

#[cfg(test)]
mod antigravity_claude_request_test;
#[cfg(test)]
mod antigravity_claude_response_test;
#[cfg(test)]
mod signature_validation_test;

pub(crate) use antigravity_claude_request::convert_claude_request_for_direct_gemini;
pub use antigravity_claude_request::convert_claude_request_to_antigravity;
pub use antigravity_claude_request::{
    claude_request_uses_native_web_search, convert_claude_request_to_antigravity_with_capabilities,
    convert_claude_request_to_antigravity_with_runtime, AntigravityClaudeRequestCapabilities,
    AntigravityClaudeRequestTranslationError,
};
pub use antigravity_claude_response::{
    claude_token_count, convert_antigravity_response_to_claude_non_stream,
    convert_antigravity_response_to_claude_stream,
    convert_antigravity_response_to_claude_stream_with_runtime,
    convert_antigravity_web_search_response_to_claude_non_stream,
    convert_antigravity_web_search_response_to_claude_stream, AntigravityClaudeStreamState,
    AntigravityClaudeWebSearchStreamState,
};
pub use init::{
    register_claude_antigravity, register_claude_antigravity_with_capability_resolver,
    AntigravityClaudeCapabilityResolver,
};
pub use signature_validation::{
    decode_gemini_claude_carrier_signature, encode_gemini_claude_carrier_signature,
    inspect_double_layer_signature, inspect_signature_payload, inspect_single_layer_signature,
    normalize_claude_bypass_signature, strip_empty_signature_thinking_blocks,
    strip_invalid_bypass_signature_thinking_blocks, strip_invalid_gemini_signature_thinking_blocks,
    validate_claude_bypass_signatures, GeminiClaudeCarrier,
};
