// Origin: CTOX
// License: AGPL-3.0-only

pub mod claude_attribution;
#[cfg(test)]
mod claude_attribution_test;
mod claude_model;
#[cfg(test)]
mod claude_model_test;
mod claude_schema;
#[cfg(test)]
mod claude_schema_test;
mod claude_tool_id;
#[cfg(test)]
mod claude_tool_id_test;
mod claude_tool_result;
#[cfg(test)]
mod claude_tool_result_test;
#[path = "util.rs"]
mod core;
pub mod gemini_schema;
#[cfg(test)]
mod gemini_schema_test;
mod gjson;
mod header_helpers;
mod image;
#[cfg(test)]
mod image_test;
mod provider;
mod proxy;
#[cfg(test)]
mod proxy_test;
#[cfg(test)]
mod sanitize_test;
pub mod ssh_helper;
mod translator;

#[cfg(test)]
mod gjson_test;
#[cfg(test)]
mod header_helpers_test;
#[cfg(test)]
mod provider_test;

pub use claude_attribution::{
    is_claude_code_attribution_system_text, strip_claude_code_attribution_system,
};
pub use claude_model::{is_claude_model, is_claude_thinking_model};
pub use claude_schema::{
    has_unsupported_unicode_property_escape, normalize_claude_tool_input_schema,
    strip_unsupported_schema_patterns, SCHEMA_MAP_KEYWORDS, SCHEMA_VALUE_KEYWORDS,
};
pub use claude_tool_id::{
    gemini_claude_tool_use_id, is_gemini_claude_tool_use_id, sanitize_claude_function_name,
    sanitize_claude_tool_id,
};
pub use claude_tool_result::{
    convert_claude_tool_result_content, ClaudeToolResult, ClaudeToolResultImage,
};
pub use core::{
    count_auth_files, log_level_decision, resolve_auth_dir, sanitize_function_name, writable_path,
    AuthRecordStore, AuthStoreFailureKind, AuthStoreListError, AuthStoreListFuture, HostLogLevel,
    LogLevelDecision, ResolveAuthDirError, UtilityHostConfig,
};
pub use gemini_schema::{
    clean_json_schema_for_antigravity, clean_json_schema_for_antigravity_response,
    clean_json_schema_for_gemini,
};
pub use gjson::{get_gjson_bytes_no_copy, parse_gjson_bytes_no_copy};
pub use header_helpers::{
    apply_custom_headers, apply_custom_headers_from_attrs, canonical_header_name,
    extract_custom_headers, HeaderRequest,
};
pub use image::{create_white_image_base64, WhiteImageEncodingError};
pub use provider::{
    get_openai_compatibility_config, get_provider_name, hide_api_key, in_array,
    is_openai_compatibility_alias, mask_authorization_header, mask_sensitive_header_value,
    mask_sensitive_query, openai_compatible_provider_key, resolve_auto_model, ModelRegistryView,
    OpenAiCompatibilityEntryView, OpenAiCompatibilityModelView,
};
pub use proxy::{set_proxy, ProxyTransportTarget};
pub use translator::{
    canonical_tool_name, deduplicate_function_declarations, disambiguated_tool_name_map, fix_json,
    map_sanitized_function_name, map_tool_name, rename_key, restore_sanitized_tool_name,
    sanitized_function_name_map, sanitized_tool_name_map, tool_name_map_from_claude_request,
    walk_json_field_paths, JsonTransformError,
};
