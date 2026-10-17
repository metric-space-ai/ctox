// ref: sdk/cliproxy/auth/home_concurrency_test.go @ a88197f845c979132c8978ea223c6af05cc81536
// Port-Status: adapted_to_ctox
// Port-Note: tuple validation, identity and registry installation behavior
// License: MIT (upstream); modifications AGPL-3.0-only

use std::time::Duration;

use crate::sdk::cliproxy::executionregistry::{Registry, ScopeSpec};

use super::{
    canonical_home_concurrency_model_key, decode_home_concurrency, decode_home_dispatch_error,
    install_home_concurrency_scope, verify_home_concurrency_identity, HomeConcurrencyBusyError,
    HomeConcurrencyError,
};
