// ref: internal/config/config_defaults.go @ a88197f845c979132c8978ea223c6af05cc81536
// Port-Status: ported
// License: MIT (upstream); modifications AGPL-3.0-only

pub const DEFAULT_PANEL_GITHUB_REPOSITORY: &str =
    "https://github.com/router-for-me/Cli-Proxy-API-Management-Center";
pub const DEFAULT_PPROF_ADDR: &str = "127.0.0.1:8316";
pub const DEFAULT_AUTH_DIR: &str = "~/.cli-proxy-api";
pub const DEFAULT_DISCOVERY_SERVICE_TYPE: &str = "_ai-gateway._tcp";

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_are_exact_upstream_literals() {
        assert_eq!(
            DEFAULT_PANEL_GITHUB_REPOSITORY,
            "https://github.com/router-for-me/Cli-Proxy-API-Management-Center"
        );
        assert_eq!(DEFAULT_PPROF_ADDR, "127.0.0.1:8316");
        assert_eq!(DEFAULT_AUTH_DIR, "~/.cli-proxy-api");
        assert_eq!(DEFAULT_DISCOVERY_SERVICE_TYPE, "_ai-gateway._tcp");
    }
}
