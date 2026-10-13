//! Slack toolkit (Web API). Action set modelled on Activepieces `piece-slack` (MIT).
//! Auth: bot/user token (xoxb-/xoxp-) as Bearer, or Slack OAuth v2 (instance-registered app).
use anyhow::{anyhow, bail, Result};
use serde_json::{json, Value};

use super::common::*;
use super::ToolkitAdapter;
use crate::http::{HttpRequest, HttpResponse};
use crate::model::*;

pub const SLUG: &str = "slack";

pub struct Slack {
    spec: ToolkitSpec,
    api_base: String,
}

impl Default for Slack {
    fn default() -> Self {
        Self::with_api_base("https://slack.com/api")
    }
}

/// Slack IDs (channels, users, timestamps) are restricted to a safe charset.
fn slack_id<'a>(args: &'a Value, key: &str) -> Result<&'a str> {
    let v = req_str(args, key)?;
    if v.len() > 64
        || !v
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'.' || b == b'_' || b == b'-')
    {
        bail!("invalid argument `{key}`");
    }
    Ok(v)
}

impl Slack {
    pub fn with_api_base(api_base: &str) -> Self {
        let t = |slug, title, desc, props: Props<'_>, req: &[&str], scopes: &[&str], m| {
            tool(SLUG, slug, title, desc, props, req, scopes, m)
        };
        let spec = ToolkitSpec {
            slug: SLUG.into(),
            name: "Slack".into(),
            description: LocalizedText::new(
                "Kanäle, Verlauf, Nachrichten und Suche in Slack.",
                "Channels, history, messages and search in Slack.",
            ),
            auth_schemes: vec![
                AuthScheme::Bearer,
                AuthScheme::OAuth2(OAuth2Scheme {
                    authorize_url: "https://slack.com/oauth/v2/authorize".into(),
                    token_url: "https://slack.com/api/oauth.v2.access".into(),
                    default_scopes: [
                        "channels:read",
                        "groups:read",
                        "channels:history",
                        "groups:history",
                        "chat:write",
                        "users:read",
                        "users:read.email",
                        "reactions:write",
                    ]
                    .iter()
                    .map(|s| s.to_string())
                    .collect(),
                    pkce: false,
                    scope_separator: ",".into(),
                    refreshable: false,
                    extra_authorize_params: Vec::new(),
                }),
            ],
            tools: vec![
                t(
                    "SLACK_LIST_CHANNELS",
                    ("Kanäle auflisten", "List channels"),
                    (
                        "Listet öffentliche/private Kanäle.",
                        "Lists public/private channels.",
                    ),
                    &[
                        ("types", s("public_channel,private_channel")),
                        ("limit", int("1-1000", 1, 1000, 200)),
                        ("cursor", s("Pagination cursor")),
                        (
                            "exclude_archived",
                            json!({"type": "boolean", "default": true}),
                        ),
                    ],
                    &[],
                    &["channels:read"],
                    false,
                ),
                t(
                    "SLACK_GET_HISTORY",
                    ("Verlauf lesen", "Read history"),
                    (
                        "Liest Nachrichten eines Kanals.",
                        "Reads messages of a channel.",
                    ),
                    &[
                        ("channel", s("Channel ID")),
                        ("limit", int("1-1000", 1, 1000, 50)),
                        ("oldest", s("Unix ts")),
                        ("latest", s("Unix ts")),
                        ("cursor", s("Pagination cursor")),
                    ],
                    &["channel"],
                    &["channels:history"],
                    false,
                ),
                t(
                    "SLACK_GET_THREAD",
                    ("Thread lesen", "Read thread"),
                    ("Liest Antworten eines Threads.", "Reads thread replies."),
                    &[
                        ("channel", s("Channel ID")),
                        ("thread_ts", s("Parent message ts")),
                        ("limit", int("1-1000", 1, 1000, 100)),
                    ],
                    &["channel", "thread_ts"],
                    &["channels:history"],
                    false,
                ),
                t(
                    "SLACK_SEARCH_MESSAGES",
                    ("Nachrichten suchen", "Search messages"),
                    (
                        "Volltextsuche (benötigt User-Token mit search:read).",
                        "Full-text search (needs user token with search:read).",
                    ),
                    &[
                        ("query", s("Slack search query")),
                        ("count", int("1-100", 1, 100, 20)),
                        ("page", int("page", 1, 100, 1)),
                    ],
                    &["query"],
                    &["search:read"],
                    false,
                ),
                t(
                    "SLACK_FIND_USER_BY_EMAIL",
                    ("Nutzer per E-Mail finden", "Find user by email"),
                    (
                        "Sucht ein Slack-Mitglied per E-Mail.",
                        "Looks up a member by email.",
                    ),
                    &[("email", s("E-mail"))],
                    &["email"],
                    &["users:read.email"],
                    false,
                ),
                t(
                    "SLACK_SEND_MESSAGE",
                    ("Nachricht senden", "Send message"),
                    (
                        "Postet eine Nachricht (optional im Thread); braucht --confirm.",
                        "Posts a message (optionally in a thread); requires --confirm.",
                    ),
                    &[
                        ("channel", s("Channel ID")),
                        ("text", s("Message text (mrkdwn)")),
                        ("thread_ts", s("Reply in thread")),
                    ],
                    &["channel", "text"],
                    &["chat:write"],
                    true,
                ),
                t(
                    "SLACK_UPDATE_MESSAGE",
                    ("Nachricht ändern", "Update message"),
                    ("Ändert eine eigene Nachricht.", "Edits an own message."),
                    &[
                        ("channel", s("Channel ID")),
                        ("ts", s("Message ts")),
                        ("text", s("New text")),
                    ],
                    &["channel", "ts", "text"],
                    &["chat:write"],
                    true,
                ),
                t(
                    "SLACK_ADD_REACTION",
                    ("Reaktion hinzufügen", "Add reaction"),
                    ("Fügt ein Emoji hinzu.", "Adds an emoji reaction."),
                    &[
                        ("channel", s("Channel ID")),
                        ("ts", s("Message ts")),
                        ("name", s("Emoji name without colons")),
                    ],
                    &["channel", "ts", "name"],
                    &["reactions:write"],
                    true,
                ),
            ],
        };
        Self {
            spec,
            api_base: api_base.trim_end_matches('/').to_string(),
        }
    }

    fn get(&self, method: &str, q: String) -> HttpRequest {
        HttpRequest::new("GET", format!("{}/{method}{q}", self.api_base))
    }
    fn post(&self, method: &str, body: Value) -> HttpRequest {
        HttpRequest::new("POST", format!("{}/{method}", self.api_base))
            .json(&body)
            .header("content-type", "application/json; charset=utf-8")
    }
}

