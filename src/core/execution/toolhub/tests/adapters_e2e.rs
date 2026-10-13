//! End-to-end adapter tests: CLI-equivalent hub.execute with a stored instance
//! credential against a scripted provider (no real credentials needed).
use std::sync::{Arc, Mutex};

use ctox_toolhub::http::{HttpRequest, HttpResponse, HttpTransport};
use ctox_toolhub::oauth2::RedirectTarget;
use ctox_toolhub::*;
use serde_json::{json, Value};

#[derive(Default)]
struct Mock(Mutex<Vec<HttpRequest>>);
impl HttpTransport for Mock {
    fn send(&self, r: HttpRequest) -> anyhow::Result<HttpResponse> {
        self.0.lock().unwrap().push(r.clone());
        let auth = r.headers.get("authorization").cloned().unwrap_or_default();
        let ok = |v: Value| {
            Ok(HttpResponse {
                status: 200,
                body: serde_json::to_vec(&v).unwrap(),
                session_id: None,
            })
        };
        if auth != "Bearer tok-good" {
            return Ok(HttpResponse {
                status: 401,
                body: b"{}".to_vec(),
                session_id: None,
            });
        }
        let u = r.url.as_str();
        if u.contains("/conversations.list") {
            return ok(
                json!({"ok": true, "channels": [{"id": "C1", "name": "general", "is_private": false, "is_member": true}], "response_metadata": {"next_cursor": ""}}),
            );
        }
        if u.contains("/conversations.history") {
            return ok(
                json!({"ok": true, "messages": [{"ts": "1.1", "user": "U1", "text": "hello"}], "has_more": false}),
            );
        }
        if u.contains("/search.messages") {
            return ok(
                json!({"ok": true, "messages": {"total": 1, "matches": [{"ts": "1.1", "text": "hello", "username": "m", "channel": {"name": "general"}}]}}),
            );
        }
        if u.contains("/chat.postMessage") {
            let b: Value = serde_json::from_slice(r.body.as_deref().unwrap()).unwrap();
            if b["channel"] == "CX" {
                return ok(json!({"ok": false, "error": "channel_not_found"}));
            }
            return ok(json!({"ok": true, "channel": b["channel"], "ts": "2.2"}));
        }
        if u.contains("/messages?") {
            return ok(
                json!({"messages": [{"id": "m1", "threadId": "t1"}], "resultSizeEstimate": 1}),
            );
        }
        if u.ends_with("/messages/m1?format=full") {
            return ok(
                json!({"id": "m1", "threadId": "t1", "snippet": "hi", "payload": {"mimeType": "text/plain",
                "headers": [{"name": "Subject", "value": "Hallo"}, {"name": "From", "value": "a@b"}], "body": {"data": "aGk"}}}),
            );
        }
        if u.ends_with("/drafts") {
            return ok(json!({"id": "d1", "message": {"id": "m2"}}));
        }
        if u.ends_with("/messages/send") {
            return ok(json!({"id": "m3", "threadId": "t3", "labelIds": ["SENT"]}));
        }
        if u.ends_with("/labels") {
            return ok(json!({"labels": [{"id": "INBOX", "name": "INBOX", "type": "system"}]}));
        }
        Ok(HttpResponse {
            status: 404,
            body: b"{}".to_vec(),
            session_id: None,
        })
    }
}

fn hub(mock: Arc<Mock>) -> ToolHub<MemorySecretBackend> {
    let mut hub = ToolHub::new(
        MemorySecretBackend::default(),
        mock,
        RedirectTarget::Loopback { port: 1 },
    );
    let _ = &mut hub;
    for (id, tk) in [("slack-token", "slack"), ("gmail-token", "gmail")] {
        hub.upsert_auth_config(
            &AuthConfig {
                id: id.into(),
                toolkit: tk.into(),
                scheme: AuthSchemeKind::Bearer,
                source: ClientCredentialSource::None,
                client_id: None,
                scopes: vec![],
                enabled: true,
            },
            None,
        )
        .unwrap();
        hub.connect_with_credential(id, "_instance", "michael", "tok-good")
            .unwrap();
    }
    hub
}

fn data(o: ToolOutcome) -> Value {
    match o {
        ToolOutcome::Success { data } => data,
        other => panic!("unexpected outcome: {other:?}"),
    }
}

#[test]
fn slack_core_actions_end_to_end() {
    let mock = Arc::new(Mock::default());
    let h = hub(mock.clone());
    let p = "_instance";
    assert_eq!(
        data(
            h.execute(p, "SLACK_LIST_CHANNELS", &json!({}), None)
                .unwrap()
        )["channels"][0]["name"],
        "general"
    );
    assert_eq!(
        data(
            h.execute(p, "SLACK_GET_HISTORY", &json!({"channel": "C1"}), None)
                .unwrap()
        )["messages"][0]["text"],
        "hello"
    );
    assert_eq!(
        data(
            h.execute(p, "SLACK_SEARCH_MESSAGES", &json!({"query": "hello"}), None)
                .unwrap()
        )["total"],
        1
    );
    assert_eq!(
        data(
            h.execute(
                p,
                "SLACK_SEND_MESSAGE",
                &json!({"channel": "C1", "text": "Hi"}),
                None
            )
            .unwrap()
        )["ts"],
        "2.2"
    );
    assert!(matches!(
        h.execute(
            p,
            "SLACK_SEND_MESSAGE",
            &json!({"channel": "CX", "text": "Hi"}),
            None
        )
        .unwrap(),
        ToolOutcome::ProviderError { .. }
    ));
    assert!(mock
        .0
        .lock()
        .unwrap()
        .iter()
        .all(|r| r.headers["authorization"] == "Bearer tok-good"));
}

#[test]
fn gmail_core_actions_end_to_end() {
    let h = hub(Arc::new(Mock::default()));
    let p = "_instance";
    assert_eq!(
        data(
            h.execute(
                p,
                "GMAIL_SEARCH_MESSAGES",
                &json!({"query": "is:unread"}),
                None
            )
            .unwrap()
        )["messages"][0]["id"],
        "m1"
    );
    let m = data(
        h.execute(p, "GMAIL_GET_MESSAGE", &json!({"message_id": "m1"}), None)
            .unwrap(),
    );
    assert_eq!(
        (m["subject"].as_str(), m["body"].as_str()),
        (Some("Hallo"), Some("hi"))
    );
    assert_eq!(
        data(
            h.execute(
                p,
                "GMAIL_CREATE_DRAFT",
                &json!({"to": ["x@y"], "subject": "S", "body": "B"}),
                None
            )
            .unwrap()
        )["draft_id"],
        "d1"
    );
    assert_eq!(
        data(
            h.execute(
                p,
                "GMAIL_SEND_EMAIL",
                &json!({"to": "x@y", "subject": "S"}),
                None
            )
            .unwrap()
        )["id"],
        "m3"
    );
    assert_eq!(
        data(h.execute(p, "GMAIL_LIST_LABELS", &json!({}), None).unwrap())[0]["id"],
        "INBOX"
    );
}

#[test]
fn mutating_flags_gate_send_actions() {
    let h = hub(Arc::new(Mock::default()));
    for (slug, mutating) in [
        ("SLACK_SEND_MESSAGE", true),
        ("SLACK_LIST_CHANNELS", false),
        ("GMAIL_SEND_EMAIL", true),
        ("GMAIL_CREATE_DRAFT", true),
        ("GMAIL_GET_MESSAGE", false),
    ] {
        assert_eq!(h.tool(slug).unwrap().1.mutating, mutating, "{slug}");
    }
}
