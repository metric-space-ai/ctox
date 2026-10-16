//! Twilio port: one recorded case per tool. Every request expectation is taken
//! from the Activepieces source — the four actions in `src/lib/action/*.ts`, the
//! shared `callTwilioApi` / `fetchAllIncomingPhoneNumbers` helper in
//! `src/lib/common/index.ts` and the resource URIs the piece itself polls or
//! ships as sample data in `src/lib/trigger/*.ts`.
mod support;

use ctox_toolhub::toolkits::twilio::Twilio;
use ctox_toolhub::toolkits::ToolkitAdapter;
use serde_json::{json, Value};
use support::{run, Case};

const API: &str = "https://api.twilio.test/2010-04-01";
const LOOKUP: &str = "https://lookups.twilio.test/v2";

const ACCOUNT: &str = "ACc0ea716d61fe90d78a123a3de71d45619";

// `url_contains` needs `&'static [&'static str]`, so the expectations live in consts.
const U_GET_MESSAGE: &[&str] = &[
    API,
    "/Accounts/",
    ACCOUNT,
    "/Messages/SM8c3920d3f2ac481ba83e639a69dadd63.json",
];
const U_LIST_MESSAGES: &[&str] = &["Messages.json?PageSize=20", "To=%2B15017122661"];
const U_MESSAGE_MEDIA: &[&str] = &["/Messages/SM8c3920d3f2ac481ba83e639a69dadd63/Media.json"];
const U_GET_CALL: &[&str] = &["/Accounts/", ACCOUNT, "/Calls/CAaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa.json"];
const U_LIST_CALLS: &[&str] = &["Calls.json?PageSize=10", "Status=completed"];
const U_GET_RECORDING: &[&str] =
    &["/Accounts/", ACCOUNT, "/Recordings/REaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa.json"];
const U_LIST_RECORDINGS: &[&str] = &["Recordings.json?PageSize=10"];
const U_DOWNLOAD: &[&str] = &[
    "/Recordings/REaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa.mp3",
    "RequestedChannels=2",
];
const U_GET_TRANSCRIPTION: &[&str] = &[
    "/Accounts/",
    ACCOUNT,
    "/Transcriptions/TRaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa.json",
];
const U_LIST_TRANSCRIPTIONS: &[&str] = &["Transcriptions.json?PageSize=10"];
const U_LIST_NUMBERS: &[&str] = &["IncomingPhoneNumbers.json?PageSize=1000"];
const U_GET_NUMBER: &[&str] = &[
    "/Accounts/",
    ACCOUNT,
    "/IncomingPhoneNumbers/PNXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXX.json",
];
const U_LOOKUP: &[&str] = &[
    LOOKUP,
    "/PhoneNumbers/%2B15558675310",
    "Fields=line_type_intelligence",
];
const U_SEND_SMS: &[&str] = &["/Accounts/", ACCOUNT, "/Messages.json"];
const U_MAKE_CALL: &[&str] = &["/Accounts/", ACCOUNT, "/Calls.json"];
const MESSAGE: &str = "SM8c3920d3f2ac481ba83e639a69dadd63";
const CALL: &str = "CAaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
const RECORDING: &str = "REaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
const TRANSCRIPTION: &str = "TRaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
const PHONE_NUMBER_SID: &str = "PNXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXX";

fn adapter() -> Twilio {
    Twilio::with_bases(API, LOOKUP)
}

