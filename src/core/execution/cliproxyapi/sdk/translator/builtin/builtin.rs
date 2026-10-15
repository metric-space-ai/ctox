// ref: sdk/translator/builtin/builtin.go @ a88197f845c979132c8978ea223c6af05cc81536
// Port-Status: adapted_to_ctox
// License: MIT (upstream); modifications AGPL-3.0-only

use std::sync::Arc;

use super::{Pipeline, Registry};

/// Returns an independently owned registry populated with every built-in
/// translator. CTOX deliberately replaces upstream's mutable package global so
/// plugin hooks and registrations cannot leak between gateway instances.
pub fn registry() -> Arc<Registry> {
    let registry = Arc::new(Registry::new());
    crate::internal::translator::register_all(&registry);
    registry
}

pub fn pipeline() -> Pipeline {
    Pipeline::new(registry())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sdk::translator::{claude, openai, openai_response};
}
