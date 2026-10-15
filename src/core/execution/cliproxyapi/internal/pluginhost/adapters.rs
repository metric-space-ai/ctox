// ref: internal/pluginhost/adapters.go @ a88197f845c979132c8978ea223c6af05cc81536
// Port-Status: adapted_to_ctox
// Port-Note: capability calls receive process-scoped callback identity
// License: MIT (upstream); modifications AGPL-3.0-only

use serde::{de::DeserializeOwned, Serialize};
use std::sync::Arc;

use crate::sdk::pluginabi::{
    METHOD_COMMAND_LINE_EXECUTE, METHOD_COMMAND_LINE_REGISTER, METHOD_MANAGEMENT_HANDLE,
    METHOD_MANAGEMENT_REGISTER,
};
use crate::sdk::pluginapi::{
    Capabilities, CommandLineExecutionRequest, CommandLineExecutionResponse, CommandLinePlugin,
    CommandLineRegistrationRequest, CommandLineRegistrationResponse, ManagementApi,
    ManagementHandler, ManagementRegistrationRequest, ManagementRegistrationResponse,
    ManagementRequest, ManagementResponse, ManagementRoute, Plugin, PluginFuture, ResourceRoute,
};

use super::adapters_auth::{RpcFrontendAuthProvider, RpcModelProvider, RpcModelRegistrar};
use super::adapters_executors::RpcProviderExecutor;
use super::adapters_interceptors::{
    RpcRequestInterceptor, RpcRequestLifecyclePlugin, RpcResponseInterceptor,
    RpcStreamChunkInterceptor,
};
use super::adapters_usage_translation::{
    ResponseNormalizePhase, RpcRequestNormalizer, RpcRequestTranslator, RpcResponseNormalizer,
    RpcResponseTranslator, RpcThinkingApplier, RpcUsagePlugin,
};
use super::auth_provider::{HostConfigSummarySource, RpcAuthProvider};

use super::abi::PluginClientError;
use super::callback_contexts::{CallbackAuthority, CallbackContextRegistry};
use super::model_router::RpcModelRouter;
use super::rpc_client::RpcPluginClient;
use super::scheduler::RpcScheduler;
use super::snapshot::CapabilityRecord;
use super::stream_bridge::StreamBridge;

#[derive(Clone)]
pub struct RpcCapabilityClient {
    plugin_id: String,
    client: RpcPluginClient,
    contexts: CallbackContextRegistry,
}

impl std::fmt::Debug for RpcCapabilityClient {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("RpcCapabilityClient")
            .field("plugin_id", &self.plugin_id)
            .field("active_callback_contexts", &self.contexts.len())
            .finish_non_exhaustive()
    }
}

impl RpcCapabilityClient {
    pub fn new(
        plugin_id: &str,
        client: RpcPluginClient,
        contexts: CallbackContextRegistry,
    ) -> Result<Self, CapabilityClientConfigError> {
        let plugin_id = plugin_id.trim();
        if plugin_id.is_empty() || plugin_id.len() > 128 || plugin_id.chars().any(char::is_control)
        {
            return Err(CapabilityClientConfigError::InvalidPluginId);
        }
        Ok(Self {
            plugin_id: plugin_id.to_owned(),
            client,
            contexts,
        })
    }

    pub async fn call<Request, Response>(
        &self,
        method: &str,
        request: &Request,
        deadline_unix_ms: Option<u64>,
    ) -> Result<Response, PluginClientError>
    where
        Request: Serialize,
        Response: DeserializeOwned,
    {
        let lease = self.contexts.open(CallbackAuthority::new(
            self.plugin_id.clone(),
            deadline_unix_ms,
        ));
        let wrapped = RpcCallbackRequest {
            request,
            host_callback_id: lease.id(),
        };
        let result = self.client.call(method, &wrapped, deadline_unix_ms).await;
        drop(lease);
        result
    }

    pub fn plugin_id(&self) -> &str {
        &self.plugin_id
    }

    pub fn contexts(&self) -> &CallbackContextRegistry {
        &self.contexts
    }

    pub fn client(&self) -> &RpcPluginClient {
        &self.client
    }
}

#[derive(Serialize)]
struct RpcCallbackRequest<'a, Request> {
    #[serde(flatten)]
    request: &'a Request,
    host_callback_id: &'a str,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CapabilityClientConfigError {
    InvalidPluginId,
}