/// `&[(ptr, value)]` for a `Case` field: leaked so it lives as long as the case.
fn ptrs<const N: usize>(pairs: [(&'static str, Value); N]) -> &'static [(&'static str, Value)] {
    Box::leak(Box::new(pairs))
}

/// Recorded Twilio message resource (`sampleData` of `new-incoming-sms.ts`).
fn message(sid: &str) -> Value {
    json!({
        "account_sid": ACCOUNT, "api_version": "2010-04-01", "body": "Your order has shipped!",
        "num_segments": "1", "direction": "outbound-api", "from": "+15017122661",
        "to": "+15558675310", "date_updated": "Wed, 08 Feb 2023 01:40:51 +0000",
        "date_created": "Wed, 08 Feb 2023 01:40:50 +0000", "date_sent": "Wed, 08 Feb 2023 01:40:51 +0000",
        "error_code": null, "error_message": null, "price": "-0.00750", "price_unit": "USD",
        "status": "delivered", "num_media": "0", "sid": sid,
        "messaging_service_sid": "MG88e323e6a88ce67ba3bf12e1bcb7e0b8",
        "uri": format!("/2010-04-01/Accounts/{ACCOUNT}/Messages/{sid}.json"),
        "subresource_uris": {
            "media": format!("/2010-04-01/Accounts/{ACCOUNT}/Messages/{sid}/Media.json")
        }
    })
}

#[test]
fn twilio_actions_are_ported() {
    let a = adapter();
    let cases: Vec<Case> = vec![
        Case {
            tool: "TWILIO_GET_MESSAGE",
            args: json!({"account_sid": ACCOUNT, "message_sid": MESSAGE}),
            method: "GET",
            url_contains: U_GET_MESSAGE,
            body_eq: ptrs([]),
            response_status: 200,
            response: message(MESSAGE),
            result_eq: ptrs([
                ("/sid", json!(MESSAGE)),
                ("/status", json!("delivered")),
                ("/to", json!("+15558675310")),
                ("/num_segments", json!("1")),
            ]),
        },
        Case {
            tool: "TWILIO_LIST_MESSAGES",
            args: json!({"account_sid": ACCOUNT, "page_size": 20, "to": "+15017122661"}),
            method: "GET",
            url_contains: U_LIST_MESSAGES,
            body_eq: ptrs([]),
            response_status: 200,
            response: json!({
                "first_page_uri": format!("/2010-04-01/Accounts/{ACCOUNT}/Messages.json?PageSize=20"),
                "next_page_uri": null, "page": 0, "page_size": 20,
                "uri": format!("/2010-04-01/Accounts/{ACCOUNT}/Messages.json"),
                "messages": [message(MESSAGE)]
            }),
            result_eq: ptrs([
                ("/count", json!(1)),
                ("/items/0/sid", json!(MESSAGE)),
                ("/items/0/direction", json!("outbound-api")),
                ("/next_page_uri", Value::Null),
                ("/page_size", json!(20)),
            ]),
        },
        Case {
            tool: "TWILIO_LIST_MESSAGE_MEDIA",
            args: json!({"account_sid": ACCOUNT, "message_sid": MESSAGE}),
            method: "GET",
            url_contains: U_MESSAGE_MEDIA,
            body_eq: ptrs([]),
            response_status: 200,
            response: json!({
                "first_page_uri": format!("/2010-04-01/Accounts/{ACCOUNT}/Messages/{MESSAGE}/Media.json"),
                "next_page_uri": null, "page": 0, "page_size": 50,
                "uri": format!("/2010-04-01/Accounts/{ACCOUNT}/Messages/{MESSAGE}/Media.json"),
                "media": [{
                    "sid": "MEaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
                    "account_sid": ACCOUNT, "content_type": "image/jpeg",
                    "filename": "photo.jpg", "size": 20481,
                    "date_created": "Wed, 08 Feb 2023 01:40:51 +0000",
                    "uri": format!("/2010-04-01/Accounts/{ACCOUNT}/Messages/{MESSAGE}/Media/MEaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa.json")
                }]
            }),
            result_eq: ptrs([
                ("/count", json!(1)),
                ("/items/0/content_type", json!("image/jpeg")),
                ("/items/0/size", json!(20481)),
            ]),
        },
        Case {
            tool: "TWILIO_GET_CALL",
            args: json!({"account_sid": ACCOUNT, "call_sid": CALL}),
            method: "GET",
            url_contains: U_GET_CALL,
            body_eq: ptrs([]),
            response_status: 200,
            response: json!({
                "account_sid": ACCOUNT, "sid": CALL, "status": "completed",
                "direction": "outbound-api", "from": "+13051416799", "to": "+13051913581",
                "from_formatted": "(305) 141-6799", "to_formatted": "(305) 191-3581",
                "start_time": "Fri, 18 Oct 2019 17:02:00 +0000",
                "end_time": "Fri, 18 Oct 2019 17:03:00 +0000",
                "duration": "4", "price": "-0.200", "price_unit": "USD",
                "answered_by": "machine_start",
                "uri": format!("/2010-04-01/Accounts/{ACCOUNT}/Calls/{CALL}.json"),
                "subresource_uris": {
                    "recordings": format!("/2010-04-01/Accounts/{ACCOUNT}/Calls/{CALL}/Recordings.json")
                }
            }),
            result_eq: ptrs([
                ("/sid", json!(CALL)),
                ("/status", json!("completed")),
                ("/duration", json!("4")),
                ("/subresource_uris/recordings", json!(format!("/2010-04-01/Accounts/{ACCOUNT}/Calls/{CALL}/Recordings.json"))),
            ]),
        },
        Case {
            tool: "TWILIO_LIST_CALLS",
            args: json!({"account_sid": ACCOUNT, "page_size": 10, "status": "completed"}),
            method: "GET",
            url_contains: U_LIST_CALLS,
            body_eq: ptrs([]),
            response_status: 200,
            response: json!({
                "calls": [{"sid": CALL, "status": "completed", "direction": "outbound-api",
                           "from": "+13051416799", "to": "+13051913581", "duration": "4",
                           "date_created": "Fri, 18 Oct 2019 17:00:00 +0000",
                           "price": "-0.200", "price_unit": "USD"}],
                "next_page_uri": null, "page": 0, "page_size": 10
            }),
            result_eq: ptrs([
                ("/count", json!(1)),
                ("/items/0/sid", json!(CALL)),
                ("/items/0/status", json!("completed")),
            ]),
        },
        Case {
            tool: "TWILIO_GET_RECORDING",
            args: json!({"account_sid": ACCOUNT, "recording_sid": RECORDING}),
            method: "GET",
            url_contains: U_GET_RECORDING,
            body_eq: ptrs([]),
            response_status: 200,
            response: json!({
                "account_sid": ACCOUNT, "sid": RECORDING, "call_sid": CALL, "status": "completed",
                "channels": 1, "source": "DialVerb", "duration": "4",
                "start_time": "Fri, 14 Oct 2016 21:56:34 +0000",
                "date_created": "Fri, 14 Oct 2016 21:56:34 +0000",
                "date_updated": "Fri, 14 Oct 2016 21:56:38 +0000",
                "price": "0.04", "price_unit": "USD", "error_code": null,
                "media_url": format!("http://api.twilio.com/2010-04-01/Accounts/{ACCOUNT}/Recordings/{RECORDING}"),
                "uri": format!("/2010-04-01/Accounts/{ACCOUNT}/Recordings/{RECORDING}.json"),
                "subresource_uris": {
                    "transcriptions": format!("/2010-04-01/Accounts/{ACCOUNT}/Recordings/{RECORDING}/Transcriptions.json")
                }
            }),
            result_eq: ptrs([
                ("/sid", json!(RECORDING)),
                ("/duration", json!("4")),
                ("/channels", json!(1)),
                ("/media_url", json!(format!("http://api.twilio.com/2010-04-01/Accounts/{ACCOUNT}/Recordings/{RECORDING}"))),
            ]),
        },
        Case {
            tool: "TWILIO_LIST_RECORDINGS",
            args: json!({"account_sid": ACCOUNT, "page_size": 10}),
            method: "GET",
            url_contains: U_LIST_RECORDINGS,
            body_eq: ptrs([]),
            response_status: 200,
            response: json!({
                "recordings": [{"sid": RECORDING, "call_sid": CALL, "status": "completed",
                                "duration": "4", "source": "DialVerb", "channels": 1,
                                "date_created": "Fri, 14 Oct 2016 21:56:34 +0000"}],
                "next_page_uri": null, "page": 0, "page_size": 10
            }),
            result_eq: ptrs([
                ("/count", json!(1)),
                ("/items/0/call_sid", json!(CALL)),
            ]),
        },
        Case {
            tool: "TWILIO_DOWNLOAD_RECORDING_MEDIA",
            args: json!({"account_sid": ACCOUNT, "recording_sid": RECORDING, "format": "mp3", "channels": 2}),
            method: "GET",
            url_contains: U_DOWNLOAD,
            body_eq: ptrs([]),
            response_status: 200,
            // The piece receives raw audio here and writes it to its file
            // store; the harness can only replay JSON bodies, so this case
            // asserts the request and the returned status. The byte-format
            // sniffing is covered by the unit test in src/toolkits/twilio.rs
            // with a real MP3 header.
            response: json!({"note": "binary audio payload"}),
            result_eq: ptrs([("/status", json!(200)), ("/detected_format", json!("unknown"))]),
        },
        Case {
            tool: "TWILIO_GET_TRANSCRIPTION",
            args: json!({"account_sid": ACCOUNT, "transcription_sid": TRANSCRIPTION}),
            method: "GET",
            url_contains: U_GET_TRANSCRIPTION,
            body_eq: ptrs([]),
            response_status: 200,
            response: json!({
                "account_sid": ACCOUNT, "sid": TRANSCRIPTION, "status": "completed",
                "recording_sid": RECORDING, "duration": "10", "price": "0.00000",
                "price_unit": "USD", "type": "fast", "api_version": "2008-08-01",
                "date_created": "Thu, 25 Aug 2011 20:59:45 +0000",
                "transcription_text": "Hi, this is Sam. Please call me back about my order.",
                "uri": format!("/2010-04-01/Accounts/{ACCOUNT}/Transcriptions/{TRANSCRIPTION}.json")
            }),
            result_eq: ptrs([
                ("/status", json!("completed")),
                ("/recording_sid", json!(RECORDING)),
                ("/transcription_text", json!("Hi, this is Sam. Please call me back about my order.")),
            ]),
        },
        Case {
            tool: "TWILIO_LIST_TRANSCRIPTIONS",
            args: json!({"account_sid": ACCOUNT, "page_size": 10}),
            method: "GET",
            url_contains: U_LIST_TRANSCRIPTIONS,
            body_eq: ptrs([]),
            response_status: 200,
            response: json!({
                "transcriptions": [{"sid": TRANSCRIPTION, "status": "completed",
                                    "recording_sid": RECORDING, "duration": "10",
                                    "transcription_text": "Hi, this is Sam."}],
                "next_page_uri": null, "page": 0, "page_size": 10
            }),
            result_eq: ptrs([
                ("/count", json!(1)),
                ("/items/0/recording_sid", json!(RECORDING)),
            ]),
        },
        Case {
            tool: "TWILIO_LIST_INCOMING_PHONE_NUMBERS",
            args: json!({"account_sid": ACCOUNT, "page_size": 1000}),
            method: "GET",
            url_contains: U_LIST_NUMBERS,
            body_eq: ptrs([]),
            response_status: 200,
            response: json!({
                "incoming_phone_numbers": [{
                    "sid": PHONE_NUMBER_SID, "account_sid": ACCOUNT,
                    "friendly_name": "(555) 123-4567", "phone_number": "+15551234567",
                    "voice_url": "https://demo.twilio.com/welcome/voice/", "voice_method": "POST",
                    "sms_url": "https://demo.twilio.com/welcome/sms/", "sms_method": "POST",
                    "date_created": "Thu, 28 Aug 2025 11:44:10 +0000",
                    "date_updated": "Thu, 28 Aug 2025 11:44:10 +0000",
                    "capabilities": {"voice": true, "sms": true, "mms": true},
                    "uri": format!("/2010-04-01/Accounts/{ACCOUNT}/IncomingPhoneNumbers/{PHONE_NUMBER_SID}.json")
                }],
                "next_page_uri": null, "page": 0, "page_size": 1000
            }),
            result_eq: ptrs([
                ("/count", json!(1)),
                ("/items/0/phone_number", json!("+15551234567")),
                ("/items/0/capabilities/mms", json!(true)),
            ]),
        },
        Case {
            tool: "TWILIO_GET_INCOMING_PHONE_NUMBER",
            args: json!({"account_sid": ACCOUNT, "phone_number_sid": PHONE_NUMBER_SID}),
            method: "GET",
            url_contains: U_GET_NUMBER,
            body_eq: ptrs([]),
            response_status: 200,
            response: json!({
                "sid": PHONE_NUMBER_SID, "account_sid": ACCOUNT,
                "friendly_name": "(555) 123-4567", "phone_number": "+15551234567",
                "voice_url": "https://demo.twilio.com/welcome/voice/", "voice_method": "POST",
                "sms_url": "https://demo.twilio.com/welcome/sms/", "sms_method": "POST",
                "date_created": "Thu, 28 Aug 2025 11:44:10 +0000",
                "date_updated": "Thu, 28 Aug 2025 11:44:10 +0000",
                "capabilities": {"voice": true, "sms": true, "mms": true},
                "uri": format!("/2010-04-01/Accounts/{ACCOUNT}/IncomingPhoneNumbers/{PHONE_NUMBER_SID}.json")
            }),
            result_eq: ptrs([
                ("/phone_number", json!("+15551234567")),
                ("/capabilities/voice", json!(true)),
            ]),
        },
        Case {
            tool: "TWILIO_PHONE_NUMBER_LOOKUP",
            args: json!({"phone_number": "+15558675310"}),
            method: "GET",
            url_contains: U_LOOKUP,
            body_eq: ptrs([]),
            response_status: 200,
            response: json!({
                "caller_name": null,
                "phone_number": "+15558675310",
                "national_format": "(555) 867-5310",
                "carrier": {"name": "Verizon Wireless", "mobile_country_code": "310",
                            "mobile_network_code": "012"},
                "add_ons": {"status": "successful"},
                "url": "https://lookups.twilio.com/v2/PhoneNumbers/+15558675310"
            }),
            result_eq: ptrs([
                ("/phone_number", json!("+15558675310")),
                ("/national_format", json!("(555) 867-5310")),
            ]),
        },
        Case {
            tool: "TWILIO_SEND_SMS",
            args: json!({"account_sid": ACCOUNT, "from": "+15017122661", "to": "+15558675310",
                         "body": "Your order has shipped!"}),
            method: "POST",
            url_contains: U_SEND_SMS,
            body_eq: ptrs([]),
            response_status: 201,
            response: message(MESSAGE),
            result_eq: ptrs([
                ("/sid", json!(MESSAGE)),
                ("/body", json!("Your order has shipped!")),
                ("/status", json!("delivered")),
            ]),
        },
        Case {
            tool: "TWILIO_MAKE_CALL",
            args: json!({"account_sid": ACCOUNT, "from": "+15017122661", "to": "+15558675310",
                         "message": "Hi, this is a reminder about your appointment tomorrow.",
                         "voice": "alice", "language": "en-US", "send_digits": "W1234#", "timeout": 60}),
            method: "POST",
            url_contains: U_MAKE_CALL,
            body_eq: ptrs([]),
            response_status: 201,
            response: json!({
                "account_sid": ACCOUNT, "sid": CALL, "status": "queued",
                "direction": "outbound-api", "from": "+15017122661", "to": "+15558675310",
                "date_created": "Fri, 18 Oct 2019 17:00:00 +0000", "queue_time": "60",
                "uri": format!("/2010-04-01/Accounts/{ACCOUNT}/Calls/{CALL}.json")
            }),
            result_eq: ptrs([
                ("/sid", json!(CALL)),
                ("/status", json!("queued")),
                ("/queue_time", json!("60")),
            ]),
        },
    ];
    support::assert_full_coverage(&a, &cases);
    for c in &cases {
        run(&a, c);
    }
}

/// The two form-encoded writes must send exactly the fields the piece sends
/// (`callTwilioApi` posts `application/x-www-form-urlencoded`).
#[test]
fn write_actions_post_the_piece_form_fields() {
    let a = adapter();
    let r = a
        .build_request(
            "TWILIO_SEND_SMS",
            &json!({"account_sid": ACCOUNT, "from": "+15017122661", "to": "+15558675310",
                    "body": "Your order has shipped!"}),
        )
        .unwrap();
    assert_eq!(r.method, "POST");
    assert_eq!(r.url, format!("{API}/Accounts/{ACCOUNT}/Messages.json"));
    assert_eq!(
        r.headers.get("content-type").map(String::as_str),
        Some("application/x-www-form-urlencoded")
    );
    let body = String::from_utf8(r.body.expect("form body")).unwrap();
    assert_eq!(
        body,
        "From=%2B15017122661&To=%2B15558675310&Body=Your+order+has+shipped%21"
    );

    let r = a
        .build_request(
            "TWILIO_MAKE_CALL",
            &json!({"account_sid": ACCOUNT, "from": "+15017122661", "to": "+15558675310",
                    "message": "Hi <there> & \"bye\"", "voice": "alice", "language": "de-DE",
                    "send_digits": "W1234#", "timeout": 30}),
        )
        .unwrap();
    assert_eq!(r.url, format!("{API}/Accounts/{ACCOUNT}/Calls.json"));
    let body = String::from_utf8(r.body.expect("form body")).unwrap();
    // `<Say voice=… language=…>` with the piece's own XML escaping.
    assert!(body.contains("Twiml=%3CResponse%3E%3CSay+voice%3D%22alice%22+language%3D%22de-DE%22%3E"), "{body}");
    assert!(body.contains("Hi+%26lt%3Bthere%26gt%3B+%26amp%3B+%26quot%3Bbye%26quot%3B%3C%2FSay%3E%3C%2FResponse%3E"), "{body}");
    assert!(body.contains("SendDigits=W1234%23"), "{body}");
    assert!(body.contains("Timeout=30"), "{body}");
}

/// Path/query injection attempts are refused before a request is built.
#[test]
fn ids_and_numbers_are_validated() {
    let a = adapter();
    assert!(a
        .build_request("TWILIO_GET_MESSAGE", &json!({"account_sid": "AC1/../x", "message_sid": MESSAGE}))
        .is_err());
    assert!(a
        .build_request("TWILIO_GET_CALL", &json!({"account_sid": ACCOUNT}))
        .is_err());
    assert!(a
        .build_request("TWILIO_GET_RECORDING", &json!({"account_sid": ACCOUNT, "recording_sid": "RE 1/x"}))
        .is_err());
    assert!(a
        .build_request(
            "TWILIO_DOWNLOAD_RECORDING_MEDIA",
            &json!({"account_sid": ACCOUNT, "recording_sid": RECORDING, "format": "flac"})
        )
        .is_err());
    assert!(a
        .build_request(
            "TWILIO_MAKE_CALL",
            &json!({"account_sid": ACCOUNT, "from": "+15017122661", "to": "+15558675310",
                    "message": "hi", "voice": "HAL9000"})
        )
        .is_err());
    assert!(a
        .build_request("TWILIO_PHONE_NUMBER_LOOKUP", &json!({"phone_number": "  "}))
        .is_err());
    assert!(a.build_request("TWILIO_LIST_SOMETHING", &json!({"account_sid": ACCOUNT})).is_err());
}

/// Live smoke test: read-only calls against `https://api.twilio.com/2010-04-01`.
/// `CTOX_TOOLHUB_LIVE_TWILIO_TOKEN` must hold the piece's credential as
/// `account_sid:auth_token` (what the Basic scheme base64-encodes); the test is
/// skipped when unset and panics on failure. The tools need the Account SID as
/// an argument, so it is read from the very same env var (never printed).
#[test]
#[ignore]
fn live_read_only() {
    let t = Twilio::default();
    let account_sid = std::env::var("CTOX_TOOLHUB_LIVE_TWILIO_TOKEN")
        .ok()
        .and_then(|c| c.split_once(':').map(|(sid, _)| sid.to_string()))
        .unwrap_or_default();
    for (tool, args) in [
        (
            "TWILIO_LIST_INCOMING_PHONE_NUMBERS",
            json!({"account_sid": account_sid, "page_size": 1}),
        ),
        (
            "TWILIO_LIST_MESSAGES",
            json!({"account_sid": account_sid, "page_size": 1}),
        ),
        (
            "TWILIO_PHONE_NUMBER_LOOKUP",
            json!({"phone_number": "+15558675310"}),
        ),
    ] {
        let spec = t
            .spec()
            .tools
            .iter()
            .find(|s| s.slug == tool)
            .expect("tool in catalog");
        assert!(!spec.mutating, "{tool} must be read-only for the live smoke test");
        // Basic auth: support::live maps an unknown kind to Bearer, which this
        // toolkit does not offer, so the credential runs through the same hub
        // exec path via live_exec with the Basic scheme.
        let Some(result) =
            support::live_exec("twilio", ctox_toolhub::AuthSchemeKind::Basic, tool, args)
        else {
            eprintln!("skip: CTOX_TOOLHUB_LIVE_TWILIO_TOKEN not set");
            return;
        };
        let out = result.expect("live call failed");
        assert!(out.is_object(), "{tool}: expected an object, got {out}");
    }
}