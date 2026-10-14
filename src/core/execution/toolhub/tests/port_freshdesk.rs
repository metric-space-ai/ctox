//! Freshdesk port: one recorded case per action, expectations derived from the
//! Activepieces `run()` code (all five actions are plain `GET /api/v2/...` calls).
mod support;

use ctox_toolhub::toolkits::freshdesk::Freshdesk;
use serde_json::{json, Value};
use support::{live, run, Case};

fn adapter() -> Freshdesk {
    Freshdesk::with_api_base("https://acme.freshdesk.com")
}

/// `&[(ptr, value)]` for a `Case` field: leaked so it lives as long as the case.
fn ptrs<const N: usize>(pairs: [(&'static str, Value); N]) -> &'static [(&'static str, Value)] {
    Box::leak(Box::new(pairs))
}

fn contact(id: i64, name: &str, email: &str) -> Value {
    json!({
        "active": true, "address": "Weinstraße 12, 80333 München",
        "company_id": 81001776471_i64, "description": "IT Support Manager bei Acme SE",
        "email": email, "id": id, "job_title": "IT Support Manager", "language": "de",
        "mobile": "+49 151 23456789", "name": name, "phone": "+49 89 123456",
        "time_zone": "Berlin", "twitter_id": null, "unique_external_id": null,
        "other_emails": [], "tags": [],
        "created_at": "2025-11-02T09:14:22Z", "updated_at": "2026-10-01T12:05:41Z"
    })
}

fn ticket(id: i64, requester_id: i64, status: i64, subject: &str) -> Value {
    json!({
        "cc_emails": [], "fwd_emails": [], "reply_cc_emails": [], "ticket_cc_emails": [],
        "fr_escalated": false, "spam": false, "email_config_id": 81000076243_i64,
        "group_id": 81000433095_i64, "priority": 1, "requester_id": requester_id,
        "responder_id": 81005522114_i64, "source": 2, "company_id": 81001776471_i64,
        "status": status, "subject": subject, "association_type": null,
        "to_emails": null, "product_id": null, "id": id, "type": "Question",
        "due_by": "2026-10-16T09:00:00Z", "fr_due_by": "2026-10-15T09:00:00Z",
        "is_escalated": false,
        "description": format!("<div>{subject}</div>"),
        "description_text": subject,
        "created_at": "2026-10-13T08:12:44Z", "updated_at": "2026-10-14T06:40:12Z"
    })
}

#[test]
fn freshdesk_actions_are_ported() {
    let a = adapter();
    let cases: Vec<Case> = vec![
        Case {
            tool: "FRESHDESK_LIST_TICKETS",
            args: json!({}),
            method: "GET",
            url_contains: &["https://acme.freshdesk.com/api/v2/tickets/"],
            body_eq: ptrs([]),
            response_status: 200,
            response: json!([
                ticket(12345, 81011948932, 2, "Login fails after password reset"),
                ticket(12349, 81011950777, 5, "Invoice PDF is blank"),
            ]),
            result_eq: ptrs([
                ("/count", json!(2)),
                ("/items/0/id", json!(12345)),
                ("/items/0/status", json!(2)),
                ("/items/1/subject", json!("Invoice PDF is blank")),
            ]),
        },
        Case {
            tool: "FRESHDESK_GET_TICKET_STATUS",
            args: json!({"ticketid": "12346"}),
            method: "GET",
            url_contains: &["https://acme.freshdesk.com/api/v2/tickets/12346"],
            body_eq: ptrs([]),
            response_status: 200,
            response: json!({
                "cc_emails": [], "spam": false, "email_config_id": 81000076243_i64,
                "group_id": 81000433095_i64, "priority": 3, "requester_id": 81011948932_i64,
                "responder_id": 81005522114_i64, "source": 2, "company_id": 81001776471_i64,
                "status": 3, "subject": "Export stuck at 40 percent", "id": 12346,
                "type": "Incident", "due_by": "2026-10-17T09:00:00Z",
                "fr_due_by": "2026-10-15T09:00:00Z", "is_escalated": false,
                "description": "<div>CSV export never finishes.</div>",
                "description_text": "CSV export never finishes.",
                "created_at": "2026-10-14T07:02:10Z", "updated_at": "2026-10-14T08:15:00Z"
            }),
            result_eq: ptrs([
                ("/ticket_status", json!("PENDING")),
                ("/assigned_status", json!("ASSIGNED")),
                ("/assigned_id", json!(81005522114_i64)),
            ]),
        },
        Case {
            tool: "FRESHDESK_GET_CONTACT",
            args: json!({"contactid": "81011948932"}),
            method: "GET",
            url_contains: &["https://acme.freshdesk.com/api/v2/contacts/81011948932"],
            body_eq: ptrs([]),
            response_status: 200,
            response: contact(81011948932, "Lena Hartmann", "lena.hartmann@acme.de"),
            result_eq: ptrs([
                ("/id", json!(81011948932_i64)),
                ("/name", json!("Lena Hartmann")),
                ("/email", json!("lena.hartmann@acme.de")),
                ("/company_id", json!(81001776471_i64)),
            ]),
        },
        Case {
            tool: "FRESHDESK_LIST_CONTACTS",
            args: json!({"filter_type": "company_id", "filter_value": "81001776471",
                         "filter_status": "verified", "per_page": 30}),
            method: "GET",
            url_contains: &["https://acme.freshdesk.com/api/v2/contacts/?company_id=81001776471&state=verified&per_page=30"],
            body_eq: ptrs([]),
            response_status: 200,
            response: json!([
                contact(81011948932, "Lena Hartmann", "lena.hartmann@acme.de"),
                contact(81012004410, "Tim Becker", "tim.becker@acme.de"),
            ]),
            result_eq: ptrs([
                ("/count", json!(2)),
                ("/items/0/email", json!("lena.hartmann@acme.de")),
                ("/items/1/name", json!("Tim Becker")),
            ]),
        },
        Case {
            tool: "FRESHDESK_LIST_TICKETS_BY_STATUS",
            args: json!({"status_filter": ["status:2", "status:3"]}),
            method: "GET",
            url_contains: &["https://acme.freshdesk.com/api/v2/search/tickets/?query=%22status%3A2+OR+status%3A3%22"],
            body_eq: ptrs([]),
            response_status: 200,
            // Recorded unsorted; the port sorts by requester_id like the piece.
            response: json!({
                "total": 2,
                "results": [
                    ticket(12346, 81011948932, 3, "Export stuck at 40 percent"),
                    ticket(12345, 81011000001, 2, "Login fails after password reset"),
                ]
            }),
            result_eq: ptrs([
                ("/total", json!(2)),
                ("/count", json!(2)),
                ("/items/0/id", json!(12345)),
                ("/items/0/status", json!("Open")),
                ("/items/1/id", json!(12346)),
                ("/items/1/status", json!("Pending")),
            ]),
        },
    ];
    support::assert_full_coverage(&a, &cases);
    for c in &cases {
        run(&a, c);
    }
}

/// Guards that do not need a recorded response.
#[test]
fn invalid_input_is_rejected_before_the_call() {
    use ctox_toolhub::toolkits::ToolkitAdapter;

    let a = adapter();
    for (tool, args) in [
        ("FRESHDESK_GET_TICKET_STATUS", json!({"ticketid": "../../admin"})),
        ("FRESHDESK_GET_TICKET_STATUS", json!({"ticketid": "12a"})),
        ("FRESHDESK_GET_TICKET_STATUS", json!({})),
        ("FRESHDESK_GET_CONTACT", json!({"contactid": "8101 1948"})),
        ("FRESHDESK_LIST_CONTACTS", json!({"filter_type": "email", "per_page": 0})),
        ("FRESHDESK_LIST_CONTACTS", json!({"filter_type": "company", "filter_value": "1", "per_page": 0})),
        ("FRESHDESK_LIST_CONTACTS", json!({"filter_status": "active", "per_page": 0})),
        ("FRESHDESK_LIST_TICKETS_BY_STATUS", json!({"status_filter": []})),
        ("FRESHDESK_LIST_TICKETS_BY_STATUS", json!({"status_filter": ["status:7"]})),
        ("FRESHDESK_LIST_TICKETS_BY_STATUS", json!({"status_filter": ["open"]})),
    ] {
        assert!(
            a.build_request(tool, &args).is_err(),
            "{tool}: expected {args} to be rejected"
        );
    }
}

/// Live read-only calls through the hub exec path. Requires
/// `CTOX_TOOLHUB_LIVE_FRESHDESK_TOKEN` (the account's API key) and an account
/// whose subdomain matches the registered adapter's base URL.
#[test]
#[ignore]
fn live_read_only_actions() {
    let a = adapter();
    for (tool, args) in [
        ("FRESHDESK_LIST_TICKETS", json!({})),
        ("FRESHDESK_LIST_CONTACTS", json!({"per_page": 1})),
        ("FRESHDESK_LIST_TICKETS_BY_STATUS", json!({"status_filter": ["status:2"]})),
    ] {
        let Some(result) = live(&a, tool, args) else {
            return;
        };
        let v = result.unwrap_or_else(|e| panic!("{tool}: {e}"));
        assert!(v.is_object(), "{tool}: expected an object, got {v}");
    }
}
