// Origin: CTOX
// License: AGPL-3.0-only

pub mod business_os;
pub mod business_os_app_testing;
pub mod business_os_harness_bench;
pub mod db_migration;
pub mod governance;
pub mod harness_flow;
pub mod harness_mining;
mod lifecycle_timeouts;
pub use lifecycle_timeouts::SERVICE_LIFECYCLE_TIMEOUTS;
pub mod mission_governor;
pub mod process_mining;
pub mod reset;
pub mod state_invariants;
pub mod state_write_guard;
pub mod turn_ledger;
#[cfg(windows)]
pub mod windows_service;
pub mod working_hours;

#[path = "service.rs"]
mod service_loop;

pub use service_loop::*;
