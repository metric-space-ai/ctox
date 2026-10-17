// ref: internal/pluginhost/config_test.go @ a88197f845c979132c8978ea223c6af05cc81536
// Port-Status: adapted_to_ctox
// Port-Note: validates normalized typed config without ambient directory resolution
// License: MIT (upstream); modifications AGPL-3.0-only

use std::collections::BTreeMap;
use std::path::PathBuf;

use crate::internal::config::config_normalization::{PluginInstanceConfig, PluginsConfig};

use super::config::{
    desired_plugin_versions, normalize_version, runtime_config_from_config, ConfigError,
};
