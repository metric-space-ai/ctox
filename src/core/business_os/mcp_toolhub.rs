// Origin: CTOX
// License: AGPL-3.0-only
//! Read-only ToolHub discovery for external MCP clients (RFC
//! toolhub-adapter-builder P2): search, schema and adapter list. External MCP
//! never executes, drafts or approves; it gets the `mcp` role rules.
use super::*;
use crate::execution::toolhub_cli::{CtoxPolicy, CtoxSecrets};
use ctox_toolhub::builder::{roles::AgentRole, ApprovalGates, Builder, GateRequest};
use ctox_toolhub::http::{HttpRequest, HttpResponse, HttpTransport};
use serde_json::json;
use std::sync::Arc;

pub(super) const SEARCH_TOOL: &str = "business_os.toolhub_search";
pub(super) const SCHEMA_TOOL: &str = "business_os.toolhub_get_schema";
pub(super) const LIST_TOOL: &str = "business_os.toolhub_list";

struct NoGates;
impl ApprovalGates for NoGates {
    fn open(&self, _: &GateRequest) -> anyhow::Result<String> {
        anyhow::bail!("read_only: external MCP cannot open approval gates")
    }
    fn state(&self, _: &str) -> anyhow::Result<Option<String>> {
        Ok(None)
    }
    fn decide(&self, _: &str, _: bool) -> anyhow::Result<()> {
        anyhow::bail!("read_only: external MCP cannot decide approval gates")
    }
}

struct NoNetwork;
impl HttpTransport for NoNetwork {
    fn send(&self, _: HttpRequest) -> anyhow::Result<HttpResponse> {
        anyhow::bail!("read_only: external MCP has no outbound ToolHub calls")
    }
}

pub(super) fn descriptors() -> Vec<BusinessOsMcpToolDescriptor> {
    vec![
        read_tool(SEARCH_TOOL,
            "Search active CTOX ToolHub tools by intent. Read-only: returns tool slugs, risk and descriptions, never credentials. Executing, drafting and approving adapters happen inside CTOX only.",
            json!({"type":"object","additionalProperties":false,"required":["query"],"properties":{"query":{"type":"string"},"limit":{"type":"integer","minimum":1,"maximum":50}}})),
        read_tool(SCHEMA_TOOL,
            "Input schema, risk and auth type of one active CTOX ToolHub tool. Read-only.",
            json!({"type":"object","additionalProperties":false,"required":["tool"],"properties":{"tool":{"type":"string"}}})),
        read_tool(LIST_TOOL,
            "List ToolHub adapters with versions, review and approval state. Read-only.",
            json!({"type":"object","additionalProperties":false,"properties":{}})),
    ]
}

pub(super) fn execute(root: &Path, context: &McpChannelRequestContext, tool: &str, args: &Value) -> anyhow::Result<Value> {
    let role = context.trusted_role.as_deref().unwrap_or("user");
    let policy = CtoxPolicy::new(&context.actor, role);
    let b = Builder::new(root, Arc::new(CtoxSecrets::new(root)), Arc::new(NoNetwork), &NoGates, &policy, AgentRole::Mcp, &context.actor);
    match tool {
        SEARCH_TOOL => {
            let query = args.get("query").and_then(Value::as_str).context("missing query")?;
            let limit = args.get("limit").and_then(Value::as_u64).unwrap_or(20).clamp(1, 50) as usize;
            b.search(query, false, limit)
        }
        SCHEMA_TOOL => b.get_schema(args.get("tool").and_then(Value::as_str).context("missing tool")?),
        LIST_TOOL => b.list(),
        other => anyhow::bail!("unknown toolhub MCP tool {other}"),
    }
}