impl ToolkitAdapter for Slack {
    fn spec(&self) -> &ToolkitSpec {
        &self.spec
    }

    fn build_request(&self, tool: &str, a: &Value) -> Result<HttpRequest> {
        let o = |k: &str| opt_str(a, k).map(String::from);
        Ok(match tool {
            "SLACK_LIST_CHANNELS" => self.get("conversations.list", query(&[
                ("types", Some(o("types").unwrap_or_else(|| "public_channel,private_channel".into()))),
                ("limit", Some(opt_u64(a, "limit", 200, 1, 1000).to_string())),
                ("cursor", o("cursor")),
                ("exclude_archived", Some((a.get("exclude_archived").and_then(Value::as_bool) != Some(false)).to_string())),
            ])),
            "SLACK_GET_HISTORY" => self.get("conversations.history", query(&[
                ("channel", Some(slack_id(a, "channel")?.into())),
                ("limit", Some(opt_u64(a, "limit", 50, 1, 1000).to_string())),
                ("oldest", o("oldest")), ("latest", o("latest")), ("cursor", o("cursor")),
            ])),
            "SLACK_GET_THREAD" => self.get("conversations.replies", query(&[
                ("channel", Some(slack_id(a, "channel")?.into())),
                ("ts", Some(slack_id(a, "thread_ts")?.into())),
                ("limit", Some(opt_u64(a, "limit", 100, 1, 1000).to_string())),
            ])),
            "SLACK_SEARCH_MESSAGES" => self.get("search.messages", query(&[
                ("query", Some(req_str(a, "query")?.into())),
                ("count", Some(opt_u64(a, "count", 20, 1, 100).to_string())),
                ("page", Some(opt_u64(a, "page", 1, 1, 100).to_string())),
            ])),
            "SLACK_FIND_USER_BY_EMAIL" => self.get("users.lookupByEmail", query(&[("email", Some(req_str(a, "email")?.into()))])),
            "SLACK_SEND_MESSAGE" => {
                let mut b = json!({"channel": slack_id(a, "channel")?, "text": req_str(a, "text")?});
                if opt_str(a, "thread_ts").is_some() {
                    b["thread_ts"] = json!(slack_id(a, "thread_ts")?);
                }
                self.post("chat.postMessage", b)
            }
            "SLACK_UPDATE_MESSAGE" => self.post("chat.update",
                json!({"channel": slack_id(a, "channel")?, "ts": slack_id(a, "ts")?, "text": req_str(a, "text")?})),
            "SLACK_ADD_REACTION" => self.post("reactions.add",
                json!({"channel": slack_id(a, "channel")?, "timestamp": slack_id(a, "ts")?,
                       "name": req_str(a, "name")?.trim_matches(':')})),
            other => bail!("unknown Slack tool `{other}`"),
        })
    }

