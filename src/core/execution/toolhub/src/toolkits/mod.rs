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
pub mod intercom;
pub mod pipedrive;
pub mod trello;
pub mod telegram;
pub mod discord;
pub mod clickup;
pub mod gitlab;
pub mod dropbox;
pub mod zendesk;
pub mod shopify;
pub mod airtable;
pub mod asana;
pub mod salesforce;
pub mod google_calendar;
pub mod google_drive;
pub mod hubspot;
pub mod jira;
pub mod linear;
pub mod notion;
pub mod outlook;
pub mod teams;

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
        Box::new(intercom::Intercom::default()),
        Box::new(pipedrive::Pipedrive::default()),
        Box::new(trello::Trello::default()),
        Box::new(telegram::Telegram::default()),
        Box::new(discord::Discord::default()),
        Box::new(clickup::ClickUp::default()),
        Box::new(gitlab::GitLab::default()),
        Box::new(dropbox::Dropbox::default()),
        Box::new(zendesk::Zendesk::default()),
        Box::new(shopify::Shopify::default()),
        Box::new(airtable::Airtable::default()),
        Box::new(asana::Asana::default()),
        Box::new(salesforce::Salesforce::default()),
        Box::new(google_calendar::GoogleCalendar::default()),
        Box::new(google_drive::GoogleDrive::default()),
        Box::new(hubspot::HubSpot::default()),
        Box::new(jira::Jira::default()),
        Box::new(linear::Linear::default()),
        Box::new(teams::Teams::default()),
        Box::new(outlook::Outlook::default()),
        Box::new(notion::Notion::default()),
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
