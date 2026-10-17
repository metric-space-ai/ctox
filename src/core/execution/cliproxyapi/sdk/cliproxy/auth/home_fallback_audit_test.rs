// ref: sdk/cliproxy/auth/home_fallback_audit_test.go @ a88197f845c979132c8978ea223c6af05cc81536
// Port-Status: adapted_to_ctox
// Port-Note: lifecycle-bound local fallback is disabled whenever Home owns dispatch
// License: MIT (upstream); modifications AGPL-3.0-only

use std::sync::Arc;

use super::{Auth, AuthMutationOptions, HomeAuthRuntime};