    fn parse_response(&self, tool: &str, response: &HttpResponse) -> Result<Value> {
        let v = response.json()?;
        // Slack answers errors with HTTP 200 + ok:false.
        if v.get("ok").and_then(Value::as_bool) != Some(true) {
            let e = v
                .get("error")
                .and_then(Value::as_str)
                .unwrap_or("unknown_error");
            return Err(anyhow!("slack error: {e}"));
        }
        let msg = |m: &Value| json!({"ts": m["ts"], "user": m["user"], "text": m["text"], "thread_ts": m["thread_ts"], "reply_count": m["reply_count"]});
        Ok(match tool {
            "SLACK_LIST_CHANNELS" => {
                json!({"channels": v["channels"].as_array().map(|c| c.iter().map(|c| json!({
                "id": c["id"], "name": c["name"], "is_private": c["is_private"], "is_member": c["is_member"],
                "num_members": c["num_members"]})).collect::<Vec<_>>()),
                "next_cursor": v["response_metadata"]["next_cursor"]})
            }
            "SLACK_GET_HISTORY" | "SLACK_GET_THREAD" => json!({"messages": v["messages"].as_array()
                .map(|m| m.iter().map(msg).collect::<Vec<_>>()), "has_more": v["has_more"],
                "next_cursor": v["response_metadata"]["next_cursor"]}),
            "SLACK_SEARCH_MESSAGES" => {
                json!({"total": v["messages"]["total"], "matches": v["messages"]["matches"]
                .as_array().map(|m| m.iter().map(|x| json!({"ts": x["ts"], "text": x["text"], "user": x["username"],
                "channel": x["channel"]["name"], "permalink": x["permalink"]})).collect::<Vec<_>>())})
            }
            "SLACK_FIND_USER_BY_EMAIL" => json!({"id": v["user"]["id"], "name": v["user"]["name"],
                "real_name": v["user"]["real_name"], "tz": v["user"]["tz"]}),
            "SLACK_SEND_MESSAGE" | "SLACK_UPDATE_MESSAGE" => {
                json!({"channel": v["channel"], "ts": v["ts"]})
            }
            _ => json!({"ok": true}),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn requests_ids_and_errors() {
        let s = Slack::with_api_base("https://s.test/api");
        let r = s
            .build_request(
                "SLACK_GET_HISTORY",
                &json!({"channel": "C123", "limit": 5000}),
            )
            .unwrap();
        assert_eq!(
            r.url,
            "https://s.test/api/conversations.history?channel=C123&limit=1000"
        );
        assert!(s
            .build_request("SLACK_GET_HISTORY", &json!({"channel": "C1&x=y"}))
            .is_err());
        let p = s
            .build_request(
                "SLACK_SEND_MESSAGE",
                &json!({"channel": "C1", "text": "Hi", "thread_ts": "1.2"}),
            )
            .unwrap();
        assert_eq!(
            serde_json::from_slice::<Value>(&p.body.unwrap()).unwrap(),
            json!({"channel": "C1", "text": "Hi", "thread_ts": "1.2"})
        );
        let err = HttpResponse {
            status: 200,
            body: br#"{"ok":false,"error":"channel_not_found"}"#.to_vec(),
            session_id: None,
        };
        assert!(s
            .parse_response("SLACK_GET_HISTORY", &err)
            .unwrap_err()
            .to_string()
            .contains("channel_not_found"));
    }
}
