// ref: internal/config/plugin_path.go @ a88197f845c979132c8978ea223c6af05cc81536
// Port-Status: adapted_to_ctox
// License: MIT (upstream); modifications AGPL-3.0-only

use std::path::{Component, Path, PathBuf};

pub const DEFAULT_PLUGINS_DIR: &str = "plugins";

/// Resolves plugin paths without consulting HOME or process environment.
/// The host supplies its typed data root for the upstream `~` shorthand.
pub fn resolve_plugins_dir(raw: &str, data_root: &Path) -> Result<PathBuf, String> {
    let value = raw.trim();
    let value = if value.is_empty() {
        DEFAULT_PLUGINS_DIR
    } else {
        value
    };
    let path = if let Some(rest) = value.strip_prefix('~') {
        if data_root.as_os_str().is_empty() {
            return Err("resolve plugins directory: data root is empty".into());
        }
        data_root.join(rest.trim_start_matches(['/', '\\']).replace('\\', "/"))
    } else {
        PathBuf::from(value)
    };
    Ok(clean_path(&path))
}

fn clean_path(path: &Path) -> PathBuf {
    let mut out = PathBuf::new();
    for component in path.components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => {
                out.pop();
            }
            other => out.push(other.as_os_str()),
        }
    }
    if out.as_os_str().is_empty() {
        PathBuf::from(".")
    } else {
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;
}
