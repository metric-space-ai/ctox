//! Port tests for the WhatsApp Business toolkit.
//! DE: Ein aufgezeichneter Fall pro Aktion, abgeleitet aus dem Activepieces-run()-Code
//! (Meta Graph API v23.0, `WHATSAPP_API_BASE` = https://graph.facebook.com/v23.0).
//! EN: one recorded case per action, derived from the Activepieces run() code.
mod support;

use ctox_toolhub::toolkits::whatsapp::WhatsApp;
use serde_json::{json, Value};
use support::{assert_full_coverage, live, run, Case};

const BASE: &str = "https://graph.facebook.com/v23.0";
const WABA: &str = "102290129340398";
const PHONE_ID: &str = "106540352242922";
const MEDIA_ID: &str = "26903540937491852";
const TEMPLATE_ID: &str = "1394796801931520";
const TO: &str = "4915212345678";
const WAMID: &str = "wamid.HBgLNDkxNTIxMjM0NTY3FQIAERhGNDdBQjMxQ0Q5RTEyMzQ1NgA=";

/// Leak the expectation slices so the `Case`s can borrow them for `'static`.
fn sts(v: Vec<&'static str>) -> &'static [&'static str] {
    Box::leak(v.into_boxed_slice())
}
fn ptrs<const N: usize>(pairs: [(&'static str, Value); N]) -> &'static [(&'static str, Value)] {
    Box::leak(Box::new(pairs))
}

/// Leak a formatted URL fragment (same shape as `format!`) for `url_contains`.
macro_rules! lp {
    ($($arg:tt)*) => {
        Box::leak(format!($($arg)*).into_boxed_str())
    };
}

/// Percent-encode a query value exactly like the shared `query` helper.
fn enc_static(s: &str) -> &'static str {
    Box::leak(url::form_urlencoded::byte_serialize(s.as_bytes()).collect::<String>().into_boxed_str())
}

fn adapter() -> WhatsApp {
    WhatsApp::with_api_base(BASE)
}

/// Recorded envelope of a successful message send.
fn sent(id: &str) -> Value {
    json!({
        "messaging_product": "whatsapp",
        "contacts": [{"input": TO, "wa_id": TO}],
        "messages": [{"id": id, "message_status": "accepted"}]
    })
}

#[test]
fn whatsapp_actions_are_ported() {
    let a = adapter();
    let cases = vec![
        // ---- phone numbers / business profile ------------------------------
        Case {
            tool: "WHATSAPP_GET_PHONE_NUMBER",
            args: json!({"phone_number_id": PHONE_ID}),
            method: "GET",
            url_contains: sts(vec![
                BASE,
                lp!("/{PHONE_ID}?fields="),
                enc_static("id,verified_name,display_phone_number,quality_rating,code_verification_status,name_status,status,throughput,platform_type"),
            ]),
            body_eq: ptrs([]),
            response_status: 200,
            response: json!({
                "id": PHONE_ID, "verified_name": "Acme GmbH",
                "display_phone_number": "+49 30 12345678", "quality_rating": "GREEN",
                "code_verification_status": "VERIFIED", "name_status": "APPROVED",
                "status": "CONNECTED", "throughput": {"level": "TIER_1000"},
                "platform_type": "CLOUD_API"
            }),
            result_eq: ptrs([
                ("/verified_name", json!("Acme GmbH")),
                ("/quality_rating", json!("GREEN")),
                ("/throughput/level", json!("TIER_1000")),
            ]),
        },
        Case {
            tool: "WHATSAPP_LIST_PHONE_NUMBERS",
            args: json!({"business_account_id": WABA, "after": "NDfh"}),
            method: "GET",
            url_contains: sts(vec![
                lp!("/{WABA}/phone_numbers?fields="),
                enc_static("id,verified_name,display_phone_number,quality_rating,code_verification_status,platform_type"),
                "limit=100",
                "after=NDfh",
            ]),
            body_eq: ptrs([]),
            response_status: 200,
            response: json!({
                "data": [{
                    "id": PHONE_ID, "verified_name": "Acme GmbH",
                    "display_phone_number": "+49 30 12345678", "quality_rating": "GREEN",
                    "code_verification_status": "VERIFIED", "platform_type": "CLOUD_API"
                }],
                "paging": {"cursors": {"before": "QWJj", "after": "NDfh"},
                           "next": "https://graph.facebook.com/v23.0/102290129340398/phone_numbers?fields=id&after=NDfh"}
            }),
            result_eq: ptrs([
                ("/count", json!(1)),
                ("/phone_numbers/0/id", json!(PHONE_ID)),
                ("/phone_numbers/0/quality_rating", json!("GREEN")),
                ("/next_after", json!("NDfh")),
            ]),
        },
        Case {
            tool: "WHATSAPP_GET_BUSINESS_PROFILE",
            args: json!({"phone_number_id": PHONE_ID}),
            method: "GET",
            url_contains: sts(vec![
                lp!("/{PHONE_ID}/whatsapp_business_profile?fields="),
                enc_static("about,address,description,email,profile_picture_url,websites,vertical"),
            ]),
            body_eq: ptrs([]),
            response_status: 200,
            response: json!({"data": [{
                "messaging_product": "whatsapp", "about": "Immer für Sie da",
                "address": "Torstraße 1, 10119 Berlin",
                "description": "Werksverkauf und Support",
                "email": "hallo@acme.test",
                "profile_picture_url": "https://lookaside.fbsbx.com/whatsapp/acme.jpg",
                "websites": ["https://acme.test"], "vertical": "RETAIL"}]}),
            result_eq: ptrs([
                ("/about", json!("Immer für Sie da")),
                ("/email", json!("hallo@acme.test")),
                ("/websites/0", json!("https://acme.test")),
                ("/vertical", json!("RETAIL")),
            ]),
        },
        // ---- message templates ---------------------------------------------
        Case {
            tool: "WHATSAPP_LIST_MESSAGE_TEMPLATES",
            args: json!({"business_account_id": WABA, "status": "APPROVED", "limit": 2}),
            method: "GET",
            url_contains: sts(vec![
                lp!("/{WABA}/message_templates?fields="),
                enc_static("id,name,language,category,status,components,quality_score"),
                "limit=2",
                "status=APPROVED",
            ]),
            body_eq: ptrs([]),
            response_status: 200,
            response: json!({
                "data": [{
                    "id": TEMPLATE_ID, "name": "order_update", "language": "de_DE",
                    "category": "UTILITY", "status": "APPROVED",
                    "components": [{"type": "BODY", "text": "Ihre Bestellung {{1}} ist versandt."}],
                    "quality_score": {"score": "GREEN"}
                }],
                "paging": {"cursors": {"after": "NEXT"},
                           "next": "https://graph.facebook.com/v23.0/102290129340398/message_templates?after=NEXT"}
            }),
            result_eq: ptrs([
                ("/count", json!(1)),
                ("/templates/0/name", json!("order_update")),
                ("/templates/0/quality_score/score", json!("GREEN")),
                ("/next_after", json!("NEXT")),
            ]),
        },
        Case {
            tool: "WHATSAPP_GET_TEMPLATE",
            args: json!({"template_id": TEMPLATE_ID}),
            method: "GET",
            url_contains: sts(vec![
                lp!("/{TEMPLATE_ID}?fields="),
                enc_static("id,name,language"),
            ]),
            body_eq: ptrs([]),
            response_status: 200,
            response: json!({"id": TEMPLATE_ID, "name": "order_update", "language": "de_DE"}),
            result_eq: ptrs([("/name", json!("order_update")), ("/language", json!("de_DE"))]),
        },
        Case {
            tool: "WHATSAPP_CREATE_MESSAGE_TEMPLATE",
            args: json!({
                "business_account_id": WABA, "name": "order_update", "language": "de_DE",
                "category": "UTILITY",
                "components": [{"type": "BODY", "text": "Ihre Bestellung [[1]] ist versandt.",
                                "example": {"body_text": [["A-1024"]]}}]
            }),
            method: "POST",
            url_contains: sts(vec![lp!("/{WABA}/message_templates")]),
            body_eq: ptrs([
                ("/name", json!("order_update")),
                ("/language", json!("de_DE")),
                ("/category", json!("UTILITY")),
                ("/components/0/text", json!("Ihre Bestellung {{1}} ist versandt.")),
                ("/components/0/example/body_text/0/0", json!("A-1024")),
            ]),
            response_status: 200,
            response: json!({"id": TEMPLATE_ID, "status": "PENDING", "category": "UTILITY"}),
            result_eq: ptrs([("/id", json!(TEMPLATE_ID)), ("/status", json!("PENDING"))]),
        },
        // Components as a JSON string, with [[name]] placeholders normalized.
        Case {
            tool: "WHATSAPP_CREATE_MESSAGE_TEMPLATE",
            args: json!({
                "business_account_id": WABA, "name": "otp_login", "language": "en_US",
                "category": "AUTHENTICATION",
                "components": "[{\"type\":\"BODY\",\"text\":\"Your code is [[1]].\"}]"
            }),
            method: "POST",
            url_contains: sts(vec![lp!("/{WABA}/message_templates")]),
            body_eq: ptrs([
                ("/name", json!("otp_login")),
                ("/components/0/text", json!("Your code is {{1}}.")),
            ]),
            response_status: 200,
            response: json!({"id": "1489674321012546", "status": "PENDING", "category": "AUTHENTICATION"}),
            result_eq: ptrs([("/status", json!("PENDING"))]),
        },
        Case {
            tool: "WHATSAPP_EDIT_MESSAGE_TEMPLATE",
            args: json!({
                "message_template_id": TEMPLATE_ID, "category": "MARKETING",
                "components": [{"type": "HEADER", "format": "TEXT", "text": "Acme"},
                               {"type": "BODY", "text": "Bestellung [[1]] versandt."}]
            }),
            method: "POST",
            url_contains: sts(vec![lp!("/{TEMPLATE_ID}")]),
            body_eq: ptrs([
                ("/components/0/type", json!("HEADER")),
                ("/components/1/text", json!("Bestellung {{1}} versandt.")),
                ("/category", json!("MARKETING")),
            ]),
            response_status: 200,
            response: json!({"success": true}),
            result_eq: ptrs([("/success", json!(true))]),
        },
        Case {
            tool: "WHATSAPP_DELETE_MESSAGE_TEMPLATE",
            args: json!({"business_account_id": WABA, "template_id": TEMPLATE_ID,
                         "template_name": "order_update"}),
            method: "DELETE",
            url_contains: sts(vec![
                lp!("/{WABA}/message_templates?"),
                "hsm_id=1394796801931520",
                "name=order_update",
            ]),
            body_eq: ptrs([]),
            response_status: 200,
            response: json!({"success": true}),
            result_eq: ptrs([("/success", json!(true))]),
        },
        // ---- media ----------------------------------------------------------
        Case {
            tool: "WHATSAPP_GET_MEDIA_URL",
            args: json!({"media_id": MEDIA_ID}),
            method: "GET",
            url_contains: sts(vec![lp!("/{MEDIA_ID}")]),
            body_eq: ptrs([]),
            response_status: 200,
            response: json!({
                "messaging_product": "whatsapp",
                "url": "https://lookaside.fbsbx.com/whatsapp/26903540937491852?filename=rechnung.pdf",
                "mime_type": "application/pdf", "sha256": "8fa0a4e21c0d5b7e93aa02ff6d1c47b0",
                "file_size": 303633, "id": MEDIA_ID
            }),
            result_eq: ptrs([
                ("/mime_type", json!("application/pdf")),
                ("/file_size", json!(303633)),
                ("/id", json!(MEDIA_ID)),
            ]),
        },
        Case {
            tool: "WHATSAPP_DELETE_MEDIA",
            args: json!({"media_id": MEDIA_ID}),
            method: "DELETE",
            url_contains: sts(vec![lp!("/{MEDIA_ID}")]),
            body_eq: ptrs([]),
            response_status: 200,
            response: json!({"success": true}),
            result_eq: ptrs([("/success", json!(true))]),
        },
        // ---- sends ----------------------------------------------------------
        Case {
            tool: "WHATSAPP_SEND_MESSAGE",
            args: json!({"phone_number_id": PHONE_ID, "to": TO, "text": "Ihr Paket ist unterwegs."}),
            method: "POST",
            url_contains: sts(vec![lp!("/{PHONE_ID}/messages")]),
            body_eq: ptrs([
                ("/messaging_product", json!("whatsapp")),
                ("/recipient_type", json!("individual")),
                ("/to", json!(TO)),
                ("/type", json!("text")),
                ("/text/body", json!("Ihr Paket ist unterwegs.")),
            ]),
            response_status: 200,
            response: sent("wamid.TEXT1"),
            result_eq: ptrs([
                ("/message_id", json!("wamid.TEXT1")),
                ("/message_status", json!("accepted")),
                ("/wa_id", json!(TO)),
                ("/contacts/0/input", json!(TO)),
            ]),
        },
        Case {
            tool: "WHATSAPP_SEND_MEDIA",
            args: json!({
                "phone_number_id": PHONE_ID, "to": TO, "type": "document",
                "media": "https://cdn.acme.test/rechnung-2026-10.pdf",
                "caption": "Ihre Rechnung", "filename": "rechnung-2026-10.pdf"
            }),
            method: "POST",
            url_contains: sts(vec![lp!("/{PHONE_ID}/messages")]),
            body_eq: ptrs([
                ("/type", json!("document")),
                ("/document/link", json!("https://cdn.acme.test/rechnung-2026-10.pdf")),
                ("/document/caption", json!("Ihre Rechnung")),
                ("/document/filename", json!("rechnung-2026-10.pdf")),
                ("/messaging_product", json!("whatsapp")),
            ]),
            response_status: 200,
            response: sent("wamid.MEDIA1"),
            result_eq: ptrs([("/message_id", json!("wamid.MEDIA1"))]),
        },
        Case {
            tool: "WHATSAPP_SEND_TEMPLATE",
            args: json!({
                "phone_number_id": PHONE_ID, "to": TO, "template_name": "order_update",
                "language_code": "de_DE", "header_parameters": ["Acme"],
                "body_parameters": ["A-1024"], "reply_to_message_id": WAMID
            }),
            method: "POST",
            url_contains: sts(vec![lp!("/{PHONE_ID}/messages")]),
            body_eq: ptrs([
                ("/type", json!("template")),
                ("/template/name", json!("order_update")),
                ("/template/language/code", json!("de_DE")),
                ("/template/components/0/type", json!("header")),
                ("/template/components/0/parameters/0", json!({"type": "text", "text": "Acme"})),
                ("/template/components/1/type", json!("body")),
                ("/template/components/1/parameters/0/text", json!("A-1024")),
                ("/context/message_id", json!(WAMID)),
                ("/to", json!(TO)),
            ]),
            response_status: 200,
            response: sent("wamid.TPL1"),
            result_eq: ptrs([("/message_id", json!("wamid.TPL1")), ("/wa_id", json!(TO))]),
        },
        Case {
            tool: "WHATSAPP_SEND_INTERACTIVE_BUTTONS",
            args: json!({
                "phone_number_id": PHONE_ID, "to": TO, "body": "Termin bestätigen?",
                "buttons": [{"id": "yes", "title": "Ja"}, {"id": "no", "title": "Nein"}],
                "footer": "Bitte antworten", "reply_to_message_id": WAMID
            }),
            method: "POST",
            url_contains: sts(vec![lp!("/{PHONE_ID}/messages")]),
            body_eq: ptrs([
                ("/type", json!("interactive")),
                ("/interactive/type", json!("button")),
                ("/interactive/body/text", json!("Termin bestätigen?")),
                ("/interactive/footer/text", json!("Bitte antworten")),
                ("/interactive/action/buttons/0/type", json!("reply")),
                ("/interactive/action/buttons/0/reply", json!({"id": "yes", "title": "Ja"})),
                ("/interactive/action/buttons/1/reply/id", json!("no")),
                ("/context/message_id", json!(WAMID)),
            ]),
            response_status: 200,
            response: sent("wamid.BTN1"),
            result_eq: ptrs([("/message_id", json!("wamid.BTN1"))]),
        },
        Case {
            tool: "WHATSAPP_SEND_INTERACTIVE_LIST",
            args: json!({
                "phone_number_id": PHONE_ID, "to": TO, "body": "Wählen Sie eine Filiale",
                "button_text": "Filialen", "section_title": "Berlin", "header_text": "Abholung",
                "rows": [{"id": "mitte", "title": "Mitte", "description": "Torstraße 1"},
                         {"id": "koepenick", "title": "Köpenick"}]
            }),
            method: "POST",
            url_contains: sts(vec![lp!("/{PHONE_ID}/messages")]),
            body_eq: ptrs([
                ("/type", json!("interactive")),
                ("/interactive/type", json!("list")),
                ("/interactive/header/text", json!("Abholung")),
                ("/interactive/action/button", json!("Filialen")),
                ("/interactive/action/sections/0/title", json!("Berlin")),
                ("/interactive/action/sections/0/rows/0", json!({"id": "mitte", "title": "Mitte", "description": "Torstraße 1"})),
                ("/interactive/action/sections/0/rows/1", json!({"id": "koepenick", "title": "Köpenick"})),
            ]),
            response_status: 200,
            response: sent("wamid.LIST1"),
            result_eq: ptrs([("/message_id", json!("wamid.LIST1")), ("/wa_id", json!(TO))]),
        },
        Case {
            tool: "WHATSAPP_SEND_INTERACTIVE_CTA_URL",
            args: json!({
                "phone_number_id": PHONE_ID, "to": TO, "body": "Nehmen Sie an der Umfrage teil",
                "button_text": "Umfrage öffnen", "url": "https://acme.test/umfrage"
            }),
            method: "POST",
            url_contains: sts(vec![lp!("/{PHONE_ID}/messages")]),
            body_eq: ptrs([
                ("/type", json!("interactive")),
                ("/interactive/type", json!("cta_url")),
                ("/interactive/body/text", json!("Nehmen Sie an der Umfrage teil")),
                ("/interactive/action/name", json!("cta_url")),
                ("/interactive/action/parameters/display_text", json!("Umfrage öffnen")),
                ("/interactive/action/parameters/url", json!("https://acme.test/umfrage")),
            ]),
            response_status: 200,
            response: sent("wamid.CTA1"),
            result_eq: ptrs([("/message_id", json!("wamid.CTA1"))]),
        },
        Case {
            tool: "WHATSAPP_SEND_LOCATION",
            args: json!({
                "phone_number_id": PHONE_ID, "to": TO, "latitude": 31.9539, "longitude": 35.9106,
                "name": "Acme Werkstatt", "address": "Torstraße 1, 10119 Berlin"
            }),
            method: "POST",
            url_contains: sts(vec![lp!("/{PHONE_ID}/messages")]),
            body_eq: ptrs([
                ("/type", json!("location")),
                ("/location/latitude", json!("31.9539")),
                ("/location/longitude", json!("35.9106")),
                ("/location/name", json!("Acme Werkstatt")),
                ("/location/address", json!("Torstraße 1, 10119 Berlin")),
            ]),
            response_status: 200,
            response: sent("wamid.LOC1"),
            result_eq: ptrs([("/message_id", json!("wamid.LOC1"))]),
        },
        Case {
            tool: "WHATSAPP_SEND_CONTACT",
            args: json!({
                "phone_number_id": PHONE_ID, "to": TO,
                "formatted_name": "Jana Muster", "first_name": "Jana", "last_name": "Muster",
                "phones": [{"phone": "+493012345678", "type": "WORK"}],
                "emails": [{"email": "jana@acme.test", "type": "WORK"}],
                "company": "Acme GmbH", "job_title": "Support", "website": "https://acme.test"
            }),
            method: "POST",
            url_contains: sts(vec![lp!("/{PHONE_ID}/messages")]),
            body_eq: ptrs([
                ("/type", json!("contacts")),
                ("/contacts/0/name/formatted_name", json!("Jana Muster")),
                ("/contacts/0/name/first_name", json!("Jana")),
                ("/contacts/0/name/last_name", json!("Muster")),
                ("/contacts/0/phones/0", json!({"phone": "+493012345678", "type": "WORK"})),
                ("/contacts/0/emails/0", json!({"email": "jana@acme.test", "type": "WORK"})),
                ("/contacts/0/org", json!({"company": "Acme GmbH", "title": "Support"})),
                ("/contacts/0/urls/0", json!({"url": "https://acme.test", "type": "WORK"})),
            ]),
            response_status: 200,
            response: sent("wamid.CONTACT1"),
            result_eq: ptrs([("/message_id", json!("wamid.CONTACT1"))]),
        },
        Case {
            tool: "WHATSAPP_SEND_REACTION",
            args: json!({"phone_number_id": PHONE_ID, "to": TO, "message_id": WAMID, "emoji": "👍"}),
            method: "POST",
            url_contains: sts(vec![lp!("/{PHONE_ID}/messages")]),
            body_eq: ptrs([
                ("/type", json!("reaction")),
                ("/reaction/message_id", json!(WAMID)),
                ("/reaction/emoji", json!("👍")),
            ]),
            response_status: 200,
            response: sent("wamid.REACT1"),
            result_eq: ptrs([("/message_id", json!("wamid.REACT1"))]),
        },
        // An empty emoji removes an existing reaction.
        Case {
            tool: "WHATSAPP_SEND_REACTION",
            args: json!({"phone_number_id": PHONE_ID, "to": TO, "message_id": WAMID}),
            method: "POST",
            url_contains: sts(vec![lp!("/{PHONE_ID}/messages")]),
            body_eq: ptrs([
                ("/reaction/message_id", json!(WAMID)),
                ("/reaction/emoji", json!("")),
            ]),
            response_status: 200,
            response: sent("wamid.REACT2"),
            result_eq: ptrs([("/message_id", json!("wamid.REACT2"))]),
        },
        Case {
            tool: "WHATSAPP_MARK_MESSAGE_AS_READ",
            args: json!({"phone_number_id": PHONE_ID, "message_id": WAMID,
                         "show_typing_indicator": true}),
            method: "POST",
            url_contains: sts(vec![lp!("/{PHONE_ID}/messages")]),
            body_eq: ptrs([
                ("/messaging_product", json!("whatsapp")),
                ("/status", json!("read")),
                ("/message_id", json!(WAMID)),
                ("/typing_indicator/type", json!("text")),
            ]),
            response_status: 200,
            response: json!({"success": true}),
            result_eq: ptrs([("/success", json!(true))]),
        },
        // ---- business profile write ----------------------------------------
        Case {
            tool: "WHATSAPP_UPDATE_BUSINESS_PROFILE",
            args: json!({
                "phone_number_id": PHONE_ID, "about": "Immer für Sie da",
                "description": "Werksverkauf und Support",
                "websites": ["https://acme.test"], "vertical": "RETAIL"
            }),
            method: "POST",
            url_contains: sts(vec![lp!("/{PHONE_ID}/whatsapp_business_profile")]),
            body_eq: ptrs([
                ("/messaging_product", json!("whatsapp")),
                ("/about", json!("Immer für Sie da")),
                ("/description", json!("Werksverkauf und Support")),
                ("/websites", json!(["https://acme.test"])),
                ("/vertical", json!("RETAIL")),
            ]),
            response_status: 200,
            response: json!({"success": true}),
            result_eq: ptrs([("/success", json!(true))]),
        },
    ];
    assert_full_coverage(&a, &cases);
    for c in &cases {
        run(&a, c);
    }
}

#[test]
fn graph_errors_and_bad_requests_surface_as_tool_errors() {
    use ctox_toolhub::http::HttpResponse;
    use ctox_toolhub::toolkits::ToolkitAdapter;

    let a = adapter();
    // Graph error envelope (HTTP 200 variant).
    let resp = HttpResponse {
        status: 200,
        body: serde_json::to_vec(&json!({
            "error": {"message": "Unsupported post request", "type": "GraphMethodException", "code": 100}
        }))
        .unwrap(),
        session_id: None,
    };
    let err = a.parse_response("WHATSAPP_GET_PHONE_NUMBER", &resp).unwrap_err();
    assert!(err.to_string().contains("Unsupported post request"), "{err}");

    // A missing `success` flag is not reported as a confirmed change.
    let resp = HttpResponse {
        status: 200,
        body: serde_json::to_vec(&json!({})).unwrap(),
        session_id: None,
    };
    assert!(a.parse_response("WHATSAPP_DELETE_MEDIA", &resp).is_err());

    // An empty business profile page is an error, not a null result.
    let resp = HttpResponse {
        status: 200,
        body: serde_json::to_vec(&json!({"data": []})).unwrap(),
        session_id: None,
    };
    assert!(a.parse_response("WHATSAPP_GET_BUSINESS_PROFILE", &resp).is_err());

    // Interactive list needs at least one row.
    assert!(a
        .build_request(
            "WHATSAPP_SEND_INTERACTIVE_LIST",
            &json!({"phone_number_id": PHONE_ID, "to": TO, "body": "b",
                    "button_text": "Menü", "section_title": "Berlin", "rows": []})
        )
        .is_err());
    // Templates reject unknown review statuses.
    assert!(a
        .build_request(
            "WHATSAPP_LIST_MESSAGE_TEMPLATES",
            &json!({"business_account_id": WABA, "status": "ARCHIVED"})
        )
        .is_err());
}

/// Live read-only calls through the hub exec path (`Authorization: Bearer` with a
/// Meta System User token). Run:
/// `CTOX_TOOLHUB_LIVE_WHATSAPP_TOKEN=… CTOX_TOOLHUB_LIVE_WHATSAPP_WABA_ID=… \
///  CTOX_TOOLHUB_LIVE_WHATSAPP_PHONE_NUMBER_ID=… cargo test --test port_whatsapp -- --ignored`
#[test]
#[ignore]
fn live_read_only_actions() {
    let a = adapter();
    if std::env::var("CTOX_TOOLHUB_LIVE_WHATSAPP_TOKEN").is_err() {
        // live() would return None; nothing to exercise without a credential.
        return;
    }
    let waba = std::env::var("CTOX_TOOLHUB_LIVE_WHATSAPP_WABA_ID").unwrap_or_default();
    let phone_id = std::env::var("CTOX_TOOLHUB_LIVE_WHATSAPP_PHONE_NUMBER_ID").unwrap_or_default();
    assert!(
        !waba.is_empty() && !phone_id.is_empty(),
        "set CTOX_TOOLHUB_LIVE_WHATSAPP_WABA_ID and CTOX_TOOLHUB_LIVE_WHATSAPP_PHONE_NUMBER_ID \
         to the Meta object ids of the account behind the token"
    );
    for (tool, args) in [
        ("WHATSAPP_LIST_PHONE_NUMBERS", json!({"business_account_id": waba})),
        (
            "WHATSAPP_LIST_MESSAGE_TEMPLATES",
            json!({"business_account_id": waba, "limit": 5}),
        ),
        ("WHATSAPP_GET_PHONE_NUMBER", json!({"phone_number_id": phone_id})),
    ] {
        let Some(result) = live(&a, tool, args) else { return };
        let v = result.unwrap_or_else(|e| panic!("{tool}: {e}"));
        assert!(v.is_object(), "{tool}: expected an object, got {v}");
    }
}
