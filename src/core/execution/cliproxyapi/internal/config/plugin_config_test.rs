// ref: internal/config/plugin_config_test.go @ a88197f845c979132c8978ea223c6af05cc81536
// Port-Status: adapted_to_ctox
// License: MIT (upstream); modifications AGPL-3.0-only

use std::path::Path;

use super::config_load::{load_config, FileConfigDocument};
use super::config_yaml::save_config_preserve_comments;
use super::parse::{parse_provider_compat_config, parse_provider_compat_config_with_root};
