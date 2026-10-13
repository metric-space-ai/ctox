//! Gmail toolkit (Gmail API v1). Action set modelled on Activepieces `piece-gmail` (MIT).
use anyhow::{bail, Result};
use base64::Engine as _;
use serde_json::{json, Value};

use super::common::*;
use super::{path_segment, ToolkitAdapter};
use crate::http::{HttpRequest, HttpResponse};
use crate::model::*;

pub const SLUG: &str = "gmail";
const READ: &str = "https://www.googleapis.com/auth/gmail.readonly";
const SEND: &str = "https://www.googleapis.com/auth/gmail.send";
const MODIFY: &str = "https://www.googleapis.com/auth/gmail.modify";
const COMPOSE: &str = "https://www.googleapis.com/auth/gmail.compose";

pub struct Gmail {
    spec: ToolkitSpec,
    api_base: String,
}

impl Default for Gmail {
    fn default() -> Self {
        Self::with_api_base("https://gmail.googleapis.com")
    }
}

impl Gmail {
    pub fn with_api_base(api_base: &str) -> Self {
        let mail: Vec<(&str, Value)> = vec![
            ("to", str_list("Empfänger / recipients")),
            ("cc", str_list("CC")),
            ("bcc", str_list("BCC")),
            ("subject", s("Betreff / subject")),
            ("body", s("Text (plain)")),
            ("body_html", s("Optional HTML body")),
            ("thread_id", s("Reply within this thread")),
        ];
        let t = |slug, title, desc, props: Props<'_>, req: &[&str], scopes: &[&str], m| {
            tool(SLUG, slug, title, desc, props, req, scopes, m)
        };
        let spec = ToolkitSpec {
            slug: SLUG.into(),
            name: "Gmail".into(),
            description: LocalizedText::new(
                "E-Mails in Gmail suchen, lesen, entwerfen und senden.",
                "Search, read, draft and send Gmail messages.",
            ),
            auth_schemes: vec![
                AuthScheme::OAuth2(OAuth2Scheme {
                    authorize_url: "https://accounts.google.com/o/oauth2/v2/auth".into(),
                    token_url: "https://oauth2.googleapis.com/token".into(),
                    default_scopes: vec![READ.into(), SEND.into(), MODIFY.into(), COMPOSE.into()],
                    pkce: true,
                    scope_separator: " ".into(),
                    refreshable: true,
                    extra_authorize_params: vec![
                        ("access_type".into(), "offline".into()),
                        ("prompt".into(), "consent".into()),
                    ],
                }),
                AuthScheme::Bearer,
            ],
            tools: vec![
                t(
                    "GMAIL_SEARCH_MESSAGES",
                    ("E-Mails suchen", "Search emails"),
                    (
                        "Sucht Nachrichten mit Gmail-Suchsyntax.",
                        "Searches messages using Gmail search syntax.",
                    ),
                    &[
                        ("query", s("e.g. `from:x is:unread`")),
                        ("label_ids", str_list("Label IDs")),
                        ("max_results", int("1-100", 1, 100, 20)),
                        ("page_token", s("Next page token")),
                    ],
                    &[],
                    &[READ],
                    false,
                ),
                t(
                    "GMAIL_GET_MESSAGE",
                    ("E-Mail lesen", "Read email"),
                    (
                        "Liest eine Nachricht mit Kopfzeilen und Text.",
                        "Reads a message with headers and text.",
                    ),
                    &[("message_id", s("Message ID"))],
                    &["message_id"],
                    &[READ],
                    false,
                ),
                t(
                    "GMAIL_GET_THREAD",
                    ("Konversation lesen", "Read thread"),
                    (
                        "Liest alle Nachrichten eines Threads.",
                        "Reads all messages of a thread.",
                    ),
                    &[("thread_id", s("Thread ID"))],
                    &["thread_id"],
                    &[READ],
                    false,
                ),
                t(
                    "GMAIL_LIST_LABELS",
                    ("Labels auflisten", "List labels"),
                    ("Listet die Labels des Postfachs.", "Lists mailbox labels."),
                    &[],
                    &[],
                    &[READ],
                    false,
                ),
                t(
                    "GMAIL_CREATE_DRAFT",
                    ("Entwurf anlegen", "Create draft"),
                    (
                        "Legt einen Entwurf an (wird nicht gesendet).",
                        "Creates a draft (not sent).",
                    ),
                    &mail,
                    &["to", "subject"],
                    &[COMPOSE],
                    true,
                ),
                t(
                    "GMAIL_SEND_EMAIL",
                    ("E-Mail senden", "Send email"),
                    (
                        "Sendet eine E-Mail; braucht --confirm.",
                        "Sends an email; requires --confirm.",
                    ),
                    &mail,
                    &["to", "subject"],
                    &[SEND],
                    true,
                ),
                t(
                    "GMAIL_MODIFY_LABELS",
                    ("Labels ändern", "Modify labels"),
                    (
                        "Labels hinzufügen/entfernen (Archivieren = INBOX entfernen).",
                        "Add/remove labels (archive = remove INBOX).",
                    ),
                    &[
                        ("message_id", s("Message ID")),
                        ("add_label_ids", str_list("Add")),
                        ("remove_label_ids", str_list("Remove")),
                    ],
                    &["message_id"],
                    &[MODIFY],
                    true,
                ),
            ],
        };
        Self {
            spec,
            api_base: api_base.trim_end_matches('/').to_string(),
        }
    }

    fn url(&self, path: &str) -> String {
        format!("{}/gmail/v1/users/me{path}", self.api_base)
    }

    pub fn raw_mime(args: &Value) -> Result<String> {
        let b64 = |x: &str| base64::engine::general_purpose::STANDARD.encode(x.as_bytes());
        if str_vec(args, "to").is_empty() {
            bail!("missing argument `to`");
        }
        let mut h = String::new();
        for (name, key) in [("To", "to"), ("Cc", "cc"), ("Bcc", "bcc")] {
            let v = str_vec(args, key);
            if !v.is_empty() {
                let joined = v.join(", ");
                header_safe(key, &joined)?;
                h.push_str(&format!("{name}: {joined}\r\n"));
            }
        }
        let subject = header_safe("subject", req_str(args, "subject")?)?;
        h.push_str(&format!(
            "Subject: =?UTF-8?B?{}?=\r\nMIME-Version: 1.0\r\n",
            b64(subject)
        ));
        let text = opt_str(args, "body").unwrap_or("");
        if let Some(html) = opt_str(args, "body_html") {
            let b = "ctox-toolhub-alt";
            h.push_str(&format!(
                "Content-Type: multipart/alternative; boundary=\"{b}\"\r\n\r\n"
            ));
            h.push_str(&format!("--{b}\r\nContent-Type: text/plain; charset=UTF-8\r\nContent-Transfer-Encoding: base64\r\n\r\n{}\r\n", b64(text)));
            h.push_str(&format!("--{b}\r\nContent-Type: text/html; charset=UTF-8\r\nContent-Transfer-Encoding: base64\r\n\r\n{}\r\n--{b}--\r\n", b64(html)));
        } else {
            h.push_str(&format!("Content-Type: text/plain; charset=UTF-8\r\nContent-Transfer-Encoding: base64\r\n\r\n{}\r\n", b64(text)));
        }
        Ok(base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(h.as_bytes()))
    }

    fn message_body(args: &Value) -> Result<Value> {
        let mut m = json!({"raw": Self::raw_mime(args)?});
        if let Some(t) = opt_str(args, "thread_id") {
            m["threadId"] = json!(t);
        }
        Ok(m)
    }
}

