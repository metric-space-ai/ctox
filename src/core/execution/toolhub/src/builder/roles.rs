//! Harness roles for the adapter-builder tools and their permission matrix
//! (`docs/rfcs/toolhub-adapter-builder.md` §3). The matrix is enforced by the
//! `ctox toolhub` CLI; the harness only hides tools.
use serde::Serialize;

use crate::spec::validate::Risk;

/// Single source of the role-specific prompt rules (also included by the harness).
pub const ROLE_RULES: &str = include_str!("../../prompts/role_rules.md");

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum AgentRole {
    Supervisor,
    Worker,
    OneShot,
    Mcp,
}

impl AgentRole {
    pub fn parse(s: &str) -> anyhow::Result<Self> {
        Ok(match s.trim().to_ascii_lowercase().as_str() {
            "supervisor" => Self::Supervisor,
            "worker" | "persistent" | "persistent_worker" => Self::Worker,
            "oneshot" | "one-shot" | "one_shot" => Self::OneShot,
            "mcp" | "external_mcp" => Self::Mcp,
            o => anyhow::bail!("unknown toolhub role `{o}` (supervisor|worker|oneshot|mcp)"),
        })
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

/// Section `## role:<name>` of [`ROLE_RULES`].
pub fn role_rules(role: AgentRole) -> String {
    section(ROLE_RULES, role.as_str())
}

/// Shared parser so the harness and the crate cut the same text.
pub fn section(source: &str, role: &str) -> String {
    let head = format!("## role:{role}");
    let mut out = Vec::new();
    let mut on = false;
    for line in source.lines() {
        if line.starts_with("## role:") {
            on = line.trim() == head;
            continue;
        }
        if on {
            out.push(line);
        }
    }
    out.join("\n").trim().to_string()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Tool {
    Search,
    GetSchema,
    List,
    Execute,
    DraftSpec,
    Validate,
    LiveTest,
    Submit,
    Approve,
    Activate,
    Deactivate,
    Rollback,
}

/// Why a role may not use a tool, with what to do instead.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Denied {
    pub code: &'static str,
    pub message: String,
}

impl std::fmt::Display for Denied {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}: {}", self.code, self.message)
    }
}
impl std::error::Error for Denied {}

fn deny(code: &'static str, message: impl Into<String>) -> Result<(), Denied> {
    Err(Denied { code, message: message.into() })
}

/// Permission matrix (RFC §3). `risk` is the action risk for `Execute`, and the
/// highest probe risk for `LiveTest`.
pub fn allows(role: AgentRole, tool: Tool, risk: Option<Risk>) -> Result<(), Denied> {
    use AgentRole::*;
    use Tool::*;
    let risk = risk.unwrap_or(Risk::Read);
    match (tool, role) {
        (Search | GetSchema | List, _) => Ok(()),
        (Execute, _) if risk == Risk::Read => Ok(()),
        (LiveTest, Supervisor | Worker | OneShot) if risk == Risk::Read => Ok(()),
        (Execute, Supervisor) => Ok(()),
        (Execute, Worker) => deny("delegate_to_supervisor", "write/destructive actions run through the supervisor"),
        (Execute | LiveTest, OneShot) => deny("not_allowed", "one-shot workers run read actions only"),
        (_, Mcp) => deny("read_only", "external MCP is read-only"),
        (DraftSpec | Validate, _) => Ok(()),
        (LiveTest, Supervisor) => Ok(()),
        (LiveTest, Worker) => deny("delegate_to_supervisor", "write probes need the supervisor"),
        (Submit, Supervisor | Worker) => Ok(()),
        (Submit, OneShot) => deny("not_allowed", "one-shot workers return drafts to whoever started them"),
        (Approve | Activate | Deactivate | Rollback, Supervisor) => Ok(()),
        (Approve | Activate | Deactivate | Rollback, Worker) => deny("delegate_to_supervisor", "only the supervisor may change active adapters"),
        (Approve | Activate | Deactivate | Rollback, OneShot) => deny("not_allowed", "only the supervisor may change active adapters"),
    }
}

/// Role-specific answer when a call needs a connection that does not exist.
pub fn access_missing(role: AgentRole, toolkit: &str) -> (&'static str, String) {
    match role {
        AgentRole::OneShot => ("access_missing_end_run", format!("Zugang fehlt / access missing: {toolkit}. End this run and report it.")),
        AgentRole::Worker => ("access_missing_delegate", format!("Access to {toolkit} is missing. Delegate to the supervisor, who asks the user.")),
        AgentRole::Supervisor => ("access_missing_ask_user", format!("Access to {toolkit} is missing. Ask the user in the supervisor chat and store it via ctox.secret.put.")),
        AgentRole::Mcp => ("access_missing", format!("Access to {toolkit} is missing.")),
    }
}
