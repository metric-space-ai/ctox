//! App-independent native capability boundary (V4 CAP-1).
//! The runtime supplies a Core-owned host; child-process context is never authority.
#[path = "caps/office.rs"]
pub mod cap2;
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