impl std::fmt::Display for CapabilityClientConfigError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("plugin capability client identifier is invalid")
    }
}

impl std::error::Error for CapabilityClientConfigError {}

#[derive(Clone, Debug)]
pub struct RpcCommandLinePlugin {
    client: RpcCapabilityClient,
}

impl RpcCommandLinePlugin {
    pub fn new(client: RpcCapabilityClient) -> Self {
        Self { client }
    }
}

impl CommandLinePlugin for RpcCommandLinePlugin {
    fn register_command_line<'a>(
        &'a self,
        request: CommandLineRegistrationRequest,
    ) -> PluginFuture<'a, CommandLineRegistrationResponse> {
        Box::pin(async move {
            self.client
                .call(METHOD_COMMAND_LINE_REGISTER, &request, None)
                .await
                .map_err(plugin_error)
        })
    }

    fn execute_command_line<'a>(
        &'a self,
        request: CommandLineExecutionRequest,
    ) -> PluginFuture<'a, CommandLineExecutionResponse> {
        Box::pin(async move {
            self.client
                .call(METHOD_COMMAND_LINE_EXECUTE, &request, None)
                .await
                .map_err(plugin_error)
        })
    }
}

#[derive(Clone, Debug)]
pub struct RpcManagementApi {
    client: RpcCapabilityClient,
}

impl RpcManagementApi {
    pub fn new(client: RpcCapabilityClient) -> Self {
        Self { client }
    }
}

impl ManagementApi for RpcManagementApi {
    fn register_management<'a>(
        &'a self,
        request: ManagementRegistrationRequest,
    ) -> PluginFuture<'a, ManagementRegistrationResponse> {
        Box::pin(async move {
            let response: RpcManagementRegistrationResponse = self
                .client
                .call(METHOD_MANAGEMENT_REGISTER, &request, None)
                .await
                .map_err(plugin_error)?;
            Ok(ManagementRegistrationResponse {
                routes: response
                    .routes
                    .into_iter()
                    .map(|route| ManagementRoute {
                        method: route.method,
                        path: route.path,
                        menu: route.menu,
                        description: route.description,
                        handler: Arc::new(RpcManagementHandler {
                            client: self.client.clone(),
                        }),
                    })
                    .collect(),
                resources: response
                    .resources
                    .into_iter()
                    .map(|route| ResourceRoute {
                        path: route.path,
                        menu: route.menu,
                        description: route.description,
                        handler: Arc::new(RpcManagementHandler {
                            client: self.client.clone(),
                        }),
                    })
                    .collect(),
            })
        })
    }
}

#[derive(Clone, Debug)]
struct RpcManagementHandler {
    client: RpcCapabilityClient,
}

impl ManagementHandler for RpcManagementHandler {
    fn handle_management<'a>(
        &'a self,
        request: ManagementRequest,
    ) -> PluginFuture<'a, ManagementResponse> {
        Box::pin(async move {
            self.client
                .call(METHOD_MANAGEMENT_HANDLE, &request, None)
                .await
                .map_err(plugin_error)
        })
    }
}

#[derive(Default, serde::Deserialize)]
#[serde(default)]
struct RpcManagementRegistrationResponse {
    #[serde(alias = "Routes")]
    routes: Vec<RpcManagementRoute>,
    #[serde(alias = "Resources")]
    resources: Vec<RpcManagementRoute>,
}

#[derive(Default, serde::Deserialize)]
#[serde(default)]
struct RpcManagementRoute {
    #[serde(alias = "Method")]
    method: String,
    #[serde(alias = "Path")]
    path: String,
    #[serde(alias = "Menu")]
    menu: String,
    #[serde(alias = "Description")]
    description: String,
}

fn plugin_error(error: PluginClientError) -> crate::sdk::pluginapi::PluginExecutionError {
    Arc::new(error)
}

