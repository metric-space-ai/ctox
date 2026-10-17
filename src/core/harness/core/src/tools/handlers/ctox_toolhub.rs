//! `ctox_toolhub_*` harness tools (RFC `docs/rfcs/toolhub-adapter-builder.md`, P2).
//!
//! Thin wrappers over `ctox toolhub ...`: the CLI runs in core (workers reach
//! it through the daemon relay), so no outbound HTTP happens in the harness.
//! The role is fixed by session config (`tools.ctox_toolhub_role`), never by
//! model arguments. Role rules share one source with the core builder.

use async_trait::async_trait;
use serde_json::Value;
use std::collections::BTreeMap;

use crate::client_common::tools::ResponsesApiTool;
use crate::client_common::tools::ToolSpec;
use crate::function_tool::FunctionCallError;
use crate::tools::context::FunctionToolOutput;
use crate::tools::context::ToolInvocation;
use crate::tools::context::ToolPayload;
use crate::tools::handlers::ctox_web::business_os_command_session_from_turn;
use crate::tools::handlers::ctox_web::resolve_ctox_binary;
use crate::tools::registry::ToolHandler;
use crate::tools::registry::ToolKind;
use crate::tools::spec::JsonSchema;

const ROLE_RULES: &str = include_str!("../../../../../execution/toolhub/prompts/role_rules.md");

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum ToolhubRole {
    Mcp,
    OneShot,
    Worker,
    Supervisor,
}

impl ToolhubRole {
    /// Unknown values fall back to the least privileged harness role.
    pub fn resolve(value: Option<&str>) -> Self {
        match value.map(|v| v.trim().to_ascii_lowercase()).as_deref() {
            Some("supervisor") => Self::Supervisor,
            Some("worker" | "persistent" | "persistent_worker") => Self::Worker,
            _ => Self::OneShot,
        }
    }
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Supervisor => "supervisor",
            Self::Worker => "worker",
            Self::OneShot => "oneshot",
            Self::Mcp => "mcp",
        }
    }
}

/// The `## role:<name>` section of the shared rules file.
pub fn ctox_toolhub_role_rules(role: ToolhubRole) -> String {
    let marker = format!("## role:{}", role.as_str());
    let Some(start) = ROLE_RULES.find(&marker) else { return String::new() };
    let rest = &ROLE_RULES[start + marker.len()..];
    let end = rest.find("\n## role:").unwrap_or(rest.len());
    format!("# CTOX ToolHub ({})\n{}", role.as_str(), rest[..end].trim())
}

struct Def {
    name: &'static str,
    min_role: ToolhubRole,
    description: &'static str,
    /// (name, description, required)
    params: &'static [(&'static str, &'static str, bool)],
}

