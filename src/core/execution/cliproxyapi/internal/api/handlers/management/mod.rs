// Origin: CTOX
// License: AGPL-3.0-only

mod api_key_usage;
mod api_tools;
mod auth_files;
mod auth_files_crud;
mod auth_files_fields;
mod auth_files_oauth_callback;
mod auth_files_provider_oauth;
mod config_apikey_disable;
mod config_auth_index;
mod config_basic;
mod config_lists;
mod handler;
mod logs;
mod model_definitions;
mod oauth_callback;
mod oauth_sessions;
mod plugin_store;
mod plugins;
mod quota;
mod usage;
mod vertex_import;

#[cfg(test)]
mod api_key_usage_test;
#[cfg(test)]
mod api_tools_test;
#[cfg(test)]
mod auth_files_batch_test;
#[cfg(test)]
mod auth_files_delete_test;
#[cfg(test)]
mod auth_files_download_test;
#[cfg(test)]
mod auth_files_download_windows_test;
#[cfg(test)]
mod auth_files_filter_test;
#[cfg(test)]
mod auth_files_patch_fields_test;
#[cfg(test)]
mod auth_files_plugin_oauth_test;
#[cfg(test)]
mod auth_files_project_id_test;
#[cfg(test)]
mod auth_files_recent_requests_test;
#[cfg(test)]
mod auth_files_upload_test;
#[cfg(test)]
mod candidate_claude_oauth_metadata_test;
#[cfg(test)]
mod config_apikey_disable_test;
#[cfg(test)]
mod config_basic_weight_test;
#[cfg(test)]
mod config_codex_alpha_search_test;
#[cfg(test)]
mod config_lists_delete_keys_test;
#[cfg(test)]
mod config_openai_compat_test;
#[cfg(test)]
mod config_weight_test;
#[cfg(test)]
mod config_xai_key_test;
#[cfg(test)]
mod handler_test;
#[cfg(test)]
mod logs_test;
#[cfg(test)]
mod oauth_callback_test;
#[cfg(test)]
mod oauth_codex_concurrency_test;
#[cfg(test)]
mod oauth_sessions_test;
#[cfg(test)]
mod plugin_store_test;
#[cfg(test)]
mod plugins_test;
#[cfg(test)]
mod quota_test;
#[cfg(test)]
mod test_main_test;
#[cfg(test)]
mod test_store_test;
#[cfg(test)]
mod usage_test;

pub use api_key_usage::{
    api_key_usage_payload, ManagementApiKeyUsageError, ManagementApiKeyUsageRecord,
    ManagementApiKeyUsageSource, ManagementRecentRequestBucket,
};
pub use api_tools::{
    ManagementApiCallExecutor, ManagementApiToolError, ManagementApiToolRequest,
    ManagementApiToolResponse, ManagementApiTools,
};
pub use auth_files::{
    ManagementCredentialBatchResult, ManagementCredentialError, ManagementCredentialFailure,
    ManagementCredentialFilter, ManagementCredentialRecord, ManagementCredentialService,
    ManagementCredentialStore, ManagementCredentialStoreError,
};
pub use auth_files_fields::{
    safe_credential_filename, ManagementCredentialDownload, ManagementCredentialFieldError,
    ManagementCredentialPatch, ManagementCredentialRuntimeDetails,
    ManagementCredentialRuntimeSource, ManagementCredentialView,
};
pub use auth_files_oauth_callback::{
    management_oauth_callback_path, oauth_callback_provider, ManagementOAuthCallback,
    ManagementOAuthCallbackError, ManagementOAuthCallbackSink, ManagementOAuthCallbackSinkError,
    ManagementOAuthCallbacks,
};
pub use auth_files_provider_oauth::{
    claude_oauth_runtime_metadata, ManagementProviderOAuth, ManagementProviderOAuthAuthority,
    ManagementProviderOAuthAuthorityError, ManagementProviderOAuthError,
    ManagementProviderOAuthPoll, ManagementProviderOAuthStart,
};
pub use config_apikey_disable::{
    patch_management_provider_key, set_config_api_key_excluded_all,
    toggle_config_api_key_excluded_all, ConfigApiKeyToggleError, ManagementProviderKeyKind,
    ManagementProviderKeyPatch, ManagementProviderKeyPatchError,
};
pub use config_auth_index::{
    management_auth_index_for_id, management_config_auth_indices,
    management_openai_compatibility_views, ManagementConfigAuthIndex,
    ManagementOpenAiCompatibilityView,
};
pub use config_basic::{
    normalize_routing_strategy, ManagementConfigError, ManagementConfigService,
    ManagementConfigStore, ManagementConfigStoreError,
};
pub use config_lists::ManagementAccountConfig;
pub use handler::{
    management_support_plugin_header, ManagementAuthClock, ManagementAuthError,
    ManagementAuthenticator, ManagementConfigReload, ManagementHandlerOwner,
    SystemManagementAuthClock,
};
pub use logs::{
    ManagementLogAttachment, ManagementLogError, ManagementLogPage, ManagementLogQuery,
    ManagementLogStore, ManagementLogs, ManagementRequestErrorLog,
};
pub use model_definitions::{static_model_definitions_payload, StaticModelDefinitionsError};
pub use oauth_callback::{
    ManagementOAuthCallbackHandler, ManagementOAuthCallbackRequest,
    ManagementOAuthCallbackRequestError,
};
pub use oauth_sessions::{
    normalize_oauth_provider, normalize_plugin_oauth_provider, validate_oauth_state,
    ManagementOAuthClock, ManagementOAuthSession, ManagementOAuthSessionError,
    ManagementOAuthSessionSource, ManagementOAuthSessions,
};
pub use plugin_store::{
    ManagementPluginInstallRequest, ManagementPluginInstallResult, ManagementPluginStagedInstall,
    ManagementPluginStoreAuthority, ManagementPluginStoreAuthorityError,
    ManagementPluginStoreCatalog, ManagementPluginStoreEntry, ManagementPluginStoreError,
    ManagementPluginStoreService, ManagementPluginStoreSource,
};
pub use plugins::{
    validate_plugin_id, ManagementPluginConfig, ManagementPluginConfigField,
    ManagementPluginConfigPatch, ManagementPluginConfigStore, ManagementPluginConfigStoreError,
    ManagementPluginError, ManagementPluginMetadata, ManagementPluginRuntimeRecord,
    ManagementPluginRuntimeSource, ManagementPluginService, ManagementPluginSnapshot,
    ManagementPluginView,
};
pub use quota::{
    CooldownManagementQuotaReset, ManagementQuotaAccount, ManagementQuotaResetError,
    ManagementQuotaResetResult, ManagementQuotaResetSource, ManagementQuotaSwitchError,
    ManagementQuotaSwitchSource, ManagementQuotaSwitches,
};
pub use usage::{
    parse_usage_queue_count, usage_queue_payload, ManagementUsageQueue, ManagementUsageQueueError,
};
pub use vertex_import::{
    import_vertex_credential, label_for_vertex, sanitize_vertex_file_part,
    ManagementVertexImportError, ManagementVertexImportResult,
};