pub fn plugin_from_record(
    record: &CapabilityRecord,
    contexts: CallbackContextRegistry,
    streams: StreamBridge,
    host: Arc<dyn HostConfigSummarySource>,
) -> Result<Plugin, CapabilityClientConfigError> {
    let client = RpcCapabilityClient::new(&record.id, record.client.clone(), contexts)?;
    let rpc = &record.capabilities;
    let identifier = |name: &str| record.identifiers.get(name).map(String::as_str);
    let auth_provider = if rpc.auth_provider {
        identifier("auth_provider")
            .and_then(|identifier| RpcAuthProvider::new(identifier, client.clone(), host).ok())
            .map(|provider| Arc::new(provider) as _)
    } else {
        None
    };
    let frontend_auth_provider = if rpc.frontend_auth_provider {
        identifier("frontend_auth_provider")
            .and_then(|identifier| RpcFrontendAuthProvider::new(identifier, client.clone()).ok())
            .map(|provider| Arc::new(provider) as _)
    } else {
        None
    };
    let executor = if rpc.executor {
        identifier("executor")
            .and_then(|identifier| {
                RpcProviderExecutor::new(identifier, client.clone(), streams).ok()
            })
            .map(|executor| Arc::new(executor) as _)
    } else {
        None
    };
    let thinking_applier = if rpc.thinking_applier {
        identifier("thinking_applier")
            .and_then(|identifier| RpcThinkingApplier::new(identifier, client.clone()).ok())
            .map(|provider| Arc::new(provider) as _)
    } else {
        None
    };
    Ok(Plugin {
        metadata: record.metadata.clone(),
        capabilities: Capabilities {
            model_registrar: rpc
                .model_registrar
                .then(|| Arc::new(RpcModelRegistrar::new(client.clone())) as _),
            model_provider: rpc
                .model_provider
                .then(|| Arc::new(RpcModelProvider::new(client.clone())) as _),
            auth_provider,
            frontend_auth_provider,
            frontend_auth_provider_exclusive: rpc.frontend_auth_provider_exclusive,
            scheduler: rpc
                .scheduler
                .then(|| Arc::new(RpcScheduler::new(record.client.clone())) as _),
            model_router: rpc
                .model_router
                .then(|| Arc::new(RpcModelRouter::new(record.client.clone())) as _),
            executor,
            executor_model_scope: rpc.executor_model_scope.clone(),
            executor_input_formats: rpc.executor_input_formats.clone(),
            executor_output_formats: rpc.executor_output_formats.clone(),
            request_translator: rpc
                .request_translator
                .then(|| Arc::new(RpcRequestTranslator::new(client.clone())) as _),
            request_normalizer: rpc
                .request_normalizer
                .then(|| Arc::new(RpcRequestNormalizer::new(client.clone())) as _),
            response_translator: rpc
                .response_translator
                .then(|| Arc::new(RpcResponseTranslator::new(client.clone())) as _),
            response_before_translator: rpc.response_before_translator.then(|| {
                Arc::new(RpcResponseNormalizer::new(
                    client.clone(),
                    ResponseNormalizePhase::Before,
                )) as _
            }),
            response_after_translator: rpc.response_after_translator.then(|| {
                Arc::new(RpcResponseNormalizer::new(
                    client.clone(),
                    ResponseNormalizePhase::After,
                )) as _
            }),
            request_interceptor: rpc
                .request_interceptor
                .then(|| Arc::new(RpcRequestInterceptor::new(client.clone())) as _),
            request_lifecycle_plugin: rpc
                .request_lifecycle_plugin
                .then(|| Arc::new(RpcRequestLifecyclePlugin::new(client.clone())) as _),
            response_interceptor: rpc
                .response_interceptor
                .then(|| Arc::new(RpcResponseInterceptor::new(client.clone())) as _),
            stream_chunk_interceptor: rpc
                .stream_chunk_interceptor
                .then(|| Arc::new(RpcStreamChunkInterceptor::new(client.clone())) as _),
            thinking_applier,
            // The additive quota SDK slot is not advertised by process plugins
            // until their guarded quota RPC adapter is installed.
            quota_provider: None,
            usage_plugin: rpc
                .usage_plugin
                .then(|| Arc::new(RpcUsagePlugin::new(client.clone())) as _),
            command_line_plugin: rpc
                .command_line_plugin
                .then(|| Arc::new(RpcCommandLinePlugin::new(client.clone())) as _),
            management_api: rpc
                .management_api
                .then(|| Arc::new(RpcManagementApi::new(client)) as _),
        },
    })
}