const DEFS: &[Def] = &[
    Def { name: "ctox_toolhub_search", min_role: ToolhubRole::OneShot, description: "Search active ToolHub tools (and optionally the vetted catalog) by intent.", params: &[("query", "What you want to do, e.g. 'look up postal code'.", true), ("include_catalog", "Also list catalog adapters that are not active yet ('true').", false)] },
    Def { name: "ctox_toolhub_get_schema", min_role: ToolhubRole::OneShot, description: "Input schema, risk and auth of one active tool.", params: &[("tool", "Tool slug.", true)] },
    Def { name: "ctox_toolhub_list", min_role: ToolhubRole::OneShot, description: "List adapters, versions, review and approval state.", params: &[] },
    Def { name: "ctox_toolhub_execute", min_role: ToolhubRole::OneShot, description: "Execute an active tool. Writes and destructive calls follow your role rules; destructive calls return approval_required with a gate id first.", params: &[("tool", "Tool slug.", true), ("args_json", "Arguments as a JSON object string.", false), ("account", "Connected account id, if several.", false), ("approval_gate", "Decided per-call gate id for a destructive call.", false)] },
    Def { name: "ctox_toolhub_draft_spec", min_role: ToolhubRole::OneShot, description: "Store a draft adapter spec (ctox.adapter-spec/v1), copy one from the vetted catalog, or generate one from an OpenAPI/Swagger document (actions, risk by method, auth suggestion, host allowlist, read probes from examples). Drafts are inactive.", params: &[("spec_json", "Full spec as JSON string.", false), ("from_catalog", "Catalog slug instead of spec_json.", false), ("from_openapi", "https URL (supervisor/worker) or file path of an OpenAPI 3 / Swagger 2 document.", false), ("slug", "Toolkit slug for from_openapi (default from the title).", false), ("operations", "Comma-separated operationIds to keep (from_openapi).", false), ("base_url", "Server override for from_openapi (relative or wrong servers).", false), ("version", "Spec version for from_openapi (default 1.0.0; versions are immutable, re-drafts need a new one).", false)] },
    Def { name: "ctox_toolhub_validate", min_role: ToolhubRole::OneShot, description: "Validate a draft: schema, hosts, risk rules.", params: &[("slug", "Adapter slug.", true), ("version", "Version.", true)] },
    Def { name: "ctox_toolhub_live_test", min_role: ToolhubRole::OneShot, description: "Run probes against the real API and record the review proof. Write probes need cleanup; destructive actions are never probed.", params: &[("slug", "Adapter slug.", true), ("version", "Version.", true), ("probes_json", "JSON array of {tool,args,cleanup[],expect{kind,pointers,min_items,allow_empty},from{arg:\"TOOL#/pointer\"}}; omit to use the probes drafted by from_openapi. Empty results fail unless expect.allow_empty.", false), ("account", "Connected account id.", false), ("mission", "Builder mission work id (workers: the proof is bound to it).", false)] },
    Def { name: "ctox_toolhub_request_adapter", min_role: ToolhubRole::Worker, description: "Queue a builder mission (adapter-builder skill) for a service that has no adapter yet. One open mission per service.", params: &[("service", "Service name, e.g. pipedrive.", true), ("reason", "Why the task needs it.", false)] },
    Def { name: "ctox_toolhub_approve", min_role: ToolhubRole::Supervisor, description: "Record the user's decision on a per-call gate of a destructive action. Only with the user's own words in user_quote.", params: &[("gate", "Gate id.", true), ("decision", "approve | reject", true), ("user_quote", "The user's literal answer.", true)] },
    Def { name: "ctox_toolhub_activate", min_role: ToolhubRole::Worker, description: "Activate a version autonomously once validate passed and a live test passed within 24 h for exactly this version. Workers: only inside a builder mission (pass its work id as mission).", params: &[("slug", "Adapter slug.", true), ("version", "Version.", true), ("mission", "Builder mission work id (required for workers).", false)] },
    Def { name: "ctox_toolhub_deactivate", min_role: ToolhubRole::Supervisor, description: "Deactivate an adapter.", params: &[("slug", "Adapter slug.", true)] },
    Def { name: "ctox_toolhub_rollback", min_role: ToolhubRole::Supervisor, description: "Roll back to the previous approved version.", params: &[("slug", "Adapter slug.", true)] },
];

#[cfg(test)]
fn tool_names(role: ToolhubRole) -> Vec<&'static str> {
    DEFS.iter().filter(|d| role >= d.min_role).map(|d| d.name).collect()
}

pub fn ctox_toolhub_create_tools(role: ToolhubRole) -> Vec<ToolSpec> {
    DEFS.iter()
        .filter(|d| role >= d.min_role)
        .map(|d| {
            let properties: BTreeMap<String, JsonSchema> = d
                .params
                .iter()
                .map(|(n, desc, _)| (n.to_string(), JsonSchema::String { description: Some(desc.to_string()) }))
                .collect();
            let required: Vec<String> = d.params.iter().filter(|p| p.2).map(|p| p.0.to_string()).collect();
            ToolSpec::Function(ResponsesApiTool {
                name: d.name.to_string(),
                description: d.description.to_string(),
                strict: false,
                defer_loading: None,
                parameters: JsonSchema::Object { properties, required: Some(required), additional_properties: Some(false.into()) },
                output_schema: None,
            })
        })
        .collect()
}

fn s(args: &Value, key: &str) -> Option<String> {
    match args.get(key)? {
        Value::String(v) if !v.trim().is_empty() => Some(v.clone()),
        Value::Null => None,
        Value::String(_) => None,
        other => Some(other.to_string()),
    }
}

fn need(args: &Value, key: &str) -> Result<String, String> {
    s(args, key).ok_or_else(|| format!("missing argument `{key}`"))
}

