// ref: internal/pluginhost/logging_test.go @ a88197f845c979132c8978ea223c6af05cc81536
// Port-Status: ported
// License: MIT (upstream); modifications AGPL-3.0-only

use crate::sdk::pluginapi::Metadata;

use super::logging::{
    plugin_hot_reload_log_fields, plugin_log_fields, plugin_log_fields_from_metadata,
};
