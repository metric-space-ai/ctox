//! Platform-owned, version-bound grants; never supplied by process.run args.
use anyhow::{bail, Context, Result};
use std::collections::BTreeMap;

/// app_id -> immutable app version -> platform tool name -> exact DNS names.
type Grants = BTreeMap<String, BTreeMap<String, BTreeMap<String, Vec<String>>>>;
pub const CONFIG_KEY: &str = "business_os.process_network_grants";

pub fn authorize_domains(
    configured: Option<&str>,
    app_id: &str,
    app_ver: &str,
    tool: &str,
    domains: &[String],
) -> Result<()> {
    if domains.is_empty() {
        return Ok(());
    }
    if domains.len() > 16 || domains.iter().any(|d| !super::domain(d)) {
        bail!("invalid network domains");
    }
    let grants: Grants = serde_json::from_str(configured.context("network grant missing")?)
        .context("invalid platform network grant")?;
    let allowed = grants
        .get(app_id)
        .and_then(|v| v.get(app_ver))
        .and_then(|t| t.get(tool))
        .context("network grant missing for app version and tool")?;
    if domains.iter().any(|domain| !allowed.contains(domain)) {
        bail!("network domain is outside the app grant");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn app_version_and_tool_grant_cannot_be_broadened_by_call() {
        let config = r#"{"app":{"v1":{"tool":["example.com"]}}}"#;
        let allowed = vec!["example.com".into()];
        assert!(authorize_domains(Some(config), "app", "v1", "tool", &allowed).is_ok());
        for (app, version, tool) in [
            ("foreign", "v1", "tool"),
            ("app", "v2", "tool"),
            ("app", "v1", "other"),
        ] {
            assert!(authorize_domains(Some(config), app, version, tool, &allowed).is_err());
        }
        assert!(
            authorize_domains(Some(config), "app", "v1", "tool", &["example.org".into()]).is_err()
        );
        assert!(authorize_domains(None, "app", "v1", "tool", &allowed).is_err());
        assert!(authorize_domains(Some("{}"), "app", "v1", "tool", &allowed).is_err());
        assert!(authorize_domains(Some("invalid"), "app", "v1", "tool", &allowed).is_err());
        assert!(authorize_domains(None, "app", "v1", "tool", &[]).is_ok());
    }
}
