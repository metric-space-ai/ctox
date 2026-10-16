// ref: internal/watcher/diff/openai_compat_test.go @ a88197f845c979132c8978ea223c6af05cc81536
// Port-Status: adapted_to_ctox
// License: MIT (upstream); modifications AGPL-3.0-only

use super::openai_compat::{diff_openai_compatibility, openai_compat_key, openai_compat_signature};
use crate::internal::watcher::config_reload::{ModelRoute, OpenAiCompatibility};
