//! Toolkit adapters. Each adapter declares its catalog entry (data) and maps
//! tool calls to provider HTTP requests. Auth headers are injected by the hub.
use anyhow::Result;
use serde_json::Value;

use crate::http::{HttpRequest, HttpResponse};
use crate::model::ToolkitSpec;

pub mod common;
pub mod github;
pub mod gmail;
pub mod slack;

pub trait ToolkitAdapter: Send + Sync {
    fn spec(&self) -> &ToolkitSpec;
    /// Build the provider request for `tool` without credentials.
    fn build_request(&self, tool: &str, args: &Value) -> Result<HttpRequest>;
    /// Reduce the provider response to the tool result.
    fn parse_response(&self, tool: &str, response: &HttpResponse) -> Result<Value> {
        let _ = tool;
        response.json()
    }
}

/// Built-in toolkits shipped with CTOX.
pub fn builtin() -> Vec<Box<dyn ToolkitAdapter>> {
    vec![
        Box::new(github::GitHub::default()),
        Box::new(gmail::Gmail::default()),
        Box::new(slack::Slack::default()),
    ]
}

/// URL path segment guard (prevents path/query injection from tool args).
pub fn path_segment<'a>(args: &'a Value, key: &str) -> Result<&'a str> {
    let v = args
        .get(key)
        .and_then(Value::as_str)
        .ok_or_else(|| anyhow::anyhow!("missing argument `{key}`"))?;
    if v.is_empty()
        || v.len() > 200
        || !v
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.'))
        || v == "."
        || v == ".."
    {
        anyhow::bail!("invalid argument `{key}`");
    }
    Ok(v)
}
