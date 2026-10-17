// ref: internal/pluginhost/loader_windows_test.go @ a88197f845c979132c8978ea223c6af05cc81536
// Port-Status: adapted_to_ctox
// Port-Note: Windows DLL shadow loading is replaced by named-pipe child processes
// License: MIT (upstream); modifications AGPL-3.0-only

use std::path::Path;

use super::platform::{plugin_file_from_name, PluginPlatform};
