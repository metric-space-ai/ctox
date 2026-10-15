// Platform collection ownership (§16.17/19), not app-specific built-in logic.
export const PLATFORM_APP_ID='ctox-platform';
export const PLATFORM_COLLECTIONS=Object.freeze(['business_commands','business_module_catalog','business_apps','business_app_versions','ctox_queue_tasks','ctox_runtime_settings','shell_preferences','files','app_drafts','_bench']);
export const collectionOwner=(collection,appId)=>PLATFORM_COLLECTIONS.includes(collection)?PLATFORM_APP_ID:appId;
