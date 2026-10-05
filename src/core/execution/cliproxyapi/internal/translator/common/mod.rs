// Origin: CTOX
// License: AGPL-3.0-only
mod apply_patch_events;
mod apply_patch_input;
mod apply_patch_responses;
mod bytes;
mod cache_control;
mod claude_native_response;
mod claude_system;
mod file_data;
mod interactions_usage;
mod json;
mod sse;

pub use apply_patch_responses::normalize_apply_patch_responses_request;
pub(crate) use apply_patch_responses::{
    delete_raw_path, prefer_chat_function_patch_tools, qualify_namespace_tool_name, set_json_i64,
    set_json_string, set_raw_path, ApplyPatchResponsesBridge,
};
pub use bytes::{
    append_sse_event, claude_input_tokens_json, gemini_token_count_json, join_raw_array,
    new_raw_array_items, set_raw_array_items, set_string_without_html_escape, sse_event_data,
};
pub use cache_control::{
    attach_cache_control, attach_message_cache_control, attach_tool_message_cache_control,
};
pub use claude_native_response::claude_messages_json_to_sse;
pub use claude_system::claude_message_system_reminder_text;
pub use file_data::normalize_openai_file_data;
pub use interactions_usage::interactions_usage;
pub use json::{set_top_level_string, JsonField, RawJson};
pub use sse::{SseDecoder, SseEvent};

#[cfg(test)]
mod bytes_test;
#[cfg(test)]
mod cache_control_test;
#[cfg(test)]
mod file_data_test;
