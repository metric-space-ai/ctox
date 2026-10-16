//! Shipped process policies. Apps select a tool name, never an interpreter or
//! host script. Package paths come from the Core's immutable artifact resolver.
use super::*;
use std::{
    collections::BTreeMap,
    path::PathBuf,
    sync::{Arc, OnceLock},
};

#[derive(Clone, Copy, Debug)]
pub struct DefaultTool {
    pub name: &'static str,
    pub app_id: &'static str,
    pub interpreter: &'static str,
    pub host_script: &'static str,
}
pub const DEFAULT_TOOLS: &[DefaultTool] = &[
    DefaultTool {
        name: "appsec-app-audit-validator",
        app_id: "appsec-pentest",
        interpreter: "node",
        host_script: "app-audit-scenarios.mjs",
    },
    DefaultTool {
        name: "olg-scrape-js",
        app_id: "outbound-lead-generation",
        interpreter: "node",
        host_script: "runtime/host.mjs",
    },
    DefaultTool {
        name: "olg-scrape-ts",
        app_id: "outbound-lead-generation",
        interpreter: "tsx",
        host_script: "runtime/host.ts",
    },
];

/// Only the Core composition root may bind this. It resolves a platform tool
/// and a script from the exact installed (app_id, app_ver) artifact, verifying
/// CAS integrity and materializing outside the writable process workspace.
/// Neither path nor environment comes from process.run arguments.
pub trait HostTools: Send + Sync {
    fn resolve(
        &self,
        ctx: &CapCallContext,
        interpreter: &str,
        script: &str,
    ) -> Result<ResolvedHost, CtoxError>;
}
pub struct ResolvedHost {
    pub interpreter: PathBuf,
    pub script: PathBuf,
    pub env: BTreeMap<String, String>,
}
static HOST_TOOLS: OnceLock<Arc<dyn HostTools>> = OnceLock::new();
pub fn bind_host_tools(tools: Arc<dyn HostTools>) -> Result<(), CtoxError> {
    HOST_TOOLS
        .set(tools)
        .map_err(|_| CtoxError::new(ErrorCode::Conflict, "process host tools already bound"))
}
fn unavailable() -> CtoxError {
    CtoxError::new(
        ErrorCode::Unavailable,
        "process tool requires installed host artifacts and E20",
    )
}
fn shipped_shape(value: &Value) -> bool {
    let absolute = |key| {
        value
            .get(key)
            .and_then(Value::as_str)
            .is_some_and(|s| !s.is_empty() && !s.contains('\0') && Path::new(s).is_absolute())
    };
    let name = value
        .get("interpreter")
        .and_then(Value::as_str)
        .and_then(|p| Path::new(p).file_name())
        .and_then(|p| p.to_str())
        .unwrap_or("");
    absolute("interpreter")
        && absolute("script")
        && ![
            "sh", "bash", "dash", "zsh", "ksh", "fish", "ash", "csh", "tcsh",
        ]
        .contains(&name)
        && value.get("program").is_none_or(Value::is_null)
        && value.get("untrusted_code") == Some(&Value::Bool(true))
}
fn selected(
    root: &Path,
    ctx: &CapCallContext,
    name: &str,
    resolver: Option<&dyn HostTools>,
) -> Result<Value, CtoxError> {
    let default = DEFAULT_TOOLS.iter().find(|t| t.name == name);
    if default.is_some_and(|t| t.app_id != ctx.app_id) {
        return Err(forbidden("process tool belongs to another app"));
    }
    // Failed reads must never resurrect a platform-disabled default.
    let config = crate::inference::runtime_env::load_persisted_runtime_env_map_cached(root)
        .map_err(native_error)?;
    let configured = config
        .get(process::ALLOWLIST_KEY)
        .filter(|raw| !raw.trim().is_empty())
        .map(|raw| serde_json::from_str::<BTreeMap<String, Value>>(raw).map_err(|_| unavailable()))
        .transpose()?;
    let value = match configured.as_ref().and_then(|map| map.get(name)) {
        Some(Value::Null) => return Err(forbidden("process tool disabled by platform")),
        Some(value) => value.clone(),
        None => {
            let tool = default.ok_or_else(|| forbidden("program is not allowlisted"))?;
            let host = resolver.ok_or_else(unavailable)?.resolve(
                ctx,
                tool.interpreter,
                tool.host_script,
            )?;
            let interpreter = host.interpreter.to_str().ok_or_else(unavailable)?;
            let script = host.script.to_str().ok_or_else(unavailable)?;
            json!({"interpreter":interpreter, "script":script,
                "env":host.env,"untrusted_code":true})
        }
    };
    if default.is_some() && !shipped_shape(&value) {
        return Err(forbidden(
            "shipped process tool needs a non-shell interpreter and host script",
        ));
    }
    Ok(value)
}
/// Same process executor and E20 gate as configured tools. A pre-E20 handler
/// cannot deserialize untrusted_code and therefore fails closed as unavailable.
pub(super) fn allowlist(
    root: &Path,
    ctx: &CapCallContext,
    name: &str,
) -> Result<BTreeMap<String, process::AllowEntry>, CtoxError> {
    let value = selected(root, ctx, name, HOST_TOOLS.get().map(|r| r.as_ref()))?;
    let entry = serde_json::from_value(value).map_err(|_| unavailable())?;
    Ok(BTreeMap::from([(name.to_owned(), entry)]))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app_capabilities::{Actor, ExecutorFence};
    fn context(app: &str) -> CapCallContext {
        CapCallContext {
            app_id: app.into(),
            app_ver: "1".into(),
            actor: Actor {
                id: "user".into(),
                role: "admin".into(),
            },
            command_id: "cmd".into(),
            call_id: "call".into(),
            fence: ExecutorFence {
                run_id: "run".into(),
                participant: "core".into(),
                fence: 1,
                expires_at_core_ms: u64::MAX,
            },
        }
    }
    fn check_tool(name: &str) {
        let tool = DEFAULT_TOOLS.iter().find(|t| t.name == name).unwrap();
        let root = tempfile::tempdir().unwrap();
        let ctx = context(tool.app_id);
        let run = || {
            process::execute(
                root.path(),
                Err(unavailable()),
                &ctx,
                &json!({"program":tool.name}),
            )
        };
        assert_eq!(
            run().unwrap_err().code,
            ErrorCode::Unavailable,
            "{} unbound",
            tool.name
        );
        assert!(
            !process::workspace(root.path(), &ctx).exists(),
            "no workspace/effect before admission"
        );
        for shape in [
            json!({"interpreter":"/bin/sh","script":"/core/host.mjs","untrusted_code":true}),
            json!({"interpreter":"/usr/bin/node","untrusted_code":true}),
            json!({"program":"/usr/bin/node","untrusted_code":true}),
            json!({"interpreter":"/usr/bin/node","script":"/core/host.mjs","untrusted_code":false}),
        ] {
            crate::inference::runtime_env::set_runtime_env_value(
                root.path(),
                process::ALLOWLIST_KEY,
                &json!({tool.name:shape}).to_string(),
            )
            .unwrap();
            assert_eq!(
                run().unwrap_err().code,
                ErrorCode::Forbidden,
                "{} refused shape",
                tool.name
            );
        }
        crate::inference::runtime_env::set_runtime_env_value(root.path(), process::ALLOWLIST_KEY,
                &json!({tool.name:{"interpreter":"/usr/bin/node","script":"/core/host.mjs","untrusted_code":true}}).to_string()).unwrap();
        assert_eq!(
            run().unwrap_err().code,
            ErrorCode::Unavailable,
            "{} needs E20",
            tool.name
        );
        assert_eq!(
            allowlist(root.path(), &context("other-app"), tool.name)
                .unwrap_err()
                .code,
            ErrorCode::Forbidden
        );
        crate::inference::runtime_env::set_runtime_env_value(
            root.path(),
            process::ALLOWLIST_KEY,
            &json!({tool.name:null}).to_string(),
        )
        .unwrap();
        assert_eq!(
            run().unwrap_err().code,
            ErrorCode::Forbidden,
            "{} disabled",
            tool.name
        );
    }
    #[test]
    fn app_capabilities_cap7_olg_js_policy() {
        check_tool("olg-scrape-js");
    }
    #[test]
    fn app_capabilities_cap7_olg_ts_policy() {
        check_tool("olg-scrape-ts");
    }
    #[test]
    fn app_capabilities_cap7_appsec_validator_policy() {
        check_tool("appsec-app-audit-validator");
    }
}
