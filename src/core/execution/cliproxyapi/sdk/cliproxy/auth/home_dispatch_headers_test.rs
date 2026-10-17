// ref: sdk/cliproxy/auth/home_dispatch_headers_test.go @ a88197f845c979132c8978ea223c6af05cc81536
// Port-Status: adapted_to_ctox
// Port-Note: verifies normalized dispatch headers through the injected transport boundary
// License: MIT (upstream); modifications AGPL-3.0-only

use std::collections::BTreeMap;

use crate::internal::home::{Client, HomeConfig, HomeTransport};

use super::home_execution_paths_test::TestHomeTransport;
