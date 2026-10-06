// Origin: CTOX
// License: AGPL-3.0-only

mod codex_client_models;
pub(crate) mod openai_chat_handlers;
mod openai_handlers;
mod openai_images_handlers;
pub mod openai_responses_handlers;
mod openai_responses_websocket;
mod openai_responses_websocket_forward;
mod openai_responses_websocket_prewarm;
mod openai_responses_websocket_requests;
mod openai_responses_websocket_session;
mod openai_responses_websocket_timeline;
mod openai_responses_websocket_toolcall_repair;
mod openai_videos_handlers;

pub use codex_client_models::{
    codex_client_models_response, codex_client_models_response_with_multi_agent_v2,
};
pub use openai_handlers::{
    convert_chat_completions_response_to_completions,
    convert_completions_request_to_chat_completions, should_treat_as_responses_format,
};
pub use openai_images_handlers::{
    build_openai_compat_images_json_request, images_model_base, is_supported_images_model,
    normalize_images_response_format, SseFrameAccumulator,
};
pub use openai_responses_websocket::{
    responses_websocket_native_passthrough_allowed, truncate_websocket_close_reason,
    websocket_close_payload_for_upstream_error,
};
pub use openai_responses_websocket_forward::{
    sorted_string_set, websocket_json_payloads_from_chunk,
};
pub use openai_responses_websocket_prewarm::{
    merge_json_array_raw, normalize_json_array_raw, synthetic_responses_websocket_prewarm_payloads,
};
pub use openai_responses_websocket_requests::{
    dedupe_responses_websocket_input_items_by_id, normalize_responses_websocket_passthrough_request,
};
pub use openai_responses_websocket_session::{
    responses_websocket_resolved_model_name, websocket_upstream_supports_incremental_input,
};
pub use openai_responses_websocket_timeline::{
    format_websocket_timeline_event, websocket_payload_event_type, WebsocketTimeline,
};
pub use openai_responses_websocket_toolcall_repair::{
    repair_responses_websocket_tool_calls, WebsocketToolOutputCache,
};
pub use openai_videos_handlers::{
    build_xai_videos_create_request, is_supported_videos_model, openai_video_status,
    VideoAuthBindingStore,
};

#[cfg(test)]
mod openai_responses_handlers_stream_error_test;
#[cfg(test)]
mod openai_responses_websocket_test;
