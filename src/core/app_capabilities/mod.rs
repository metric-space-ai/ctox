//! App-independent native capability boundary (V4 CAP-1).
//! The runtime supplies a Core-owned host; child-process context is never authority.
#[cfg(test)]
#[path = "handler_registry_tests.rs"]
mod handler_registry_tests;
#[path = "caps/office.rs"]
pub mod cap2;
#[path = "caps/cap3.rs"]
pub mod cap3;
#[path = "caps/cap4.rs"]
pub mod cap4;
pub mod caps;
pub mod native;
pub mod policy;
pub mod protocol;
pub mod registry;
mod schema;

pub use policy::{Actor, CapCallContext, CapabilityGrant, CapabilityHost, ExecutorFence};
pub use protocol::{CtoxError, ErrorCode};
pub use registry::{CapabilityBackend, CapabilityRegistry};

#[cfg(test)]
mod tests;
