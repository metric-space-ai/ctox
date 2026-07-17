// Origin: CTOX
// License: AGPL-3.0-only

mod app_runtime;
mod ats_gates;
mod browser_runtime;
mod capability;
pub(crate) mod command_lifecycle;
mod command_lifecycle_generated;
mod external_sql_sync;
mod importer;
mod invoices;
pub mod mcp_channel;
pub mod office_engine;
mod person_research_command;
pub mod policy;
mod rxdb_peer;
pub mod server;
pub mod store;
mod support;
mod threads;

pub(crate) use app_runtime::inspect_module as inspect_app_runtime_module;
pub(crate) use browser_runtime::BrowserSessionAutomationRequest;
pub use rxdb_peer::browser_context_capture;
pub(crate) use rxdb_peer::browser_session_automation as run_browser_session_automation;
pub use rxdb_peer::browser_session_status;
pub use rxdb_peer::enqueue_business_command_document;
pub use rxdb_peer::native_peer_status;
pub use rxdb_peer::repair_optional_rxdb_collection_schema_drift;
pub use rxdb_peer::run_native_peer_foreground;
pub use rxdb_peer::sync_desktop_file_from_path;
pub use rxdb_peer::sync_desktop_files_from_workspace_root;
pub use rxdb_peer::BrowserContextCaptureRequest;
pub use rxdb_peer::{ensure_native_peer, restart_native_peer};
pub use server::serve_business_os;
pub use server::BusinessOsServeOptions;

pub(crate) use external_sql_sync::start_background_sync;
