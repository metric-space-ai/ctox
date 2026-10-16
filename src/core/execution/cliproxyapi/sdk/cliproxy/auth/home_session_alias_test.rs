// ref: sdk/cliproxy/auth/home_session_alias_test.go @ a88197f845c979132c8978ea223c6af05cc81536
// Port-Status: adapted_to_ctox
// Port-Note: alias reconciliation, expiry and clear behavior
// License: MIT (upstream); modifications AGPL-3.0-only

use std::time::Duration;

use super::HomeSessionAliasCache;
