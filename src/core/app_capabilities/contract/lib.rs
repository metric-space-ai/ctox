//! Isolated checks of the production contract sources, without the daemon's native backend.
#[path = "../caps/mod.rs"]
pub mod caps;
#[path = "../policy.rs"]
pub mod policy;
#[path = "../protocol.rs"]
pub mod protocol;
#[path = "../registry.rs"]
pub mod registry;
#[path = "../schema.rs"]
pub mod schema;
pub use policy::{Actor, CapCallContext, CapabilityGrant, CapabilityHost, ExecutorFence};
pub use protocol::{CtoxError, ErrorCode};
pub use registry::{CapabilityBackend, CapabilityRegistry};
// Production family files refer to this module name in the daemon.
pub mod app_capabilities {
    pub use crate::*;
}
#[cfg(test)]
#[path = "../handler_calls.rs"]
mod handler_calls;
#[cfg(test)]
#[path = "../handler_knowledge_tests.rs"]
mod handler_knowledge_tests;
#[cfg(test)]
#[path = "../tests.rs"]
mod tests;
