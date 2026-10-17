use anyhow::Result;
pub use ctox_protocol::protocol::SandboxPolicy;

pub fn parse_policy(value: &str) -> Result<SandboxPolicy> {
    match value {
        "read-only" => Ok(SandboxPolicy::new_read_only_policy()),
        "workspace-write" => Ok(SandboxPolicy::new_workspace_write_policy()),
        "danger-full-access" | "external-sandbox" => anyhow::bail!(
            "DangerFullAccess and ExternalSandbox are not supported for sandboxing"
        ),
        other => {
            let parsed: SandboxPolicy = serde_json::from_str(other)?;
            if matches!(
                parsed,
                SandboxPolicy::DangerFullAccess | SandboxPolicy::ExternalSandbox { .. }
            ) {
                anyhow::bail!(
                    "DangerFullAccess and ExternalSandbox are not supported for sandboxing"
                );
            }
            Ok(parsed)
        }
    }
}
