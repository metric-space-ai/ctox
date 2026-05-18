use anyhow::Context;
use anyhow::Result;
use base64::engine::general_purpose::STANDARD as BASE64_STANDARD;
use base64::Engine as _;
use serde::Deserialize;
use serde::Serialize;
use serde_json::json;
use serde_json::Value;
use std::collections::BTreeMap;
use std::path::Path;
use std::time::Duration;

use crate::mission::ticket_protocol::TicketCommentWritebackRequest;
use crate::mission::ticket_protocol::TicketEventRecord;
use crate::mission::ticket_protocol::TicketMirrorRecord;
use crate::mission::ticket_protocol::TicketSelfWorkAssignRequest;
use crate::mission::ticket_protocol::TicketSelfWorkAssignResult;
use crate::mission::ticket_protocol::TicketSelfWorkNoteRequest;
use crate::mission::ticket_protocol::TicketSelfWorkPublishRequest;
use crate::mission::ticket_protocol::TicketSelfWorkPublishResult;
use crate::mission::ticket_protocol::TicketSelfWorkTransitionRequest;
use crate::mission::ticket_protocol::TicketSyncBatch;
use crate::mission::ticket_protocol::TicketTransitionWritebackRequest;
use crate::mission::ticket_protocol::TicketWritebackResult;

const DEFAULT_PAGE_SIZE: usize = 50;
const DEFAULT_TIMEOUT_SECS: u64 = 20;
const DEFAULT_ARTICLE_TYPE: &str = "note";

#[derive(Debug, Clone)]
struct ZammadConfig {
    base_url: String,
    auth: ZammadAuth,
    timeout_secs: u64,
    page_size: usize,
    article_type: String,
    comment_internal: bool,
    self_work_group: Option<String>,
    self_work_customer: Option<String>,
    self_work_priority: Option<String>,
}

#[derive(Debug, Clone)]
enum ZammadAuth {
    Token(String),
    Basic { user: String, password: String },
}

