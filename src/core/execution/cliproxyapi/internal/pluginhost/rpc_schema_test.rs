// ref: internal/pluginhost/rpc_schema_test.go @ a88197f845c979132c8978ea223c6af05cc81536
// Port-Status: adapted_to_ctox
// Port-Note: complete upstream schema parity inside CTOX process frames
// License: MIT (upstream); modifications AGPL-3.0-only

use serde_json::json;

use super::rpc_schema::*;
use crate::sdk::{
    pluginabi::{Envelope, SCHEMA_VERSION},
    pluginapi::{ConfigField, ConfigFieldType, ExecutorModelScope, ExecutorRequest, Metadata},
};

fn registration() -> RpcRegistration {
    RpcRegistration {
        schema_version: SCHEMA_VERSION,
        metadata: Metadata {
            name: "safe-plugin".into(),
            version: "1.0.0".into(),
            author: "ctox".into(),
            github_repository: "https://example.invalid/safe-plugin".into(),
            logo: String::new(),
            config_fields: vec![ConfigField {
                name: "mode".into(),
                field_type: ConfigFieldType(ConfigFieldType::ENUM.into()),
                enum_values: vec!["safe".into()],
                description: "execution mode".into(),
            }],
        },
        capabilities: RpcCapabilities {
            model_registrar: false,
            model_provider: false,
            auth_provider: false,
            frontend_auth_provider: true,
            frontend_auth_provider_exclusive: true,
            scheduler: true,
            model_router: true,
            executor: true,
            executor_model_scope: ExecutorModelScope(ExecutorModelScope::BOTH.into()),
            executor_input_formats: vec!["openai-responses".into()],
            executor_output_formats: vec!["anthropic-messages".into()],
            request_translator: true,
            request_normalizer: false,
            request_interceptor: false,
            request_lifecycle_plugin: true,
            response_translator: true,
            response_before_translator: false,
            response_after_translator: false,
            response_interceptor: false,
            stream_chunk_interceptor: true,
            thinking_applier: false,
            usage_plugin: false,
            command_line_plugin: false,
            management_api: false,
        },
    }
}
