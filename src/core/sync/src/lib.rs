//! Shared native execution authority for CTOX Sync.
//! Data replication and execution ownership are deliberately separate protocols.
pub mod authority;
pub mod business_data;
pub use authority::auth::business_data_identity;
#[path = "business-data.generated.rs"]
pub mod business_data_contract;
pub mod checkpoint;
#[path = "contracts.generated.rs"]
pub mod contracts;
pub mod credential_ipc;
pub mod host_config;
#[cfg(feature = "webrtc")]
pub mod host_runtime;
#[cfg(feature = "webrtc")]
pub mod host_transport;
pub mod ipc;
#[cfg(unix)]
pub mod local_host;
#[cfg(feature = "webrtc")]
pub mod native;
#[cfg(feature = "webrtc")]
pub mod native_execution;
