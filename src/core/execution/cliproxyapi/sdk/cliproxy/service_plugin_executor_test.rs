// ref: sdk/cliproxy/service_plugin_executor_test.go @ a88197f845c979132c8978ea223c6af05cc81536
// Port-Status: adapted_to_ctox
// License: MIT (upstream); modifications AGPL-3.0-only

use std::sync::Arc;

use super::service_executors::has_native_openai_compat_executor_config;
use super::service_test_support::{auth, registration, runtime_fixture, TestPluginRuntime};
