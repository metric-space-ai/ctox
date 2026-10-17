// ref: internal/watcher/diff/model_hash_test.go @ a88197f845c979132c8978ea223c6af05cc81536
// Port-Status: adapted_to_ctox
// License: MIT (upstream); modifications AGPL-3.0-only

use super::model_hash::{compute_excluded_models_hash, compute_models_hash};
use crate::internal::watcher::config_reload::ModelRoute;
