// Origin: CTOX
// License: AGPL-3.0-only

pub mod claude;
#[path = "handlers.rs"]
mod core;
pub mod gemini;
mod handlers_context;
mod handlers_errors;
mod handlers_execution;
mod handlers_interceptors;
mod handlers_routing;
mod handlers_stream;
pub mod header_filter;
mod model_execution;
pub mod openai;
pub mod openai_responses_stream_error;
pub mod request_body;
pub mod responses_sse_framer;
mod stream_forwarder;

pub use core::{
    build_error_response_body, non_streaming_keep_alive_interval, passthrough_headers_enabled,
    set_generate_metadata, set_reasoning_effort_metadata, set_service_tier_metadata,
    streaming_bootstrap_retries, streaming_keep_alive_interval, ErrorDetail, ErrorResponse,
};
pub use handlers_context::{HandlerCancellation, HandlerRequestContext};
pub use handlers_errors::{build_error_response, HandlerResponse};
pub use handlers_execution::{build_executor_request, HandlerExecutionError};
pub use handlers_interceptors::{
    append_stream_interceptor_history, apply_interceptor_headers, termination_response,
};
pub use handlers_routing::{
    adjust_execution_providers_for_entry_protocol, exclude_execution_provider,
    is_openai_image_only_model, prefer_execution_provider, route_model_base_name,
    validate_image_only_model, HandlerRouteDecision,
};
pub use handlers_stream::{bootstrap_stream, validate_sse_data_json, StreamBootstrap};
pub use model_execution::{
    response_protocol, ModelExecutionChunk, ModelExecutionError, ModelExecutionRequest,
    ModelExecutionResponse, ModelExecutionStream, ProtocolExecutionRequest,
};
pub use stream_forwarder::{ForwardedStreamEvent, StreamForwarder};
