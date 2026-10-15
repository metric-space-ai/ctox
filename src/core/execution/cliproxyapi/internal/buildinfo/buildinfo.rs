// ref: internal/buildinfo/buildinfo.go @ a88197f845c979132c8978ea223c6af05cc81536
// Port-Status: adapted_to_ctox
// License: MIT (upstream); modifications AGPL-3.0-only

/// Immutable build metadata injected by the owning host at assembly time.
///
/// Go mutates package globals through linker flags. CTOX keeps the same local
/// defaults but passes release metadata explicitly, avoiding mutable global
/// process state inside the gateway library.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BuildInfo {
    pub version: String,
    pub commit: String,
    pub build_date: String,
}

impl BuildInfo {
    pub fn new(
        version: impl Into<String>,
        commit: impl Into<String>,
        build_date: impl Into<String>,
    ) -> Self {
        Self {
            version: version.into(),
            commit: commit.into(),
            build_date: build_date.into(),
        }
    }
}

impl Default for BuildInfo {
    fn default() -> Self {
        Self::new("dev", "none", "unknown")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

}
