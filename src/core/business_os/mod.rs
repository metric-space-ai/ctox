// Origin: CTOX
// License: AGPL-3.0-only

mod app_runtime;
mod ats_gates;
mod backup_restore;
mod browser_control;
mod browser_runtime;
mod capability;
mod command_plane;
mod control_command_types;
mod crew_commands;
mod customer_apps;
pub mod decision_hub;
mod desktop_files;
mod domain_effect;
mod external_sql_sync;
pub(crate) mod harness_cockpit;
mod hashing;
mod importer;
mod inventory_drift_tests;
mod invoices;
mod iot_supervision;
pub mod mcp_channel;
pub mod mobile_invites;
mod module_lifecycle;
mod module_manifest_loader;
pub mod office_cli;
pub mod office_engine;
pub(crate) mod office_staging_repair;
mod person_research_command;
mod person_research_gap_closure;
pub mod policy;
mod project_chats;
mod rxdb_peer;
mod rxdb_peer_browser;
mod rxdb_peer_commands;
mod rxdb_peer_demand_files;
mod rxdb_peer_desktop_files;
mod rxdb_peer_domain_recovery;
mod rxdb_peer_intake;
mod rxdb_peer_intake_state;
mod rxdb_peer_projections;
mod rxdb_peer_tombstones;
mod rxdb_peer_workjet_devices;
pub mod server;
mod session;
mod shell_assets;
pub mod shell_update;
pub mod store;
mod store_appsec_commands;
mod store_ats_commands;
mod store_catalog_projections;
mod store_customer_commands;
mod store_office_commands;
mod store_outbound_commands;
mod store_outbound_delivery_policy;
mod store_policy;
mod store_policy_audit;
mod store_projections;
mod store_release_review;
mod store_workjet_computers;
mod store_workjet_projects;
mod store_workjet_sessions;
mod support;
mod threads;
mod worker_profile_bindings;
pub mod workjet_transfer_git;

pub(crate) use app_runtime::inspect_module as inspect_app_runtime_module;
pub use browser_control::browser_context_capture;
pub(crate) use browser_control::browser_session_automation as run_browser_session_automation;
pub use browser_control::browser_session_status;
pub use browser_control::BrowserContextCaptureRequest;
pub fn audit_customer_apps(root: &std::path::Path) -> anyhow::Result<serde_json::Value> {
    let entries = customer_apps::audit_runtime_customer_apps(root)?;
    let blocked = entries
        .iter()
        .filter(|entry| entry.status == "blocked")
        .count();
    Ok(serde_json::json!({
        "type": "ctox.business-os.customer-app-audit.v1",
        "ok": blocked == 0,
        "read_only": true,
        "blocked": blocked,
        "entries": entries,
    }))
}
pub(crate) use browser_runtime::BrowserSessionAutomationRequest;
pub use rxdb_peer::enqueue_business_command_document;
pub use rxdb_peer::native_peer_status;
pub use rxdb_peer::repair_optional_rxdb_collection_schema_drift;
pub use rxdb_peer::run_native_peer_foreground;
pub(crate) use rxdb_peer::sync_business_record_projections;
pub use rxdb_peer::sync_desktop_file_from_path;
pub use rxdb_peer::sync_desktop_files_from_workspace_root;
pub(crate) use rxdb_peer::sync_knowledge_tables;
pub use rxdb_peer::{ensure_native_peer, native_peer_maintenance_health, restart_native_peer};
pub use server::serve_business_os;
pub use server::BusinessOsServeOptions;

pub(crate) use external_sql_sync::start_background_sync;
pub(crate) use person_research_command::recover_once as recover_person_research_commands_once;
pub use store_workjet_sessions::{run_workjet_session_transfer_recovery, RecoveryOutcome};
pub(crate) use workjet_transfer_git::execute_cli as execute_workjet_transfer_git_cli;
