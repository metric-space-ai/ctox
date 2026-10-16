//! Telegram port: one recorded case per action. Every expectation is derived from
//! the Activepieces `run()` code — `POST https://api.telegram.org/bot<token>/<method>`
//! (`telegramCommons.getApiUrl`) with the JSON body the action assembles — and the
//! responses are the `{ok, result}` envelopes Telegram returns.
mod support;

use ctox_toolhub::toolkits::telegram::Telegram;
use serde_json::{json, Value};
use support::{live, run, Case};

/// `telegramCommons.getApiUrl(auth, method)` = `https://api.telegram.org/bot<token>/<method>`.
const BASE: &str = "https://telegram.test/bot700000:AAHrecordedtoken";

const CHAT_ID: &str = "-1001234567890";
const USER_ID: &str = "11111111";

fn adapter() -> Telegram {
    Telegram::with_api_base(BASE)
}

/// `&[(ptr, value)]` for a `Case` field: leaked so it lives as long as the case.
fn ptrs<const N: usize>(pairs: [(&'static str, Value); N]) -> &'static [(&'static str, Value)] {
    Box::leak(Box::new(pairs))
}

fn message(id: i64, chat: Value) -> Value {
    json!({
        "message_id": id,
        "date": 1_791_979_200,
        "chat": chat,
        "from": {"id": 700000, "is_bot": true, "first_name": "Acme Bot", "username": "acmebot"},
        "text": "Deployment 4.2 finished."
    })
}

fn chat() -> Value {
    json!({
        "id": -1001234567890_i64,
        "type": "supergroup",
        "title": "Acme Engineering",
        "username": "acmeeng",
        "description": "Release coordination",
        "invite_link": "https://t.me/joinchat/AAAA",
        "permissions": {"can_send_messages": true, "can_pin_messages": true},
        "photo": {"small_file_id": "s1", "small_file_unique_id": "su1",
                  "big_file_id": "b1", "big_file_unique_id": "bu1"},
        "members_count": 412
    })
}

#[test]
fn telegram_actions_are_ported() {
    let a = adapter();
    let cases: Vec<Case> = vec![
        Case {
            tool: "TELEGRAM_GET_BOT_INFO",
            args: json!({}),
            method: "POST",
            url_contains: &[BASE, "/getMe"],
            body_eq: ptrs([]),
            response_status: 200,
            response: json!({"ok": true, "result": {
                "id": 700000, "is_bot": true, "first_name": "Acme",
                "username": "acmebot", "can_join_groups": true,
                "can_read_all_group_messages": false, "supports_inline_queries": false,
                "can_connect_to_business": true, "has_main_web_app": false}}),
            result_eq: ptrs([
                ("/ok", json!(true)),
                ("/result/username", json!("acmebot")),
                ("/result/is_bot", json!(true)),
                ("/result/can_join_groups", json!(true)),
            ]),
        },
        Case {
            tool: "TELEGRAM_GET_CHAT",
            args: json!({"chat_id": CHAT_ID}),
            method: "POST",
            url_contains: &[BASE, "/getChat"],
            body_eq: ptrs([("/chat_id", json!(CHAT_ID))]),
            response_status: 200,
            response: json!({"ok": true, "result": chat()}),
            result_eq: ptrs([
                ("/result/title", json!("Acme Engineering")),
                ("/result/type", json!("supergroup")),
                ("/result/members_count", json!(412)),
            ]),
        },
        Case {
            tool: "TELEGRAM_GET_CHAT_MEMBER",
            args: json!({"chat_id": CHAT_ID, "user_id": USER_ID}),
            method: "POST",
            url_contains: &[BASE, "/getChatMember"],
            body_eq: ptrs([("/chat_id", json!(CHAT_ID)), ("/user_id", json!(USER_ID))]),
            response_status: 200,
            response: json!({"ok": true, "result": {
                "status": "administrator",
                "user": {"id": 11111111, "is_bot": false, "first_name": "Ada",
                         "last_name": "Lovelace", "username": "ada"},
                "can_be_edited": false, "is_anonymous": false,
                "can_manage_chat": true, "can_delete_messages": true,
                "can_manage_video_chats": true, "can_restrict_members": true,
                "can_promote_members": false, "can_change_info": true,
                "can_invite_users": true, "can_post_messages": true,
                "can_edit_messages": true, "can_pin_messages": true,
                "can_manage_topics": true}}),
            result_eq: ptrs([
                ("/result/status", json!("administrator")),
                ("/result/user/username", json!("ada")),
                ("/result/can_pin_messages", json!(true)),
            ]),
        },
        Case {
            tool: "TELEGRAM_GET_CHAT_MEMBER_COUNT",
            args: json!({"chat_id": "@acmeeng"}),
            method: "POST",
            url_contains: &[BASE, "/getChatMemberCount"],
            body_eq: ptrs([("/chat_id", json!("@acmeeng"))]),
            response_status: 200,
            response: json!({"ok": true, "result": 412}),
            result_eq: ptrs([("/ok", json!(true)), ("/result", json!(412))]),
        },
        Case {
            tool: "TELEGRAM_GET_CHAT_ADMINISTRATORS",
            args: json!({"chat_id": CHAT_ID}),
            method: "POST",
            url_contains: &[BASE, "/getChatAdministrators"],
            body_eq: ptrs([("/chat_id", json!(CHAT_ID))]),
            response_status: 200,
            response: json!({"ok": true, "result": [
                {"status": "creator",
                 "user": {"id": 11111111, "is_bot": false, "first_name": "Ada", "username": "ada"},
                 "can_be_edited": false, "is_anonymous": false, "can_manage_chat": true,
                 "can_delete_messages": true, "can_manage_video_chats": true,
                 "can_restrict_members": true, "can_promote_members": false,
                 "can_change_info": true, "can_invite_users": true, "can_post_messages": true,
                 "can_edit_messages": true, "can_pin_messages": true, "can_manage_topics": true},
                {"status": "administrator",
                 "user": {"id": 22222222, "is_bot": true, "first_name": "Ops Bot", "username": "opsbot"},
                 "can_be_edited": true, "is_anonymous": false, "can_manage_chat": false,
                 "can_delete_messages": true, "can_manage_video_chats": true,
                 "can_restrict_members": false, "can_promote_members": false,
                 "can_change_info": false, "can_invite_users": true,
                 "can_post_messages": true, "can_edit_messages": true,
                 "can_pin_messages": true, "can_manage_topics": true}]}),
            result_eq: ptrs([
                ("/count", json!(2)),
                ("/result/0/status", json!("creator")),
                ("/result/1/user/username", json!("opsbot")),
            ]),
        },
        Case {
            tool: "TELEGRAM_GET_FILE",
            args: json!({"file_id": "BQACAgQAAx7f-report-pdf"}),
            method: "POST",
            url_contains: &[BASE, "/getFile"],
            body_eq: ptrs([("/file_id", json!("BQACAgQAAx7f-report-pdf"))]),
            response_status: 200,
            response: json!({"ok": true, "result": {
                "file_id": "BQACAgQAAx7f-report-pdf", "file_unique_id": "AQADAAAD-report",
                "file_size": 204_810, "file_path": "documents/file_7.pdf"}}),
            result_eq: ptrs([
                ("/result/file_id", json!("BQACAgQAAx7f-report-pdf")),
                ("/result/file_path", json!("documents/file_7.pdf")),
                ("/file_url", json!("https://telegram.test/file/bot700000:AAHrecordedtoken/documents/file_7.pdf")),
            ]),
        },
        Case {
            tool: "TELEGRAM_SEND_MESSAGE",
            args: json!({"chat_id": CHAT_ID, "message": "Deployment *4.2* finished.",
                         "format": "MarkdownV2", "message_thread_id": "42",
                         "disable_web_page_preview": true, "disable_notification": true,
                         "protect_content": true, "reply_to_message_id": 8_812,
                         "reply_markup": {"inline_keyboard": [[{"text": "Release notes",
                                                                  "url": "https://acme.example.com/4.2"}]]}}),
            method: "POST",
            url_contains: &[BASE, "/sendMessage"],
            body_eq: ptrs([
                ("/chat_id", json!(CHAT_ID)),
                ("/text", json!("Deployment *4.2* finished.")),
                ("/parse_mode", json!("MarkdownV2")),
                ("/message_thread_id", json!("42")),
                ("/disable_web_page_preview", json!(true)),
                ("/disable_notification", json!(true)),
                ("/protect_content", json!(true)),
                ("/reply_to_message_id", json!(8_812)),
                ("/reply_markup/inline_keyboard/0/0/text", json!("Release notes")),
            ]),
            response_status: 200,
            response: json!({"ok": true, "result": message(8_813, chat())}),
            result_eq: ptrs([
                ("/result/message_id", json!(8_813)),
                ("/result/chat/title", json!("Acme Engineering")),
            ]),
        },
        Case {
            tool: "TELEGRAM_EDIT_MESSAGE_TEXT",
            args: json!({"chat_id": CHAT_ID, "message_id": "8813", "text": "Deployment 4.2 rolled back.",
                         "format": "HTML", "reply_markup": {"inline_keyboard": [[{"text": "Status",
                                                                                "url": "https://acme.example.com/4.2/status"}]]}}),
            method: "POST",
            url_contains: &[BASE, "/editMessageText"],
            body_eq: ptrs([
                ("/chat_id", json!(CHAT_ID)),
                ("/message_id", json!("8813")),
                ("/text", json!("Deployment 4.2 rolled back.")),
                ("/parse_mode", json!("HTML")),
                ("/disable_web_page_preview", json!(false)),
            ]),
            response_status: 200,
            response: json!({"ok": true, "result":
                {"message_id": 8_813, "date": 1_791_979_800, "chat": chat(),
                 "edit_date": 1_791_979_800, "text": "Deployment 4.2 rolled back."}}),
            result_eq: ptrs([
                ("/result/text", json!("Deployment 4.2 rolled back.")),
                ("/result/edit_date", json!(1_791_979_800)),
            ]),
        },
        Case {
            tool: "TELEGRAM_DELETE_MESSAGE",
            args: json!({"chat_id": CHAT_ID, "message_id": "8813"}),
            method: "POST",
            url_contains: &[BASE, "/deleteMessage"],
            body_eq: ptrs([("/chat_id", json!(CHAT_ID)), ("/message_id", json!("8813"))]),
            response_status: 200,
            response: json!({"ok": true, "result": true}),
            result_eq: ptrs([("/ok", json!(true)), ("/result", json!(true))]),
        },
        Case {
            tool: "TELEGRAM_FORWARD_MESSAGE",
            args: json!({"chat_id": "-1009998887776", "from_chat_id": CHAT_ID, "message_id": "8801",
                         "message_thread_id": "7", "disable_notification": true,
                         "protect_content": true}),
            method: "POST",
            url_contains: &[BASE, "/forwardMessage"],
            body_eq: ptrs([
                ("/chat_id", json!("-1009998887776")),
                ("/from_chat_id", json!(CHAT_ID)),
                ("/message_id", json!("8801")),
                ("/message_thread_id", json!("7")),
                ("/disable_notification", json!(true)),
                ("/protect_content", json!(true)),
            ]),
            response_status: 200,
            response: json!({"ok": true, "result": {
                "message_id": 4_204, "date": 1_791_980_400, "chat": {"id": -1009998887776_i64, "type": "channel", "title": "Acme Ops"},
                "from_chat_id": -1001234567890_i64,
                "forward_origin": {"type": "user", "sender_user": {"id": 11111111, "first_name": "Ada", "username": "ada"},
                                   "date": 1_791_979_200},
                "text": "Deployment 4.2 finished."}}),
            result_eq: ptrs([
                ("/result/message_id", json!(4_204)),
                ("/result/forward_origin/sender_user/username", json!("ada")),
            ]),
        },
        Case {
            tool: "TELEGRAM_PIN_MESSAGE",
            args: json!({"chat_id": CHAT_ID, "message_id": "8813", "disable_notification": true}),
            method: "POST",
            url_contains: &[BASE, "/pinChatMessage"],
            body_eq: ptrs([
                ("/chat_id", json!(CHAT_ID)),
                ("/message_id", json!("8813")),
                ("/disable_notification", json!(true)),
            ]),
            response_status: 200,
            response: json!({"ok": true, "result": true}),
            result_eq: ptrs([("/result", json!(true))]),
        },
        Case {
            tool: "TELEGRAM_UNPIN_MESSAGE",
            args: json!({"chat_id": CHAT_ID}),
            method: "POST",
            url_contains: &[BASE, "/unpinChatMessage"],
            body_eq: ptrs([("/chat_id", json!(CHAT_ID))]),
            response_status: 200,
            response: json!({"ok": true, "result": true}),
            result_eq: ptrs([("/ok", json!(true))]),
        },
        Case {
            tool: "TELEGRAM_SEND_CHAT_ACTION",
            args: json!({"chat_id": CHAT_ID, "action": "upload_document", "message_thread_id": "42"}),
            method: "POST",
            url_contains: &[BASE, "/sendChatAction"],
            body_eq: ptrs([
                ("/chat_id", json!(CHAT_ID)),
                ("/action", json!("upload_document")),
                ("/message_thread_id", json!("42")),
            ]),
            response_status: 200,
            response: json!({"ok": true, "result": true}),
            result_eq: ptrs([("/result", json!(true))]),
        },
        Case {
            tool: "TELEGRAM_SEND_LOCATION",
            args: json!({"chat_id": CHAT_ID, "latitude": 52.5200, "longitude": 13.4050,
                         "horizontal_accuracy": 25, "live_period": 3600, "heading": 90,
                         "proximity_alert_radius": 500, "disable_notification": true,
                         "reply_to_message_id": 8_812,
                         "reply_markup": {"inline_keyboard": [[{"text": "Route", "url": "https://acme.example.com/route"}]]}}),
            method: "POST",
            url_contains: &[BASE, "/sendLocation"],
            body_eq: ptrs([
                ("/chat_id", json!(CHAT_ID)),
                ("/latitude", json!(52.5200)),
                ("/longitude", json!(13.4050)),
                ("/horizontal_accuracy", json!(25)),
                ("/live_period", json!(3_600)),
                ("/heading", json!(90)),
                ("/proximity_alert_radius", json!(500)),
                ("/disable_notification", json!(true)),
                ("/protect_content", json!(false)),
                ("/reply_to_message_id", json!(8_812)),
                ("/reply_markup/inline_keyboard/0/0/url", json!("https://acme.example.com/route")),
            ]),
            response_status: 200,
            response: json!({"ok": true, "result":
                {"message_id": 8_820, "date": 1_791_980_600, "chat": chat(),
                 "location": {"latitude": 52.52, "longitude": 13.405, "live_period": 3600}}}),
            result_eq: ptrs([
                ("/result/message_id", json!(8_820)),
                ("/result/location/live_period", json!(3_600)),
            ]),
        },
        Case {
            tool: "TELEGRAM_SEND_POLL",
            args: json!({"chat_id": CHAT_ID, "question": "Ship release 4.3 on Friday?",
                         "options": ["Yes", "No"], "type": "quiz", "correct_option_id": 0,
                         "explanation": "The regression suite is green.",
                         "explanation_format": "MarkdownV2", "open_period": 600,
                         "disable_notification": true}),
            method: "POST",
            url_contains: &[BASE, "/sendPoll"],
            body_eq: ptrs([
                ("/chat_id", json!(CHAT_ID)),
                ("/question", json!("Ship release 4.3 on Friday?")),
                ("/options", json!(["Yes", "No"])),
                ("/type", json!("quiz")),
                ("/correct_option_id", json!(0)),
                ("/explanation", json!("The regression suite is green.")),
                ("/explanation_parse_mode", json!("MarkdownV2")),
                ("/open_period", json!(600)),
                ("/is_anonymous", json!(true)),
                ("/allows_multiple_answers", json!(false)),
                ("/is_closed", json!(false)),
                ("/disable_notification", json!(true)),
            ]),
            response_status: 200,
            response: json!({"ok": true, "result":
                {"message_id": 8_830, "date": 1_791_981_000, "chat": chat(),
                 "poll": {"id": "5301", "question": "Ship release 4.3 on Friday?",
                          "options": [{"text": "Yes", "voter_count": 4},
                                      {"text": "No", "voter_count": 1}],
                          "total_voter_count": 5, "is_closed": false, "is_anonymous": true,
                          "type": "quiz", "allows_multiple_answers": false, "correct_option_id": 0}}}),
            result_eq: ptrs([
                ("/result/poll/question", json!("Ship release 4.3 on Friday?")),
                ("/result/poll/correct_option_id", json!(0)),
                ("/result/poll/total_voter_count", json!(5)),
            ]),
        },
        Case {
            tool: "TELEGRAM_ANSWER_CALLBACK_QUERY",
            args: json!({"callback_query_id": "4382047:AAFq", "text": "Saved.",
                         "show_alert": true, "url": "https://acme.example.com/incidents/91",
                         "cache_time": 30}),
            method: "POST",
            url_contains: &[BASE, "/answerCallbackQuery"],
            body_eq: ptrs([
                ("/callback_query_id", json!("4382047:AAFq")),
                ("/text", json!("Saved.")),
                ("/show_alert", json!(true)),
                ("/url", json!("https://acme.example.com/incidents/91")),
                ("/cache_time", json!(30)),
            ]),
            response_status: 200,
            response: json!({"ok": true, "result": true}),
            result_eq: ptrs([("/result", json!(true))]),
        },
        Case {
            tool: "TELEGRAM_CREATE_INVITE_LINK",
            args: json!({"chat_id": CHAT_ID, "name": "release-4.3",
                         "expire_date": "2026-10-21T09:00:00Z", "member_limit": 50,
                         "creates_join_request": true}),
            method: "POST",
            url_contains: &[BASE, "/createChatInviteLink"],
            body_eq: ptrs([
                ("/chat_id", json!(CHAT_ID)),
                ("/name", json!("release-4.3")),
                ("/expire_date", json!(1_792_573_200)),
                ("/member_limit", json!(50)),
                ("/creates_join_request", json!(true)),
            ]),
            response_status: 200,
            response: json!({"ok": true, "result": {
                "invite_link": "https://t.me/+AcmeRelease43",
                "creator": {"id": 700000, "is_bot": true, "first_name": "Acme", "username": "acmebot"},
                "creates_join_request": true, "is_primary": false, "is_revoked": false,
                "name": "release-4.3", "expire_date": 1_792_573_200, "member_limit": 50}}),
            result_eq: ptrs([
                ("/result/invite_link", json!("https://t.me/+AcmeRelease43")),
                ("/result/expire_date", json!(1_792_573_200)),
                ("/result/member_limit", json!(50)),
            ]),
        },
    ];
    support::assert_full_coverage(&a, &cases);
    for c in &cases {
        run(&a, c);
    }
}

#[test]
fn api_error_envelopes_surface_as_tool_errors() {
    use ctox_toolhub::http::HttpResponse;
    use ctox_toolhub::toolkits::ToolkitAdapter;

    let a = adapter();
    let resp = HttpResponse {
        status: 400,
        body: serde_json::to_vec(
            &json!({"ok": false, "error_code": 400, "description": "Bad Request: chat not found"}),
        )
        .unwrap(),
        session_id: None,
    };
    let err = a
        .parse_response("TELEGRAM_GET_CHAT", &resp)
        .unwrap_err()
        .to_string();
    assert!(err.contains("chat not found"), "{err}");
}

/// Live read-only calls through the hub exec path (bot token).
#[test]
#[ignore]
fn live_read_only_actions() {
    let a = adapter();
    let mut calls = vec![("TELEGRAM_GET_BOT_INFO", json!({}))];
    // A chat-scoped call needs a chat the bot can reach.
    if let Ok(chat_id) = std::env::var("CTOX_TOOLHUB_LIVE_TELEGRAM_CHAT_ID") {
        calls.push(("TELEGRAM_GET_CHAT", json!({"chat_id": chat_id.clone()})));
        calls.push(("TELEGRAM_GET_CHAT_MEMBER_COUNT", json!({"chat_id": chat_id})));
    }
    for (tool, args) in calls {
        let Some(result) = live(&a, tool, args) else { return };
        let v = result.unwrap_or_else(|e| panic!("{tool}: {e}"));
        assert!(v.is_object(), "{tool}: expected an object, got {v}");
    }
}