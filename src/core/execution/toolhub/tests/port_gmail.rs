//! Recorded cases for every Gmail tool, derived from the Activepieces
//! `piece-gmail` `run()` implementations (MIT), plus one ignored live test.
mod support;

use base64::Engine as _;
use ctox_toolhub::toolkits::gmail::Gmail;
use ctox_toolhub::toolkits::ToolkitAdapter;
use serde_json::{json, Value};
use support::Case;

/// Full-format message (text/plain part data = "hi").
fn full_message() -> Value {
    json!({
        "id": "18a", "threadId": "t1", "labelIds": ["INBOX", "UNREAD"],
        "snippet": "Quarterly report hi",
        "payload": {
            "mimeType": "multipart/alternative",
            "headers": [
                {"name": "From", "value": "boss@example.com"},
                {"name": "To", "value": "me@example.com"},
                {"name": "Subject", "value": "Quarterly report"},
                {"name": "Date", "value": "Mon, 12 Oct 2026 09:15:00 +0000"}
            ],
            "parts": [
                {"mimeType": "text/plain", "body": {"data": "aGk"}},
                {"mimeType": "text/html", "body": {"data": "PGk+aGk8L2k+"}}
            ]
        }
    })
}

#[test]
fn full_coverage() {
    let g = Gmail::default();
    let msg = full_message();
    // Bound to a `let` so the json! temporaries referenced by `result_eq`
    // live until the end of the test.
    let cases = [
        // --- read / list / search --------------------------------------
        Case {
            tool: "GMAIL_SEARCH_MESSAGES",
            args: json!({"query": "is:unread", "label_ids": ["INBOX"], "max_results": 5}),
            method: "GET",
            url_contains: &["/gmail/v1/users/me/messages?", "q=is%3Aunread", "maxResults=5", "labelIds=INBOX"],
            body_eq: &[],
            response_status: 200,
            response: json!({"messages": [{"id": "18a", "threadId": "t1"}], "resultSizeEstimate": 1, "nextPageToken": "tok2"}),
            result_eq: &[
                ("/messages/0/id", json!("18a")),
                ("/result_size_estimate", json!(1)),
                ("/next_page_token", json!("tok2")),
            ],
        },
        Case {
            // search-email-action.ts: builder filters -> one Gmail query string.
            tool: "GMAIL_SEARCH_MAIL",
            args: json!({
                "from": "boss@example.com",
                "subject": "Invoice",
                "content": "overdue",
                "has_attachment": true,
                "attachment_name": "inv.pdf",
                "category": "updates",
                "after_date": "2026-01-31T10:00:00Z",
                "before_date": "2026-02-28T00:00:00Z",
                "include_spam_trash": true,
                "max_results": 10,
            }),
            method: "GET",
            url_contains: &[
                "q=from%3A%28boss%40example.com%29",
                "subject%3A%28Invoice%29",
                "%22overdue%22",
                "has%3Aattachment",
                "filename%3A%28inv.pdf%29",
                "category%3Aupdates",
                "after%3A2026%2F01%2F31",
                "before%3A2026%2F02%2F28",
                "maxResults=10",
                "includeSpamTrash=true",
            ],
            body_eq: &[],
            response_status: 200,
            response: json!({"messages": [{"id": "18a", "threadId": "t1"}], "resultSizeEstimate": 1}),
            result_eq: &[("/found", json!(true)), ("/results/count", json!(1))],
        },
        Case {
            tool: "GMAIL_GET_MESSAGE",
            args: json!({"message_id": "18a"}),
            method: "GET",
            url_contains: &["/gmail/v1/users/me/messages/18a?format=full"],
            body_eq: &[],
            response_status: 200,
            response: msg.clone(),
            result_eq: &[
                ("/id", json!("18a")),
                ("/subject", json!("Quarterly report")),
                ("/from", json!("boss@example.com")),
                ("/body", json!("hi")),
                ("/thread_id", json!("t1")),
            ],
        },
        Case {
            tool: "GMAIL_GET_THREAD",
            args: json!({"thread_id": "t1"}),
            method: "GET",
            url_contains: &["/gmail/v1/users/me/threads/t1?format=full"],
            body_eq: &[],
            response_status: 200,
            response: json!({"id": "t1", "messages": [msg]}),
            result_eq: &[("/id", json!("t1")), ("/messages/0/id", json!("18a"))],
        },
        Case {
            tool: "GMAIL_LIST_LABELS",
            args: json!({}),
            method: "GET",
            url_contains: &["/gmail/v1/users/me/labels"],
            body_eq: &[],
            response_status: 200,
            response: json!({"labels": [
                {"id": "INBOX", "name": "INBOX", "type": "system"},
                {"id": "Label_5", "name": "Follow Up", "type": "user"}
            ]}),
            result_eq: &[
                ("/0/id", json!("INBOX")),
                ("/1/name", json!("Follow Up")),
                ("/1/type", json!("user")),
            ],
        },
        Case {
            tool: "GMAIL_LIST_DRAFTS",
            args: json!({"query": "subject:invoice", "max_results": 25}),
            method: "GET",
            url_contains: &["/gmail/v1/users/me/drafts?", "q=subject%3Ainvoice", "maxResults=25"],
            body_eq: &[],
            response_status: 200,
            response: json!({"drafts": [{"id": "r99", "message": {"id": "18a", "threadId": "t1"}}], "resultSizeEstimate": 1}),
            result_eq: &[
                ("/drafts/0/id", json!("r99")),
                ("/drafts/0/message/id", json!("18a")),
                ("/count", json!(1)),
            ],
        },
        Case {
            tool: "GMAIL_LIST_THREADS",
            args: json!({
                "query": "is:unread",
                "label_ids": ["INBOX"],
                "include_spam_trash": true,
                "max_results": 20,
            }),
            method: "GET",
            url_contains: &[
                "/gmail/v1/users/me/threads?",
                "q=is%3Aunread",
                "includeSpamTrash=true",
                "maxResults=20",
                "labelIds=INBOX",
            ],
            body_eq: &[],
            response_status: 200,
            response: json!({"threads": [{"id": "t1", "snippet": "Quarterly report hi"}], "resultSizeEstimate": 1, "nextPageToken": "p2"}),
            result_eq: &[
                ("/threads/0/id", json!("t1")),
                ("/count", json!(1)),
                ("/next_page_token", json!("p2")),
            ],
        },
        Case {
            tool: "GMAIL_LIST_HISTORY",
            args: json!({
                "start_history_id": "9912",
                "history_types": ["messageAdded", "labelAdded"],
                "max_results": 50,
            }),
            method: "GET",
            url_contains: &[
                "/gmail/v1/users/me/history?",
                "startHistoryId=9912",
                "historyTypes=messageAdded",
                "historyTypes=labelAdded",
                "maxResults=50",
            ],
            body_eq: &[],
            response_status: 200,
            response: json!({
                "history": [{"messagesAdded": [{"message": {"id": "18a", "threadId": "t1"}}]}],
                "historyId": "9915"
            }),
            result_eq: &[
                ("/historyId", json!("9915")),
                ("/history/0/messagesAdded/0/message/id", json!("18a")),
            ],
        },
        Case {
            tool: "GMAIL_GET_DRAFT",
            args: json!({"draft_id": "r99"}),
            method: "GET",
            url_contains: &["/gmail/v1/users/me/drafts/r99?format=full"],
            body_eq: &[],
            response_status: 200,
            response: json!({"id": "r99", "message": {"id": "18a", "threadId": "t1"}}),
            result_eq: &[("/id", json!("r99")), ("/message/id", json!("18a"))],
        },
        Case {
            tool: "GMAIL_GET_LABEL",
            args: json!({"label_id": "Label_5"}),
            method: "GET",
            url_contains: &["/gmail/v1/users/me/labels/Label_5"],
            body_eq: &[],
            response_status: 200,
            response: json!({
                "id": "Label_5", "name": "Follow Up", "type": "user",
                "messageListVisibility": "show", "labelListVisibility": "labelShow",
                "messagesTotal": 12, "threadsTotal": 3
            }),
            result_eq: &[
                ("/name", json!("Follow Up")),
                ("/messagesTotal", json!(12)),
                ("/threadsTotal", json!(3)),
            ],
        },
        Case {
            tool: "GMAIL_GET_ATTACHMENT",
            args: json!({"message_id": "18a", "attachment_id": "ANG1", "file_name": "inv.pdf"}),
            method: "GET",
            url_contains: &["/gmail/v1/users/me/messages/18a/attachments/ANG1"],
            body_eq: &[],
            response_status: 200,
            response: json!({"size": 4, "data": "aGVsbG8="}),
            result_eq: &[("/size", json!(4)), ("/data", json!("aGVsbG8="))],
        },
        Case {
            tool: "GMAIL_GET_PROFILE",
            args: json!({}),
            method: "GET",
            url_contains: &["/gmail/v1/users/me/profile"],
            body_eq: &[],
            response_status: 200,
            response: json!({
                "emailAddress": "me@example.com",
                "messagesTotal": 1200,
                "threadsTotal": 300,
                "historyId": "9915"
            }),
            result_eq: &[
                ("/emailAddress", json!("me@example.com")),
                ("/historyId", json!("9915")),
            ],
        },
        // --- write / send ----------------------------------------------
        Case {
            // archive-email-action.ts: modify with removeLabelIds ["INBOX"].
            tool: "GMAIL_ARCHIVE_MESSAGE",
            args: json!({"message_id": "18a"}),
            method: "POST",
            url_contains: &["/gmail/v1/users/me/messages/18a/modify"],
            body_eq: &[("/removeLabelIds", json!(["INBOX"]))],
            response_status: 200,
            response: json!({"id": "18a", "threadId": "t1", "labelIds": ["UNREAD"]}),
            result_eq: &[("/labelIds/0", json!("UNREAD"))],
        },
        Case {
            tool: "GMAIL_TRASH_MESSAGE",
            args: json!({"message_id": "18a"}),
            method: "POST",
            url_contains: &["/gmail/v1/users/me/messages/18a/trash"],
            body_eq: &[],
            response_status: 200,
            response: json!({"id": "18a", "threadId": "t1", "labelIds": ["TRASH"]}),
            result_eq: &[("/labelIds/0", json!("TRASH"))],
        },
        Case {
            tool: "GMAIL_UNTRASH_MESSAGE",
            args: json!({"message_id": "18a"}),
            method: "POST",
            url_contains: &["/gmail/v1/users/me/messages/18a/untrash"],
            body_eq: &[],
            response_status: 200,
            response: json!({"id": "18a", "threadId": "t1", "labelIds": ["INBOX"]}),
            result_eq: &[("/labelIds/0", json!("INBOX"))],
        },
        Case {
            // add-label-to-email-action.ts: modify with addLabelIds [label.id].
            tool: "GMAIL_ADD_LABEL_TO_EMAIL",
            args: json!({"message_id": "18a", "label_id": "Label_5"}),
            method: "POST",
            url_contains: &["/gmail/v1/users/me/messages/18a/modify"],
            body_eq: &[("/addLabelIds", json!(["Label_5"]))],
            response_status: 200,
            response: json!({"id": "18a", "labelIds": ["INBOX", "Label_5"]}),
            result_eq: &[("/labelIds/1", json!("Label_5"))],
        },
        Case {
            tool: "GMAIL_REMOVE_LABEL_FROM_EMAIL",
            args: json!({"message_id": "18a", "label_id": "UNREAD"}),
            method: "POST",
            url_contains: &["/gmail/v1/users/me/messages/18a/modify"],
            body_eq: &[("/removeLabelIds", json!(["UNREAD"]))],
            response_status: 200,
            response: json!({"id": "18a", "labelIds": ["INBOX"]}),
            result_eq: &[("/labelIds/0", json!("INBOX"))],
        },
        Case {
            tool: "GMAIL_MODIFY_LABELS",
            args: json!({
                "message_id": "18a",
                "add_label_ids": ["Label_5"],
                "remove_label_ids": ["UNREAD"],
            }),
            method: "POST",
            url_contains: &["/gmail/v1/users/me/messages/18a/modify"],
            body_eq: &[
                ("/addLabelIds", json!(["Label_5"])),
                ("/removeLabelIds", json!(["UNREAD"])),
            ],
            response_status: 200,
            response: json!({"id": "18a", "labelIds": ["INBOX", "Label_5"]}),
            result_eq: &[("/id", json!("18a"))],
        },
        Case {
            // modify-labels-action.ts: users.messages.batchModify {ids, ...}.
            tool: "GMAIL_BATCH_MODIFY_LABELS",
            args: json!({"message_ids": ["18a", "18b"], "remove_label_ids": ["INBOX"]}),
            method: "POST",
            url_contains: &["/gmail/v1/users/me/messages/batchModify"],
            body_eq: &[
                ("/ids", json!(["18a", "18b"])),
                ("/removeLabelIds", json!(["INBOX"])),
            ],
            response_status: 204,
            response: Value::Null,
            result_eq: &[("/success", json!(true))],
        },
        Case {
            tool: "GMAIL_MODIFY_THREAD_LABELS",

            args: json!({"thread_id": "t1", "add_label_ids": ["Label_5"], "remove_label_ids": ["UNREAD"]}),
            method: "POST",
            url_contains: &["/gmail/v1/users/me/threads/t1/modify"],
            body_eq: &[
                ("/addLabelIds", json!(["Label_5"])),
                ("/removeLabelIds", json!(["UNREAD"])),
            ],
            response_status: 200,
            response: json!({"id": "t1", "snippet": "Quarterly report hi"}),
            result_eq: &[("/id", json!("t1"))],
        },
        Case {
            // create-label-action.ts: users.labels.create {name}.
            tool: "GMAIL_CREATE_LABEL",
            args: json!({"name": "Follow Up"}),
            method: "POST",
            url_contains: &["/gmail/v1/users/me/labels"],
            body_eq: &[("/name", json!("Follow Up"))],
            response_status: 200,
            response: json!({"id": "Label_9", "name": "Follow Up", "type": "user"}),
            result_eq: &[("/id", json!("Label_9")), ("/type", json!("user"))],
        },
        Case {
            // update-label-action.ts: users.labels.patch with only set fields.
            tool: "GMAIL_UPDATE_LABEL",
            args: json!({
                "label_id": "Label_5",
                "name": "Follow Up Later",
                "label_list_visibility": "labelHide",
                "message_list_visibility": "hide",
                "background_color": "#fb4c2f",
                "text_color": "#000000",
            }),
            method: "PATCH",
            url_contains: &["/gmail/v1/users/me/labels/Label_5"],
            body_eq: &[
                ("/name", json!("Follow Up Later")),
                ("/labelListVisibility", json!("labelHide")),
                ("/messageListVisibility", json!("hide")),
                ("/color/backgroundColor", json!("#fb4c2f")),
                ("/color/textColor", json!("#000000")),
            ],
            response_status: 200,
            response: json!({"id": "Label_5", "name": "Follow Up Later", "type": "user"}),
            result_eq: &[("/name", json!("Follow Up Later"))],
        },
        Case {
            tool: "GMAIL_CREATE_DRAFT",
            args: json!({
                "to": ["x@y"],
                "cc": ["c@z"],
                "subject": "S",
                "body": "B",
                "body_type": "plain_text",
                "from": "me@example.com",
                "sender_name": "Me Myself",
                "thread_id": "t1",
            }),
            method: "POST",
            url_contains: &["/gmail/v1/users/me/drafts"],
            body_eq: &[("/message/threadId", json!("t1"))],
            response_status: 200,
            response: json!({"id": "d1", "message": {"id": "18c", "threadId": "t1"}}),
            result_eq: &[("/draft_id", json!("d1")), ("/message_id", json!("18c"))],
        },
        Case {
            // update-draft-action.ts: users.drafts.update {message {raw}}.
            tool: "GMAIL_UPDATE_DRAFT",
            args: json!({"draft_id": "r99", "to": ["x@y"], "subject": "S2", "body": "B2"}),
            method: "PATCH",
            url_contains: &["/gmail/v1/users/me/drafts/r99"],
            body_eq: &[],
            response_status: 200,
            response: json!({"id": "r99", "message": {"id": "18d", "threadId": "t1"}}),
            result_eq: &[("/draft_id", json!("r99")), ("/message_id", json!("18d"))],
        },
        Case {
            // send-draft-action.ts: users.drafts.send {id}.
            tool: "GMAIL_SEND_DRAFT",
            args: json!({"draft_id": "r99"}),
            method: "POST",
            url_contains: &["/gmail/v1/users/me/drafts/send"],
            body_eq: &[("/id", json!("r99"))],
            response_status: 200,
            response: json!({"id": "18e", "threadId": "t1", "labelIds": ["SENT"]}),
            result_eq: &[
                ("/id", json!("18e")),
                ("/thread_id", json!("t1")),
                ("/label_ids", json!(["SENT"])),
            ],
        },
        Case {
            // delete-draft-action.ts: users.drafts.delete (204 on success).
            tool: "GMAIL_DELETE_DRAFT",
            args: json!({"draft_id": "r99"}),
            method: "DELETE",
            url_contains: &["/gmail/v1/users/me/drafts/r99"],
            body_eq: &[],
            response_status: 204,
            response: Value::Null,
            result_eq: &[
                ("/success", json!(true)),
                ("/deleted", json!(true)),
            ],
        },
        Case {
            tool: "GMAIL_SEND_EMAIL",
            args: json!({
                "to": ["a@b", "c@d"],
                "cc": ["e@f"],
                "subject": "Invoice",
                "body": "Hello",
                "body_type": "plain_text",
                "reply_to": ["support@example.com"],
                "from": "sales@example.com",
                "sender_name": "Acme Sales",
                "in_reply_to": "<msg-1@mail.example>",
                "thread_id": "t9",
            }),
            method: "POST",
            url_contains: &["/gmail/v1/users/me/messages/send"],
            body_eq: &[("/threadId", json!("t9"))],
            response_status: 200,
            response: json!({"id": "18f", "threadId": "t9", "labelIds": ["SENT"]}),
            result_eq: &[
                ("/id", json!("18f")),
                ("/thread_id", json!("t9")),
                ("/label_ids", json!(["SENT"])),
            ],
        },
        Case {
            // stop-watch-action.ts: users.stop.
            tool: "GMAIL_STOP_WATCH",
            args: json!({}),
            method: "POST",
            url_contains: &["/gmail/v1/users/me/stop"],
            body_eq: &[],
            response_status: 204,
            response: Value::Null,
            result_eq: &[("/success", json!(true))],
        },
    ];

    support::assert_full_coverage(&g, &cases);
    for c in &cases {
        support::run(&g, c);
    }
}

