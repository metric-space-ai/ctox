//! Toolkit adapters. Each adapter declares its catalog entry (data) and maps
//! tool calls to provider HTTP requests. Auth headers are injected by the hub.
use anyhow::Result;
use std::collections::BTreeMap;
use serde_json::Value;

use crate::http::{HttpRequest, HttpResponse};
use crate::model::ToolkitSpec;

pub mod common;
pub mod github;
pub mod gmail;
pub mod slack;
pub mod google_sheets;
pub mod confluence;
pub mod calendly;
pub mod sharepoint;
pub mod twilio;
pub mod brevo;
pub mod onedrive;
pub mod todoist;
pub mod freshdesk;
pub mod whatsapp;
pub mod mailchimp;
pub mod monday;
pub mod stripe;
pub mod typeform;
pub mod zoom;
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
    /// Extra per-connection inputs (instance URL, subdomain, shop, data center).
    fn connection_fields(&self) -> Vec<crate::model::ConnectionField> {
        Vec::new()
    }
    /// Validate connection params and return the per-connection API base that
    /// replaces `default_api_base()` in built request URLs. `Ok(None)` = no rewrite.
    fn connection_api_base(&self, params: &BTreeMap<String, String>) -> Result<Option<String>> {
        let _ = params;
        Ok(None)
    }
    /// The API base `build_request` uses when no connection params apply.
    fn default_api_base(&self) -> Option<String> {
        None
    }
    /// Tool arguments filled from connection params when the caller omitted them.
    fn connection_args(&self, params: &BTreeMap<String, String>) -> Vec<(String, String)> {
        let _ = params;
        Vec::new()
    }
    /// Params derivable without asking (API-key suffix, OAuth token response).
    fn derive_connection_params(
        &self,
        credential: Option<&str>,
        token_extra: &BTreeMap<String, String>,
    ) -> BTreeMap<String, String> {
        let _ = (credential, token_extra);
        BTreeMap::new()
    }
    /// Outbound restrictions; `Some` for runtime (spec) adapters. The hub then
    /// checks every request URL and sends through the transport's restricted
    /// path (no redirects, no private addresses).
    fn egress_policy(&self) -> Option<&crate::spec::EgressPolicy> {
        None
    }
}

/// Built-in toolkits shipped with CTOX.
pub fn builtin() -> Vec<Box<dyn ToolkitAdapter>> {
    vec![
        Box::new(github::GitHub::default()),
        Box::new(gmail::Gmail::default()),
        Box::new(slack::Slack::default()),
        Box::new(google_sheets::GoogleSheets::default()),
        Box::new(confluence::Confluence::default()),
        Box::new(calendly::Calendly::default()),
        Box::new(sharepoint::SharePoint::default()),
        Box::new(twilio::Twilio::default()),
        Box::new(brevo::Brevo::default()),
        Box::new(onedrive::OneDrive::default()),
        Box::new(todoist::Todoist::default()),
        Box::new(freshdesk::Freshdesk::default()),
        Box::new(whatsapp::WhatsApp::default()),
        Box::new(mailchimp::Mailchimp::default()),
        Box::new(monday::Monday::default()),
        Box::new(stripe::Stripe::default()),
        Box::new(typeform::Typeform::default()),
        Box::new(zoom::Zoom::default()),
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