/// argv after `ctox`. The role comes from config, not from `args`.
pub fn build_argv(tool: &str, args: &Value, role: ToolhubRole, session: Option<&str>) -> Result<Vec<String>, String> {
    let def = DEFS.iter().find(|d| d.name == tool).ok_or_else(|| format!("unknown toolhub tool {tool}"))?;
    if role < def.min_role {
        return Err(format!("{tool} is not available for role {}", role.as_str()));
    }
    let mut v: Vec<String> = vec!["toolhub".into(), "--as".into(), role.as_str().into()];
    if let Some(token) = session {
        v.extend(["--command-session".into(), token.into()]);
    }
    let mut opt = |v: &mut Vec<String>, flag: &str, key: &str| {
        if let Some(x) = s(args, key) {
            v.push(flag.into());
            v.push(x);
        }
    };
    match tool {
        "ctox_toolhub_search" => {
            v.extend(["search".into(), need(args, "query")?]);
            if s(args, "include_catalog").as_deref() == Some("true") {
                v.push("--include-catalog".into());
            }
        }
        "ctox_toolhub_get_schema" => v.extend(["get-schema".into(), need(args, "tool")?]),
        "ctox_toolhub_list" => v.push("list".into()),
        "ctox_toolhub_execute" => {
            v.extend(["execute".into(), need(args, "tool")?]);
            opt(&mut v, "--args-json", "args_json");
            opt(&mut v, "--account", "account");
            opt(&mut v, "--approval-gate", "approval_gate");
        }
        "ctox_toolhub_draft_spec" => {
            v.push("draft-spec".into());
            if let Some(c) = s(args, "from_catalog") {
                v.extend(["--from-catalog".into(), c]);
            } else if let Some(o) = s(args, "from_openapi") {
                v.extend(["--from-openapi".into(), o]);
                opt(&mut v, "--slug", "slug");
                opt(&mut v, "--operations", "operations");
                opt(&mut v, "--base-url", "base_url");
                opt(&mut v, "--version", "version");
            } else {
                v.extend(["--spec-json".into(), need(args, "spec_json")?]);
            }
        }
        "ctox_toolhub_validate" => v.extend(["validate".into(), need(args, "slug")?, need(args, "version")?]),
        "ctox_toolhub_live_test" => {
            v.extend(["live-test".into(), need(args, "slug")?, need(args, "version")?]);
            opt(&mut v, "--probes-json", "probes_json");
            opt(&mut v, "--account", "account");
            opt(&mut v, "--mission", "mission");
        }
        "ctox_toolhub_request_adapter" => {
            v.extend(["request-adapter".into(), need(args, "service")?]);
            opt(&mut v, "--reason", "reason");
        }
        "ctox_toolhub_approve" => v.extend([
            "approve".into(), "--gate".into(), need(args, "gate")?, "--decision".into(), need(args, "decision")?, "--user-quote".into(), need(args, "user_quote")?,
        ]),
        "ctox_toolhub_activate" => {
            v.extend(["activate".into(), need(args, "slug")?, need(args, "version")?]);
            opt(&mut v, "--mission", "mission");
        }
        "ctox_toolhub_deactivate" => v.extend(["deactivate".into(), need(args, "slug")?]),
        "ctox_toolhub_rollback" => v.extend(["rollback".into(), need(args, "slug")?]),
        _ => unreachable!(),
    }
    Ok(v)
}

pub struct CtoxToolhubHandler;

#[async_trait]
impl ToolHandler for CtoxToolhubHandler {
    type Output = FunctionToolOutput;

    fn kind(&self) -> ToolKind {
        ToolKind::Function
    }

    async fn is_mutating(&self, invocation: &ToolInvocation) -> bool {
        !matches!(invocation.tool_name.as_str(), "ctox_toolhub_search" | "ctox_toolhub_get_schema" | "ctox_toolhub_list" | "ctox_toolhub_validate")
    }