/// The recorded cases only pin MIME-level fields through pointers; decode the
/// built `raw` payloads here to check the MIME headers derived from the TS
/// `buildRawMessage`/`MailComposer` options.
#[test]
fn raw_mime_headers() {
    let g = Gmail::default();
    let mime = |tool: &str, args: Value| -> String {
        let req = g.build_request(tool, &args).expect("build");
        let body: Value = serde_json::from_slice(req.body.as_deref().expect("body")).expect("json");
        let raw = body.pointer("/message/raw").or_else(|| body.pointer("/raw")).expect("raw");
        let bytes = base64::engine::general_purpose::URL_SAFE_NO_PAD
            .decode(raw.as_str().unwrap())
            .expect("base64url");
        String::from_utf8(bytes).expect("utf-8")
    };

    let m = mime(
        "GMAIL_SEND_EMAIL",
        json!({
            "to": ["a@b", "c@d"], "cc": ["e@f"], "bcc": ["g@h"],
            "subject": "Invoice",
            "body": "Hello",
            "reply_to": ["support@example.com"],
            "from": "sales@example.com",
            "sender_name": "Acme Sales",
            "in_reply_to": "<msg-1@mail.example>",
        }),
    );
    assert!(m.starts_with("To: a@b, c@d\r\nCc: e@f\r\nBcc: g@h\r\n"));
    assert!(m.contains("Reply-To: support@example.com\r\n"));
    assert!(m.contains("From: Acme Sales <sales@example.com>\r\n"));
    assert!(m.contains("Subject: =?UTF-8?B?SW52b2ljZQ==?=\r\n"));
    assert!(m.contains("In-Reply-To: <msg-1@mail.example>\r\nReferences: <msg-1@mail.example>\r\n"));
    assert!(m.contains("MIME-Version: 1.0\r\n"));
    assert!(m.contains("Content-Type: text/plain; charset=UTF-8\r\nContent-Transfer-Encoding: base64\r\n\r\nSGVsbG8=\r\n"));

    // body_type html -> single text/html part (send-email-action.ts `html` option).
    let m = mime(
        "GMAIL_SEND_EMAIL",
        json!({"to": ["a@b"], "subject": "S", "body": "<b>Hi</b>", "body_type": "html"}),
    );
    assert!(m.contains("Content-Type: text/html; charset=UTF-8"));
    assert!(m.contains("PGI+SGk8L2I+"));

    // body_html -> multipart/alternative (legacy prop kept for compatibility).
    let m = mime(
        "GMAIL_CREATE_DRAFT",
        json!({"to": ["a@b"], "subject": "S", "body": "plain", "body_html": "<i>rich</i>"}),
    );
    assert!(m.contains("multipart/alternative"));
    assert!(m.contains("Content-Type: text/plain"));
    assert!(m.contains("Content-Type: text/html"));

    // Header injection is rejected before any request is built.
    assert!(g
        .build_request(
            "GMAIL_SEND_EMAIL",
            &json!({"to": ["a@b"], "subject": "x\r\nBcc: evil@x"})
        )
        .is_err());
    assert!(g
        .build_request(
            "GMAIL_SEND_EMAIL",
            &json!({"to": ["a@b"], "subject": "S", "from": "a@b\r\nBcc: evil@x"})
        )
        .is_err());
}

/// Live read-only smoke test: `CTOX_TOOLHUB_LIVE_GMAIL_TOKEN=… cargo test
/// --test port_gmail -- --ignored`. Skips silently without the token.
#[test]
#[ignore]
fn live_read_only() {
    let g = Gmail::default();
    for (tool, args) in [
        ("GMAIL_GET_PROFILE", json!({})),
        ("GMAIL_LIST_LABELS", json!({})),
        ("GMAIL_SEARCH_MESSAGES", json!({"max_results": 3})),
    ] {
        match support::live(&g, tool, args) {
            None => return, // no token configured
            Some(Ok(v)) => println!("{tool}: {v}"),
            Some(Err(e)) => panic!("{tool} live call failed: {e}"),
        }
    }
}
