//! Version-awareness helpers: probe a CTOX installation for its running
//! version and compare against the latest GitHub release tag.

use std::path::Path;
use std::process::Command;
use std::time::Duration;

use anyhow::{Context, Result};
use serde::Deserialize;

const DEFAULT_RELEASE_REPO: &str = "metric-space-ai/ctox";
const GITHUB_API_BASE: &str = "https://api.github.com";

/// Latest release metadata pulled from the GitHub API.
#[derive(Debug, Clone)]
pub struct LatestRelease {
    pub tag_name: String,
    pub html_url: Option<String>,
}

#[derive(Debug, Deserialize)]
struct GithubRelease {
    tag_name: String,
    #[serde(default)]
    html_url: Option<String>,
}

#[derive(Debug, Deserialize)]
struct CliVersionResponse {
    version: String,
    #[serde(default)]
    current_release: Option<String>,
}

/// Blocking GitHub API call. Caller should run this off the UI thread (e.g.
/// std::thread::spawn) and cache the result for at least a few minutes —
/// GitHub rate-limits anonymous clients at 60 requests/hour.
pub fn fetch_latest_release() -> Result<LatestRelease> {
    let url = format!("{GITHUB_API_BASE}/repos/{DEFAULT_RELEASE_REPO}/releases/latest");
    let agent = reqwest::blocking::Client::builder()
        .timeout(Duration::from_secs(10))
        .user_agent(concat!("ctox-desktop/", env!("CTOX_BUILD_VERSION")))
        .build()
        .context("failed to build http client")?;
    let response = agent
        .get(&url)
        .header("accept", "application/vnd.github+json")
        .send()
        .with_context(|| format!("GET {url}"))?
        .error_for_status()
        .context("github release API returned error status")?;
    let parsed: GithubRelease = response.json().context("parse github release json")?;
    Ok(LatestRelease {
        tag_name: parsed.tag_name,
        html_url: parsed.html_url,
    })
}

/// Probe a locally installed `ctox` binary for its installed release by running
/// `<binary> version`. Managed installs report both the binary crate version
/// and the release manifest version; the release manifest is what the desktop
/// must display.
pub fn probe_local_version(binary: &Path) -> Result<String> {
    let output = Command::new(binary)
        .arg("version")
        .output()
        .with_context(|| format!("spawn {} version", binary.display()))?;
    if !output.status.success() {
        anyhow::bail!(
            "`{} version` exited non-zero: {}",
            binary.display(),
            String::from_utf8_lossy(&output.stderr).trim()
        );
    }
    let stdout = String::from_utf8_lossy(&output.stdout);
    let version = parse_version_output(&stdout)
        .ok_or_else(|| anyhow::anyhow!("{} version produced no output", binary.display()))?;
    if version.is_empty() {
        anyhow::bail!("{} version produced no output", binary.display());
    }
    Ok(version)
}

/// Probe a remote host over SSH for its installed CTOX release.
pub fn probe_remote_version(user: &str, host: &str, port: u16, password: &str) -> Result<String> {
    if password.is_empty() {
        anyhow::bail!("remote ssh password is empty");
    }
    // PATH extension covers ~/.local/bin (default install destination) in case
    // the user's non-interactive ssh session ships a minimal PATH.
    let remote_cmd = "PATH=\"$HOME/.local/bin:$PATH\" ctox version 2>&1";
    let output = Command::new("sshpass")
        .arg("-p")
        .arg(password)
        .arg("ssh")
        .arg("-o")
        .arg("StrictHostKeyChecking=no")
        .arg("-o")
        .arg("ConnectTimeout=6")
        .arg("-p")
        .arg(port.to_string())
        .arg(format!("{user}@{host}"))
        .arg(remote_cmd)
        .output()
        .context("spawn ssh for remote ctox version")?;
    if !output.status.success() {
        anyhow::bail!(
            "remote ssh exited non-zero: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        );
    }
    let stdout = String::from_utf8_lossy(&output.stdout);
    let version = parse_version_output(&stdout)
        .ok_or_else(|| anyhow::anyhow!("remote `ctox version` produced no output"))?;
    if version.is_empty() {
        anyhow::bail!("remote `ctox version` produced no output");
    }
    Ok(version)
}

fn parse_version_output(stdout: &str) -> Option<String> {
    serde_json::from_str::<CliVersionResponse>(stdout)
        .ok()
        .map(|payload| {
            payload
                .current_release
                .map(|value| value.trim().to_owned())
                .filter(|value| !value.is_empty())
                .unwrap_or(payload.version)
        })
        .or_else(|| {
            stdout
                .lines()
                .find(|line| !line.trim().is_empty())
                .map(|line| line.trim().to_owned())
        })
}

/// Normalize a version string (`"ctox 0.1.0"`, `"v0.1.0"`, `"0.1.0"`) to a
/// comparable tag form (`"v0.1.0"`).
pub fn normalize_tag(raw: &str) -> String {
    let trimmed = raw.trim();
    let stripped = trimmed
        .strip_prefix("ctox ")
        .or_else(|| trimmed.strip_prefix("ctox-"))
        .unwrap_or(trimmed);
    if stripped.starts_with('v') {
        stripped.to_owned()
    } else {
        format!("v{stripped}")
    }
}

/// Convenience — is `installed_raw` older than `latest_tag`? This is a naive
/// equality-based check (any difference = "update available") rather than a
/// full semver compare, which is what most managed-release setups actually
/// want anyway.
pub fn update_available(installed_raw: &str, latest_tag: &str) -> bool {
    let installed = normalize_tag(installed_raw);
    let latest = normalize_tag(latest_tag);
    !installed.is_empty() && installed != latest
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_pretty_json_version_output() {
        let stdout = "{\n  \"version\": \"0.3.2\"\n}\n";
        assert_eq!(parse_version_output(stdout).as_deref(), Some("0.3.2"));
    }

    #[test]
    fn prefers_managed_current_release_over_crate_version() {
        let stdout = r#"{
  "version": "0.1.0",
  "install_mode": "managed",
  "current_release": "v0.3.19"
}"#;
        assert_eq!(parse_version_output(stdout).as_deref(), Some("v0.3.19"));
    }

    #[test]
    fn falls_back_to_plain_text_output() {
        let stdout = "0.3.2\nextra\n";
        assert_eq!(parse_version_output(stdout).as_deref(), Some("0.3.2"));
    }
}