    async fn handle(&self, invocation: ToolInvocation) -> Result<Self::Output, FunctionCallError> {
        let ToolInvocation { turn, tool_name, payload, .. } = invocation;
        let ToolPayload::Function { arguments } = payload else {
            return Err(FunctionCallError::RespondToModel("ctox toolhub handler received unsupported payload".into()));
        };
        let args: Value = if arguments.trim().is_empty() {
            Value::Object(Default::default())
        } else {
            serde_json::from_str(&arguments).map_err(|e| FunctionCallError::RespondToModel(format!("invalid arguments: {e}")))?
        };
        let role = ToolhubRole::resolve(turn.tools_config.ctox_toolhub_role.as_deref());
        let argv = build_argv(&tool_name, &args, role, business_os_command_session_from_turn(&turn))
            .map_err(FunctionCallError::RespondToModel)?;
        let output = tokio::process::Command::new(resolve_ctox_binary()?)
            .args(&argv)
            .output()
            .await
            .map_err(|e| FunctionCallError::RespondToModel(format!("failed to run ctox: {e}")))?;
        let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
            let detail = if stderr.is_empty() { stdout } else { stderr };
            return Err(FunctionCallError::RespondToModel(format!("ctox {tool_name} failed: {detail}")));
        }
        Ok(FunctionToolOutput::from_text(stdout, Some(true)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn tools_follow_role() {
        assert_eq!(tool_names(ToolhubRole::OneShot).len(), 7);
        assert!(tool_names(ToolhubRole::Worker).contains(&"ctox_toolhub_activate"));
        assert!(!tool_names(ToolhubRole::OneShot).contains(&"ctox_toolhub_activate"));
        assert!(!tool_names(ToolhubRole::Worker).contains(&"ctox_toolhub_approve"));
        assert_eq!(tool_names(ToolhubRole::Supervisor).len(), 12);
        assert_eq!(ctox_toolhub_create_tools(ToolhubRole::Supervisor).len(), 12);
    }

    #[test]
    fn role_comes_from_config_not_args() {
        let argv = build_argv("ctox_toolhub_execute", &json!({"tool": "X", "args_json": "{}", "as": "supervisor"}), ToolhubRole::OneShot, None).unwrap();
        assert_eq!(&argv[..3], &["toolhub", "--as", "oneshot"]);
        assert!(build_argv("ctox_toolhub_activate", &json!({"slug": "a", "version": "1"}), ToolhubRole::OneShot, None).is_err());
        let argv = build_argv("ctox_toolhub_activate", &json!({"slug": "a", "version": "1", "mission": "w1"}), ToolhubRole::Worker, None).unwrap();
        assert_eq!(&argv[3..], &["activate", "a", "1", "--mission", "w1"]);
        let argv = build_argv("ctox_toolhub_live_test", &json!({"slug": "a", "version": "1", "mission": "w1"}), ToolhubRole::Worker, None).unwrap();
        assert_eq!(&argv[3..], &["live-test", "a", "1", "--mission", "w1"]);
        let argv = build_argv("ctox_toolhub_draft_spec", &json!({"from_openapi": "https://x.org/o.yaml", "slug": "x"}), ToolhubRole::Worker, None).unwrap();
        assert_eq!(&argv[3..], &["draft-spec", "--from-openapi", "https://x.org/o.yaml", "--slug", "x"]);
        let argv = build_argv("ctox_toolhub_draft_spec", &json!({"from_openapi": "/d/o.json", "base_url": "https://x.org/v4", "version": "1.0.1"}), ToolhubRole::Worker, None).unwrap();
        assert_eq!(&argv[3..], &["draft-spec", "--from-openapi", "/d/o.json", "--base-url", "https://x.org/v4", "--version", "1.0.1"]);
        assert_eq!(ToolhubRole::resolve(Some("root")), ToolhubRole::OneShot);
        let argv = build_argv("ctox_toolhub_approve", &json!({"gate": "g", "decision": "approve", "user_quote": "ja"}), ToolhubRole::Supervisor, Some("tok")).unwrap();
        assert_eq!(argv, ["toolhub", "--as", "supervisor", "--command-session", "tok", "approve", "--gate", "g", "--decision", "approve", "--user-quote", "ja"]);
    }

    #[test]
    fn rules_share_one_source() {
        assert!(ctox_toolhub_role_rules(ToolhubRole::Supervisor).contains("no user approval to install or activate"));
        assert!(ctox_toolhub_role_rules(ToolhubRole::OneShot).contains("Zugang fehlt"));
        assert!(!ctox_toolhub_role_rules(ToolhubRole::Worker).contains("## role:"));
    }
}
