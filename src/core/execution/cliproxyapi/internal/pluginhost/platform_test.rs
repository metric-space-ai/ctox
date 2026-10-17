// ref: internal/pluginhost/platform_test.go @ a88197f845c979132c8978ea223c6af05cc81536
// Port-Status: adapted_to_ctox
// Port-Note: process-executable discovery uses an injected filesystem
// License: MIT (upstream); modifications AGPL-3.0-only

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use super::platform::{
    cleanup_unselected_plugin_files, discover_plugin_files, plugin_file_from_name,
    validate_plugin_id, PlatformError, PluginDiscoveryFilesystem, PluginPlatform,
};

#[derive(Default)]
struct Filesystem {
    directories: BTreeMap<PathBuf, Vec<String>>,
    removed: Mutex<Vec<PathBuf>>,
}

impl PluginDiscoveryFilesystem for Filesystem {
    fn regular_file_names(&self, directory: &Path) -> Result<Vec<String>, PlatformError> {
        Ok(self.directories.get(directory).cloned().unwrap_or_default())
    }

    fn remove_file(&self, path: &Path) -> Result<(), PlatformError> {
        self.removed
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .push(path.to_path_buf());
        Ok(())
    }
}
