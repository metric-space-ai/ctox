// Origin: CTOX
// License: AGPL-3.0-only

mod core;

#[path = "auth/go/main.rs"]
mod auth;
#[path = "cli/go/main.rs"]
mod cli;
#[path = "codex-service-tier/go/main.rs"]
mod codex_service_tier;
#[path = "executor/go/main.rs"]
mod executor;
#[path = "frontend-auth/go/main.rs"]
mod frontend_auth;
#[path = "frontend-auth-exclusive/go/main.rs"]
mod frontend_auth_exclusive;
#[path = "host-callback/go/main.rs"]
mod host_callback;
#[path = "host-callback-auth-files/go/main.rs"]
mod host_callback_auth_files;
#[path = "host-model-callback/go/main.rs"]
mod host_model_callback;
#[path = "management-api/go/main.rs"]
mod management_api;
#[path = "model/go/main.rs"]
mod model;
#[path = "protocol-format/go/main.rs"]
mod protocol_format;
#[path = "request-lifecycle/go/main.rs"]
mod request_lifecycle;
#[path = "request-normalizer/go/main.rs"]
mod request_normalizer;
#[path = "request-translator/go/main.rs"]
mod request_translator;
#[path = "response-normalizer/go/main.rs"]
mod response_normalizer;
#[path = "response-translator/go/main.rs"]
mod response_translator;
#[path = "scheduler/go/main.rs"]
mod scheduler;
#[path = "simple/go/main.rs"]
mod simple;
#[path = "thinking/go/main.rs"]
mod thinking;
#[path = "usage/go/main.rs"]
mod usage;

#[path = "claude-web-search-router/go/main.rs"]
mod claude_web_search_router;

#[cfg(test)]
mod tests {
    use super::*;

}
