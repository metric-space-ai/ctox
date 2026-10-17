// ref: internal/watcher/synthesizer/file_test.go @ a88197f845c979132c8978ea223c6af05cc81536
// Port-Status: adapted_to_ctox
// License: MIT (upstream); modifications AGPL-3.0-only

use super::context::SynthesisContext;
use super::file::{synthesize_auth_file, FileSynthesizer};
use super::interface::AuthSynthesizer;
use crate::internal::watcher::config_reload::WatcherConfig;
use crate::internal::watcher::NativeWatchFilesystem;
use std::sync::Arc;