#[derive(Debug, Clone, Deserialize, Serialize)]
struct ZammadTicketRecord {
    id: i64,
    number: Option<String>,
    title: String,
    state: Option<String>,
    state_id: Option<i64>,
    priority: Option<String>,
    priority_id: Option<i64>,
    group: Option<String>,
    group_id: Option<i64>,
    customer_id: Option<i64>,
    article_ids: Option<Vec<i64>>,
    created_at: String,
    updated_at: String,
    #[serde(flatten)]
    extra: BTreeMap<String, Value>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
struct ZammadArticleRecord {
    id: i64,
    ticket_id: i64,
    subject: Option<String>,
    body: String,
    internal: Option<bool>,
    sender: Option<String>,
    r#type: Option<String>,
    created_at: String,
    updated_at: Option<String>,
    #[serde(flatten)]
    extra: BTreeMap<String, Value>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
struct ZammadNamedLookupRecord {
    id: i64,
    name: String,
    #[serde(flatten)]
    extra: BTreeMap<String, Value>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
struct ZammadCreatedTicketRecord {
    id: i64,
    number: Option<String>,
    title: String,
    created_at: Option<String>,
    updated_at: Option<String>,
    #[serde(flatten)]
    extra: BTreeMap<String, Value>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
struct ZammadUserRecord {
    id: i64,
    email: Option<String>,
    login: Option<String>,
    firstname: Option<String>,
    lastname: Option<String>,
    #[serde(flatten)]
    extra: BTreeMap<String, Value>,
}

#[derive(Debug, Clone, Default)]
struct ZammadLookups {
    states: BTreeMap<i64, String>,
    priorities: BTreeMap<i64, String>,
    groups: BTreeMap<i64, String>,
}

pub(crate) fn fetch_sync_batch(
    root: &Path,
    settings: &BTreeMap<String, String>,
) -> Result<TicketSyncBatch> {
    let config = config_from_settings(settings)?;
    let lookups = fetch_lookups(&config);
    let mut page = 1usize;
    let mut fetched_count = 0usize;
    let mut mirror_records = Vec::new();
    let mut event_records = Vec::new();

    loop {
        let tickets_page = fetch_tickets_page(&config, page, config.page_size)?;
        if tickets_page.is_empty() {
            break;
        }
        fetched_count += tickets_page.len();
        for ticket in tickets_page {
            let articles = fetch_articles_for_ticket(&config, ticket.id)?;
            let body_text = articles
                .first()
                .map(|article| article.body.trim().to_string())
                .filter(|value| !value.is_empty())
                .unwrap_or_default();
            let remote_status = ticket
                .state
                .as_deref()
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(ToOwned::to_owned)
                .or_else(|| {
                    ticket
                        .state_id
                        .and_then(|value| lookups.states.get(&value).cloned())
                })
                .or_else(|| ticket.state_id.map(|value| format!("state:{value}")))
                .unwrap_or_else(|| "unknown".to_string());
            let priority = ticket
                .priority
                .as_deref()
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(ToOwned::to_owned)
                .or_else(|| {
                    ticket
                        .priority_id
                        .and_then(|value| lookups.priorities.get(&value).cloned())
                })
                .or_else(|| ticket.priority_id.map(|value| format!("priority:{value}")));
            let requester = ticket.customer_id.map(|value| value.to_string());
            let ticket_metadata = enrich_ticket_metadata(&ticket, &lookups)?;
            mirror_records.push(TicketMirrorRecord {
                remote_ticket_id: ticket.id.to_string(),
                title: ticket.title.clone(),
                body_text,
                remote_status,
                priority,
                requester,
                metadata: ticket_metadata,
                external_created_at: ticket.created_at.clone(),
                external_updated_at: ticket.updated_at.clone(),
            });

            let mut ordered_articles = articles;
            ordered_articles.sort_by(|left, right| left.created_at.cmp(&right.created_at));
            for article in ordered_articles {
                let article_type = article
                    .r#type
                    .clone()
                    .unwrap_or_else(|| "article".to_string());
                let summary = article
                    .subject
                    .as_deref()
                    .map(str::trim)
                    .filter(|value| !value.is_empty())
                    .unwrap_or(article_type.as_str())
                    .to_string();
                event_records.push(TicketEventRecord {
                    remote_ticket_id: ticket.id.to_string(),
                    remote_event_id: article.id.to_string(),
                    direction: "inbound".to_string(),
                    event_type: article_type,
                    summary,
                    body_text: article.body.clone(),
                    metadata: serde_json::to_value(&article)?,
                    external_created_at: article.created_at.clone(),
                });
            }
        }
        if fetched_count == 0 || fetched_count % config.page_size != 0 || page > 500 {
            break;
        }
        page += 1;
    }

    let _ = root;
    Ok(TicketSyncBatch {
        system: "zammad".to_string(),
        fetched_ticket_count: fetched_count,
        tickets: mirror_records,
        events: event_records,
        metadata: json!({"base_url": config.base_url}),
    })
}

pub(crate) fn test(_root: &Path, settings: &BTreeMap<String, String>) -> Result<Value> {
    let config = config_from_settings(settings)?;
    let sample = fetch_tickets_page(&config, 1, 1)?;
    Ok(json!({
        "ok": true,
        "system": "zammad",
        "base_url": config.base_url,
        "sample_ticket_count": sample.len(),
        "page_size": config.page_size,
    }))
}

pub(crate) fn writeback_comment(
    _root: &Path,
    settings: &BTreeMap<String, String>,
    request: &TicketCommentWritebackRequest<'_>,
) -> Result<TicketWritebackResult> {
    let config = config_from_settings(settings)?;
    let article = create_article(
        &config,
        request.remote_ticket_id,
        request.body.trim(),
        request.internal || config.comment_internal,
    )?;
    Ok(TicketWritebackResult {
        remote_event_ids: vec![article.id.to_string()],
    })
}

pub(crate) fn writeback_transition(
    _root: &Path,
    settings: &BTreeMap<String, String>,
    request: &TicketTransitionWritebackRequest<'_>,
) -> Result<TicketWritebackResult> {
    let config = config_from_settings(settings)?;
    update_ticket_state(&config, request.remote_ticket_id, request.state)?;
    let mut remote_event_ids = Vec::new();
    if let Some(body) =
        render_optional_zammad_article_body(request.note_body, request.control_note.as_ref())?
    {
        let article = create_article(
            &config,
            request.remote_ticket_id,
            &body,
            request.internal_note || config.comment_internal,
        )?;
        remote_event_ids.push(article.id.to_string());
    }
    Ok(TicketWritebackResult { remote_event_ids })
}

pub(crate) fn publish_self_work_item(
    _root: &Path,
    settings: &BTreeMap<String, String>,
    request: &TicketSelfWorkPublishRequest<'_>,
) -> Result<TicketSelfWorkPublishResult> {
    let config = config_from_settings(settings)?;
    let record = create_internal_ticket(&config, request.title, request.body)?;
    Ok(TicketSelfWorkPublishResult {
        remote_ticket_id: Some(record.id.to_string()),
        remote_locator: Some(format!("{}/#ticket/zoom/{}", config.base_url, record.id)),
    })
}

pub(crate) fn assign_self_work_item(
    _root: &Path,
    settings: &BTreeMap<String, String>,
    request: &TicketSelfWorkAssignRequest<'_>,
) -> Result<TicketSelfWorkAssignResult> {
    let config = config_from_settings(settings)?;
    let owner_id = resolve_owner_id(&config, request.assignee)?;
    let _response = request_json(
        &config,
        "PUT",
        &format!("/api/v1/tickets/{}", request.remote_ticket_id.trim()),
        Some(&json!({ "owner_id": owner_id })),
    )?;
    Ok(TicketSelfWorkAssignResult {
        remote_assignee: Some(owner_id.to_string()),
        remote_event_ids: Vec::new(),
    })
}

pub(crate) fn append_self_work_note(
    _root: &Path,
    settings: &BTreeMap<String, String>,
    request: &TicketSelfWorkNoteRequest<'_>,
) -> Result<TicketWritebackResult> {
    let config = config_from_settings(settings)?;
    let article = create_article(
        &config,
        request.remote_ticket_id,
        request.body.trim(),
        request.internal || config.comment_internal,
    )?;
    Ok(TicketWritebackResult {
        remote_event_ids: vec![article.id.to_string()],
    })
}

pub(crate) fn transition_self_work_item(
    _root: &Path,
    settings: &BTreeMap<String, String>,
    request: &TicketSelfWorkTransitionRequest<'_>,
) -> Result<TicketWritebackResult> {
    let config = config_from_settings(settings)?;
    update_ticket_state(&config, request.remote_ticket_id, request.state)?;
    let mut remote_event_ids = Vec::new();
    if let Some(body) = request
        .note_body
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        let article = create_article(
            &config,
            request.remote_ticket_id,
            body,
            request.internal_note || config.comment_internal,
        )?;
        remote_event_ids.push(article.id.to_string());
    }
    Ok(TicketWritebackResult { remote_event_ids })
}

fn config_from_settings(settings: &BTreeMap<String, String>) -> Result<ZammadConfig> {
    let base_url = settings
        .get("CTO_ZAMMAD_BASE_URL")
        .map(String::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .context("missing CTO_ZAMMAD_BASE_URL")?
        .trim_end_matches('/')
        .to_string();
    let token = settings
        .get("CTO_ZAMMAD_TOKEN")
        .map(String::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned);
    let basic_user = settings
        .get("CTO_ZAMMAD_USER")
        .map(String::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned);
    let basic_password = settings
        .get("CTO_ZAMMAD_PASSWORD")
        .map(String::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned);
    let auth = if let Some(token) = token {
        ZammadAuth::Token(token)
    } else if let (Some(user), Some(password)) = (basic_user, basic_password) {
        ZammadAuth::Basic { user, password }
    } else {
        anyhow::bail!(
            "missing Zammad auth: set CTO_ZAMMAD_TOKEN or CTO_ZAMMAD_USER + CTO_ZAMMAD_PASSWORD"
        );
    };
    let timeout_secs = settings
        .get("CTO_ZAMMAD_HTTP_TIMEOUT_SECS")
        .and_then(|value| value.parse::<u64>().ok())
        .filter(|value| *value > 0)
        .unwrap_or(DEFAULT_TIMEOUT_SECS);
    let page_size = settings
        .get("CTO_ZAMMAD_PAGE_SIZE")
        .and_then(|value| value.parse::<usize>().ok())
        .filter(|value| *value > 0)
        .unwrap_or(DEFAULT_PAGE_SIZE);
    let article_type = settings
        .get("CTO_ZAMMAD_ARTICLE_TYPE")
        .map(String::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or(DEFAULT_ARTICLE_TYPE)
        .to_string();
    let comment_internal = settings
        .get("CTO_ZAMMAD_COMMENT_INTERNAL")
        .map(String::as_str)
        .map(str::trim)
        .map(|value| matches!(value, "1" | "true" | "yes"))
        .unwrap_or(false);
    let self_work_group = settings
        .get("CTO_ZAMMAD_SELF_WORK_GROUP")
        .map(String::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned);
    let self_work_customer = settings
        .get("CTO_ZAMMAD_SELF_WORK_CUSTOMER")
        .map(String::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned);
    let self_work_priority = settings
        .get("CTO_ZAMMAD_SELF_WORK_PRIORITY")
        .map(String::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned);
    Ok(ZammadConfig {
        base_url,
        auth,
        timeout_secs,
        page_size,
        article_type,
        comment_internal,
        self_work_group,
        self_work_customer,
        self_work_priority,
    })
}

fn fetch_tickets_page(
    config: &ZammadConfig,
    page: usize,
    per_page: usize,
) -> Result<Vec<ZammadTicketRecord>> {
    let response = request_json(
        config,
        "GET",
        &format!("/api/v1/tickets?page={page}&per_page={per_page}"),
        None,
    )?;
    serde_json::from_value(response).context("failed to decode Zammad ticket page")
}

fn fetch_lookups(config: &ZammadConfig) -> ZammadLookups {
    ZammadLookups {
        states: try_fetch_lookup_map(config, "/api/v1/ticket_states"),
        priorities: try_fetch_lookup_map(config, "/api/v1/ticket_priorities"),
        groups: try_fetch_lookup_map(config, "/api/v1/groups"),
    }
}

fn try_fetch_lookup_map(config: &ZammadConfig, path: &str) -> BTreeMap<i64, String> {
    request_json(config, "GET", path, None)
        .ok()
        .and_then(|response| serde_json::from_value::<Vec<ZammadNamedLookupRecord>>(response).ok())
        .map(|items| {
            items
                .into_iter()
                .map(|item| (item.id, item.name))
                .collect::<BTreeMap<_, _>>()
        })
        .unwrap_or_default()
}

fn fetch_articles_for_ticket(
    config: &ZammadConfig,
    ticket_id: i64,
) -> Result<Vec<ZammadArticleRecord>> {
    let response = request_json(
        config,
        "GET",
        &format!("/api/v1/ticket_articles/by_ticket/{ticket_id}"),
        None,
    )?;
    serde_json::from_value(response).context("failed to decode Zammad ticket articles")
}

fn create_article(
    config: &ZammadConfig,
    remote_ticket_id: &str,
    body: &str,
    internal: bool,
) -> Result<ZammadArticleRecord> {
    let ticket_id = remote_ticket_id
        .trim()
        .parse::<i64>()
        .with_context(|| format!("invalid Zammad ticket id: {remote_ticket_id}"))?;
    let response = request_json(
        config,
        "POST",
        "/api/v1/ticket_articles",
        Some(&json!({
            "ticket_id": ticket_id,
            "subject": "CTOX update",
            "body": body.trim(),
            "content_type": "text/plain",
            "type": config.article_type,
            "internal": internal,
            "sender": "Agent",
        })),
    )?;
    serde_json::from_value(response).context("failed to decode created Zammad article")
}

fn create_internal_ticket(
    config: &ZammadConfig,
    title: &str,
    body: &str,
) -> Result<ZammadCreatedTicketRecord> {
    let group = config
        .self_work_group
        .as_deref()
        .context("missing CTO_ZAMMAD_SELF_WORK_GROUP for self-work publishing")?;
    let customer = config
        .self_work_customer
        .as_deref()
        .context("missing CTO_ZAMMAD_SELF_WORK_CUSTOMER for self-work publishing")?;
    let mut payload = json!({
        "title": title.trim(),
        "group": group,
        "customer": customer,
        "article": {
            "subject": title.trim(),
            "body": body.trim(),
            "content_type": "text/plain",
            "type": config.article_type,
            "internal": true,
            "sender": "Agent",
        }
    });
    if let Some(priority) = config.self_work_priority.as_deref() {
        payload["priority"] = Value::String(priority.to_string());
    }
    let response = request_json(config, "POST", "/api/v1/tickets", Some(&payload))?;
    serde_json::from_value(response).context("failed to decode created Zammad ticket")
}

fn resolve_owner_id(config: &ZammadConfig, assignee: &str) -> Result<i64> {
    let trimmed = assignee.trim();
    if let Ok(id) = trimmed.parse::<i64>() {
        return Ok(id);
    }
    let me = fetch_current_user(config)?;
    if trimmed.is_empty()
        || matches!(trimmed, "self" | "me" | "ctox")
        || me.email.as_deref() == Some(trimmed)
        || me.login.as_deref() == Some(trimmed)
    {
        return Ok(me.id);
    }
    Ok(me.id)
}

fn fetch_current_user(config: &ZammadConfig) -> Result<ZammadUserRecord> {
    let response = request_json(config, "GET", "/api/v1/users/me", None)?;
    serde_json::from_value(response).context("failed to decode current Zammad user")
}

fn enrich_ticket_metadata(ticket: &ZammadTicketRecord, lookups: &ZammadLookups) -> Result<Value> {
    let mut metadata = serde_json::to_value(ticket)?;
    if let Some(object) = metadata.as_object_mut() {
        if let Some(group_name) = ticket
            .group
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(ToOwned::to_owned)
            .or_else(|| {
                ticket
                    .group_id
                    .and_then(|value| lookups.groups.get(&value).cloned())
            })
        {
            object.insert("group_name".to_string(), Value::String(group_name));
        }
        if let Some(state_name) = ticket
            .state
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(ToOwned::to_owned)
            .or_else(|| {
                ticket
                    .state_id
                    .and_then(|value| lookups.states.get(&value).cloned())
            })
        {
            object.insert("state_name".to_string(), Value::String(state_name));
        }
        if let Some(priority_name) = ticket
            .priority
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(ToOwned::to_owned)
            .or_else(|| {
                ticket
                    .priority_id
                    .and_then(|value| lookups.priorities.get(&value).cloned())
            })
        {
            object.insert("priority_name".to_string(), Value::String(priority_name));
        }
    }
    Ok(metadata)
}

fn render_optional_zammad_article_body(
    body: Option<&str>,
    _control_note: Option<&crate::mission::ticket_protocol::TicketControlNote>,
) -> Result<Option<String>> {
    let rendered = body.unwrap_or_default().trim().to_string();
    if rendered.trim().is_empty() {
        Ok(None)
    } else {
        Ok(Some(rendered))
    }
}

fn update_ticket_state(config: &ZammadConfig, remote_ticket_id: &str, state: &str) -> Result<()> {
    let _response = request_json(
        config,
        "PUT",
        &format!("/api/v1/tickets/{}", remote_ticket_id.trim()),
        Some(&json!({
            "state": state.trim(),
        })),
    )?;
    Ok(())
}

fn request_json(
    config: &ZammadConfig,
    method: &str,
    path: &str,
    body: Option<&Value>,
) -> Result<Value> {
    let agent = ureq::AgentBuilder::new()
        .timeout(Duration::from_secs(config.timeout_secs))
        .build();
    let url = format!("{}{}", config.base_url, path);
    let mut request = agent
        .request(method, &url)
        .set("accept", "application/json");
    request = match &config.auth {
        ZammadAuth::Token(token) => request.set("authorization", &format!("Token token={token}")),
        ZammadAuth::Basic { user, password } => {
            let creds = BASE64_STANDARD.encode(format!("{user}:{password}"));
            request.set("authorization", &format!("Basic {creds}"))
        }
    };
    let response = if let Some(body) = body {
        let payload = serde_json::to_string(body)?;
        request = request.set("content-type", "application/json");
        match request.send_string(&payload) {
            Ok(response) => response,
            Err(ureq::Error::Status(_, response)) => response,
            Err(ureq::Error::Transport(error)) => {
                anyhow::bail!("Zammad request to {url} failed: {error}");
            }
        }
    } else {
        match request.call() {
            Ok(response) => response,
            Err(ureq::Error::Status(_, response)) => response,
            Err(ureq::Error::Transport(error)) => {
                anyhow::bail!("Zammad request to {url} failed: {error}");
            }
        }
    };
    let status = response.status();
    let body_text = response
        .into_string()
        .context("failed to read Zammad response body")?;
    if !(200..300).contains(&status) {
        anyhow::bail!(
            "Zammad request {} {} failed with {}: {}",
            method,
            url,
            status,
            body_text
        );
    }
    if body_text.trim().is_empty() {
        Ok(json!({}))
    } else {
        serde_json::from_str(&body_text)
            .with_context(|| format!("failed to parse Zammad JSON response from {url}"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{Read, Write};
    use std::net::TcpListener;
    use std::thread;

    fn spawn_mock_server() -> Result<(String, std::sync::mpsc::Receiver<String>)> {
        let listener = TcpListener::bind("127.0.0.1:0")?;
        let address = format!("http://{}", listener.local_addr()?);
        let (tx, rx) = std::sync::mpsc::channel::<String>();
        thread::spawn(move || {
            for _ in 0..20 {
                let Ok((mut stream, _addr)) = listener.accept() else {
                    break;
                };
                let mut buffer = Vec::new();
                let mut chunk = [0u8; 4096];
                let mut header_end = None;
                let mut expected_total = None;
                loop {
                    let Ok(size) = stream.read(&mut chunk) else {
                        break;
                    };
                    if size == 0 {
                        break;
                    }
                    buffer.extend_from_slice(&chunk[..size]);
                    if header_end.is_none() {
                        header_end = buffer
                            .windows(4)
                            .position(|window| window == b"\r\n\r\n")
                            .map(|pos| pos + 4);
                        if let Some(end) = header_end {
                            let headers = String::from_utf8_lossy(&buffer[..end]);
                            let content_length = headers
                                .lines()
                                .find_map(|line| {
                                    let (name, value) = line.split_once(':')?;
                                    if name.trim().eq_ignore_ascii_case("content-length") {
                                        value.trim().parse::<usize>().ok()
                                    } else {
                                        None
                                    }
                                })
                                .unwrap_or(0);
                            expected_total = Some(end + content_length);
                        }
                    }
                    if let Some(total) = expected_total {
                        if buffer.len() >= total {
                            break;
                        }
                    }
                }
                let request = String::from_utf8_lossy(&buffer).to_string();
                let first_line = request.lines().next().unwrap_or_default().to_string();
                let _ = tx.send(first_line.clone());
                let response_body = if first_line
                    .starts_with("GET /api/v1/tickets?page=1&per_page=1")
                {
                    r#"[{"id":1,"number":"20001","title":"VPN issue","state_id":2,"priority_id":2,"group_id":1,"customer_id":42,"article_ids":[10],"created_at":"2026-04-09T12:00:00Z","updated_at":"2026-04-09T12:10:00Z"}]"#.to_string()
                } else if first_line.starts_with("GET /api/v1/ticket_states") {
                    r#"[{"id":1,"name":"new"},{"id":2,"name":"open"},{"id":4,"name":"closed"}]"#
                        .to_string()
                } else if first_line.starts_with("GET /api/v1/ticket_priorities") {
                    r#"[{"id":2,"name":"2 normal"}]"#.to_string()
                } else if first_line.starts_with("GET /api/v1/groups") {
                    r#"[{"id":1,"name":"Users"}]"#.to_string()
                } else if first_line.starts_with("GET /api/v1/users/me") {
                    r#"{"id":42,"email":"ctox@example.test","login":"ctox"}"#.to_string()
                } else if first_line.starts_with("GET /api/v1/ticket_articles/by_ticket/1") {
                    r#"[{"id":10,"ticket_id":1,"subject":"VPN issue","body":"Users cannot connect","internal":false,"sender":"Customer","type":"email","created_at":"2026-04-09T12:00:00Z"}]"#.to_string()
                } else if first_line.starts_with("POST /api/v1/ticket_articles") {
                    r#"{"id":99,"ticket_id":1,"subject":"CTOX update","body":"handled","internal":true,"sender":"Agent","type":"note","created_at":"2026-04-09T12:30:00Z"}"#.to_string()
                } else if first_line.starts_with("POST /api/v1/tickets") {
                    r#"{"id":55,"number":"30055","title":"CTOX self work","created_at":"2026-04-09T12:20:00Z","updated_at":"2026-04-09T12:20:00Z"}"#.to_string()
                } else if first_line.starts_with("PUT /api/v1/tickets/55") {
                    r#"{"id":55,"title":"CTOX self work","state":"open","updated_at":"2026-04-09T12:45:00Z","created_at":"2026-04-09T12:20:00Z"}"#.to_string()
                } else if first_line.starts_with("PUT /api/v1/tickets/1") {
                    r#"{"id":1,"title":"VPN issue","state":"closed","updated_at":"2026-04-09T12:40:00Z","created_at":"2026-04-09T12:00:00Z"}"#.to_string()
                } else {
                    "[]".to_string()
                };
                let response = format!(
                    "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                    response_body.len(),
                    response_body
                );
                let _ = stream.write_all(response.as_bytes());
                let _ = stream.flush();
            }
        });
        Ok((address, rx))
    }

    fn settings_with_base_url(base_url: &str) -> BTreeMap<String, String> {
        BTreeMap::from([
            ("CTO_ZAMMAD_BASE_URL".to_string(), base_url.to_string()),
            ("CTO_ZAMMAD_TOKEN".to_string(), "token-123".to_string()),
            ("CTO_ZAMMAD_PAGE_SIZE".to_string(), "1".to_string()),
            (
                "CTO_ZAMMAD_COMMENT_INTERNAL".to_string(),
                "true".to_string(),
            ),
            (
                "CTO_ZAMMAD_SELF_WORK_GROUP".to_string(),
                "Users".to_string(),
            ),
            (
                "CTO_ZAMMAD_SELF_WORK_CUSTOMER".to_string(),
                "helpdesk.admin@example.com".to_string(),
            ),
        ])
    }

    fn basic_auth_settings_with_base_url(base_url: &str) -> BTreeMap<String, String> {
        BTreeMap::from([
            ("CTO_ZAMMAD_BASE_URL".to_string(), base_url.to_string()),
            (
                "CTO_ZAMMAD_USER".to_string(),
                "helpdesk.admin@example.com".to_string(),
            ),
            ("CTO_ZAMMAD_PASSWORD".to_string(), "testpass".to_string()),
            ("CTO_ZAMMAD_PAGE_SIZE".to_string(), "1".to_string()),
        ])
    }

    #[test]
    fn zammad_sync_and_writebacks_use_expected_endpoints() -> Result<()> {
        let (base_url, rx) = spawn_mock_server()?;
        let settings = settings_with_base_url(&base_url);
        let root = std::env::temp_dir().join("ctox-zammad-adapter-test");
        std::fs::create_dir_all(&root)?;

        let sync = fetch_sync_batch(&root, &settings)?;
        assert_eq!(sync.system, "zammad");
        assert_eq!(sync.fetched_ticket_count, 1);
        assert_eq!(sync.tickets.len(), 1);
        assert_eq!(sync.events.len(), 1);
        assert_eq!(sync.tickets[0].remote_status, "open");
        assert_eq!(sync.tickets[0].priority.as_deref(), Some("2 normal"));
        assert_eq!(
            sync.tickets[0]
                .metadata
                .get("group_name")
                .and_then(Value::as_str),
            Some("Users")
        );

        let comment = writeback_comment(
            &root,
            &settings,
            &TicketCommentWritebackRequest {
                remote_ticket_id: "1",
                body: "handled",
                internal: true,
            },
        )?;
        assert_eq!(comment.remote_event_ids, vec!["99".to_string()]);

        let transition = writeback_transition(
            &root,
            &settings,
            &TicketTransitionWritebackRequest {
                remote_ticket_id: "1",
                state: "closed",
                note_body: Some("handled"),
                internal_note: true,
                control_note: None,
            },
        )?;
        assert_eq!(transition.remote_event_ids, vec!["99".to_string()]);

        let published = publish_self_work_item(
            &root,
            &settings,
            &TicketSelfWorkPublishRequest {
                title: "CTOX self work",
                body: "review the observed source profile",
            },
        )?;
        assert_eq!(published.remote_ticket_id.as_deref(), Some("55"));

        let assignment = assign_self_work_item(
            &root,
            &settings,
            &TicketSelfWorkAssignRequest {
                remote_ticket_id: "55",
                assignee: "ctox",
            },
        )?;
        assert_eq!(assignment.remote_assignee.as_deref(), Some("42"));

        let note = append_self_work_note(
            &root,
            &settings,
            &TicketSelfWorkNoteRequest {
                remote_ticket_id: "55",
                body: "Working through the first onboarding pass.",
                internal: true,
            },
        )?;
        assert_eq!(note.remote_event_ids, vec!["99".to_string()]);

        let self_work_transition = transition_self_work_item(
            &root,
            &settings,
            &TicketSelfWorkTransitionRequest {
                remote_ticket_id: "55",
                state: "closed",
                note_body: Some("Onboarding pass completed."),
                internal_note: true,
            },
        )?;
        assert_eq!(
            self_work_transition.remote_event_ids,
            vec!["99".to_string()]
        );

        let requests = (0..20)
            .filter_map(|_| rx.recv_timeout(Duration::from_secs(2)).ok())
            .collect::<Vec<_>>();
        assert!(requests
            .iter()
            .any(|line| line.starts_with("GET /api/v1/tickets?page=1&per_page=1")));
        assert!(requests
            .iter()
            .any(|line| line.starts_with("GET /api/v1/ticket_states")));
        assert!(requests
            .iter()
            .any(|line| line.starts_with("GET /api/v1/ticket_priorities")));
        assert!(requests
            .iter()
            .any(|line| line.starts_with("GET /api/v1/groups")));
        assert!(requests
            .iter()
            .any(|line| line.starts_with("GET /api/v1/users/me")));
        assert!(requests
            .iter()
            .any(|line| line.starts_with("GET /api/v1/ticket_articles/by_ticket/1")));
        assert!(requests
            .iter()
            .any(|line| line.starts_with("POST /api/v1/ticket_articles")));
        assert!(requests
            .iter()
            .any(|line| line.starts_with("POST /api/v1/tickets")));
        assert!(requests
            .iter()
            .any(|line| line.starts_with("PUT /api/v1/tickets/1")));
        assert!(requests
            .iter()
            .any(|line| line.starts_with("PUT /api/v1/tickets/55")));

        let _ = std::fs::remove_dir_all(&root);
        Ok(())
    }

    #[test]
    fn zammad_basic_auth_settings_are_accepted() -> Result<()> {
        let (base_url, _rx) = spawn_mock_server()?;
        let settings = basic_auth_settings_with_base_url(&base_url);
        let config = config_from_settings(&settings)?;
        match config.auth {
            ZammadAuth::Basic { user, password } => {
                assert_eq!(user, "helpdesk.admin@example.com");
                assert_eq!(password, "testpass");
            }
            other => panic!("unexpected auth mode: {other:?}"),
        }
        Ok(())
    }
}
