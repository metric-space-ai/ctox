// ref: internal/watcher/synthesizer/helpers_test.go @ a88197f845c979132c8978ea223c6af05cc81536
// Port-Status: adapted_to_ctox
// License: MIT (upstream); modifications AGPL-3.0-only

use super::context::SynthesizedAuth;
use super::helpers::{
    add_config_headers_to_attrs, apply_auth_excluded_models_meta, StableIdGenerator,
};
use std::collections::BTreeMap;
