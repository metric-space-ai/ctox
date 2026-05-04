// Origin: CTOX
// License: Apache-2.0

use std::path::{Path, PathBuf};

pub(crate) fn channel_dir(root: &Path, channel: &str) -> PathBuf {
    root.join("runtime").join("communication").join(channel)
}

pub(crate) fn state_dir(root: &Path, channel: &str) -> PathBuf {
    channel_dir(root, channel).join("state")
}

pub(crate) fn raw_dir(root: &Path, channel: &str) -> PathBuf {
    channel_dir(root, channel).join("raw")
}

pub(crate) fn artifacts_dir(root: &Path, channel: &str) -> PathBuf {
    channel_dir(root, channel).join("artifacts")
}

pub(crate) fn state_file(root: &Path, channel: &str, file_name: &str) -> PathBuf {
    state_dir(root, channel).join(file_name)
}

pub(crate) fn resolve_configured_path(
    root: &Path,
    configured: Option<&str>,
    default_path: PathBuf,
) -> PathBuf {
    configured
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(|value| {
            let path = PathBuf::from(value);
            if path.is_absolute() {
                path
            } else {
                root.join(path)
            }
        })
        .unwrap_or(default_path)
}

pub(crate) fn migration_aware_state_file(
    root: &Path,
    channel: &str,
    file_name: &str,
    legacy_paths: &[PathBuf],
) -> PathBuf {
    let canonical = state_file(root, channel, file_name);
    if canonical.exists() {
        return canonical;
    }
    legacy_paths
        .iter()
        .find(|path| path.exists())
        .cloned()
        .unwrap_or(canonical)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn channel_runtime_paths_are_namespaced_by_adapter() {
        let root = Path::new("/tmp/ctox");
        assert_eq!(
            raw_dir(root, "email"),
            PathBuf::from("/tmp/ctox/runtime/communication/email/raw")
        );
        assert_eq!(
            state_file(root, "whatsapp", "device.sqlite3"),
            PathBuf::from("/tmp/ctox/runtime/communication/whatsapp/state/device.sqlite3")
        );
        assert_eq!(
            artifacts_dir(root, "meeting"),
            PathBuf::from("/tmp/ctox/runtime/communication/meeting/artifacts")
        );
    }
}