fn header(msg: &Value, name: &str) -> Value {
    msg["payload"]["headers"]
        .as_array()
        .and_then(|hs| {
            hs.iter().find(|h| {
                h["name"]
                    .as_str()
                    .is_some_and(|n| n.eq_ignore_ascii_case(name))
            })
        })
        .map(|h| h["value"].clone())
        .unwrap_or(Value::Null)
}

fn text_body(part: &Value) -> Option<String> {
    fn find(p: &Value, mime: &str) -> Option<String> {
        if p["mimeType"].as_str() == Some(mime) {
            if let Some(d) = p["body"]["data"].as_str() {
                let bytes = base64::engine::general_purpose::URL_SAFE_NO_PAD
                    .decode(d.trim_end_matches('='))
                    .ok()?;
                return Some(String::from_utf8_lossy(&bytes).into_owned());
            }
        }
        p["parts"].as_array()?.iter().find_map(|c| find(c, mime))
    }
    find(part, "text/plain").or_else(|| find(part, "text/html"))
}

fn summarize(m: &Value) -> Value {
    json!({"id": m["id"], "thread_id": m["threadId"], "label_ids": m["labelIds"], "snippet": m["snippet"],
        "from": header(m, "From"), "to": header(m, "To"), "subject": header(m, "Subject"),
        "date": header(m, "Date"), "body": text_body(&m["payload"])})
}

impl ToolkitAdapter for Gmail {
    fn spec(&self) -> &ToolkitSpec {
        &self.spec
    }

    fn build_request(&self, tool: &str, args: &Value) -> Result<HttpRequest> {
        Ok(match tool {
            "GMAIL_SEARCH_MESSAGES" => {
                let mut q = query(&[
                    ("q", opt_str(args, "query").map(String::from)),
                    (
                        "maxResults",
                        Some(opt_u64(args, "max_results", 20, 1, 100).to_string()),
                    ),
                    ("pageToken", opt_str(args, "page_token").map(String::from)),
                ]);
                for l in str_vec(args, "label_ids") {
                    let enc: String = url::form_urlencoded::byte_serialize(l.as_bytes()).collect();
                    q.push_str(&format!("&labelIds={enc}"));
                }
                HttpRequest::new("GET", self.url(&format!("/messages{q}")))
            }
            "GMAIL_GET_MESSAGE" => HttpRequest::new(
                "GET",
                self.url(&format!(
                    "/messages/{}?format=full",
                    path_segment(args, "message_id")?
                )),
            ),
            "GMAIL_GET_THREAD" => HttpRequest::new(
                "GET",
                self.url(&format!(
                    "/threads/{}?format=full",
                    path_segment(args, "thread_id")?
                )),
            ),
            "GMAIL_LIST_LABELS" => HttpRequest::new("GET", self.url("/labels")),
            "GMAIL_SEND_EMAIL" => HttpRequest::new("POST", self.url("/messages/send"))
                .json(&Self::message_body(args)?),
            "GMAIL_CREATE_DRAFT" => HttpRequest::new("POST", self.url("/drafts"))
                .json(&json!({"message": Self::message_body(args)?})),
            "GMAIL_MODIFY_LABELS" => {
                let (add, remove) = (
                    str_vec(args, "add_label_ids"),
                    str_vec(args, "remove_label_ids"),
                );
                if add.is_empty() && remove.is_empty() {
                    bail!("missing argument `add_label_ids` or `remove_label_ids`");
                }
                HttpRequest::new(
                    "POST",
                    self.url(&format!(
                        "/messages/{}/modify",
                        path_segment(args, "message_id")?
                    )),
                )
                .json(&json!({"addLabelIds": add, "removeLabelIds": remove}))
            }
            other => bail!("unknown Gmail tool `{other}`"),
        })
    }

    fn parse_response(&self, tool: &str, response: &HttpResponse) -> Result<Value> {
        let v = response.json()?;
        Ok(match tool {
            "GMAIL_GET_MESSAGE" => summarize(&v),
            "GMAIL_GET_THREAD" => json!({"id": v["id"], "messages":
                v["messages"].as_array().map(|m| m.iter().map(summarize).collect::<Vec<_>>())}),
            "GMAIL_SEARCH_MESSAGES" => {
                json!({"messages": v["messages"], "next_page_token": v["nextPageToken"],
                "result_size_estimate": v["resultSizeEstimate"]})
            }
            "GMAIL_LIST_LABELS" => json!(v["labels"].as_array().map(|l| l
                .iter()
                .map(|x| json!({"id": x["id"], "name": x["name"], "type": x["type"]}))
                .collect::<Vec<_>>())),
            "GMAIL_SEND_EMAIL" => {
                json!({"id": v["id"], "thread_id": v["threadId"], "label_ids": v["labelIds"]})
            }
            "GMAIL_CREATE_DRAFT" => json!({"draft_id": v["id"], "message_id": v["message"]["id"]}),
            _ => v,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn requests_and_header_injection() {
        let g = Gmail::with_api_base("https://g.test");
        let r = g
            .build_request(
                "GMAIL_SEARCH_MESSAGES",
                &json!({"query": "is:unread from:a@b", "label_ids": ["INBOX"], "max_results": 999}),
            )
            .unwrap();
        assert_eq!(r.url, "https://g.test/gmail/v1/users/me/messages?q=is%3Aunread+from%3Aa%40b&maxResults=100&labelIds=INBOX");
        assert!(g
            .build_request("GMAIL_GET_MESSAGE", &json!({"message_id": "../x"}))
            .is_err());
        assert!(g
            .build_request(
                "GMAIL_SEND_EMAIL",
                &json!({"to": "a@b", "subject": "x\r\nBcc: evil@x"})
            )
            .is_err());
        let msg = json!({"id": "1", "payload": {"headers": [{"name": "Subject", "value": "S"}],
            "mimeType": "multipart/alternative", "parts": [{"mimeType": "text/plain", "body": {"data": "aGk"}}]}});
        assert_eq!(summarize(&msg)["body"], "hi");
    }
}
