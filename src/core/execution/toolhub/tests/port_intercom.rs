//! Intercom port: one recorded case per action. Every method, path and body
//! field comes from the Activepieces piece (`packages/pieces/community/
//! intercom/src/lib`) — the `intercomRequest()` calls name the paths
//! directly, SDK calls are mapped to the REST endpoint they wrap.
mod support;

use ctox_toolhub::toolkits::intercom::Intercom;
use serde_json::{json, Value};
use support::{live, run, Case};

const BASE: &str = "https://api.intercom.test";

const ADMIN: &str = "531415";
const CONTACT: &str = "5ba682d23d7cf935befca3b7";
const COMPANY: &str = "5f2d3d4e5a1b2c3d4e5f6a7b";
const CONVERSATION: &str = "1234567890";
const TICKET: &str = "27000012345678";
const TAG: &str = "12345";

fn adapter() -> Intercom {
    Intercom::with_api_base(BASE)
}

/// `&[(ptr, value)]` for a `Case` field: leaked so it lives as long as the case.
fn ptrs<const N: usize>(pairs: [(&'static str, Value); N]) -> &'static [(&'static str, Value)] {
    Box::leak(Box::new(pairs))
}

fn contact_json() -> Value {
    json!({
        "type": "contact",
        "id": CONTACT,
        "role": "user",
        "email": "ada@acme.com",
        "name": "Ada Lovelace",
        "phone": "+49 30 1234567",
        "external_id": "usr_1024",
        "created_at": 1_757_222_400,
        "updated_at": 1_767_225_600,
        "signed_up_at": 1_757_222_400,
        "owner": {"type": "admin", "id": ADMIN},
        "custom_attributes": {"plan": "enterprise"},
        "tags": {"tags": [{"id": TAG, "name": "vip"}], "pages": {"next": null}},
    })
}

fn conversation_json() -> Value {
    json!({
        "type": "conversation",
        "id": CONVERSATION,
        "title": "Login page times out",
        "created_at": 1_767_225_600,
        "updated_at": 1_767_312_000,
        "state": "open",
        "open": true,
        "read": true,
        "admin_assignee_id": ADMIN,
        "team_assignee_id": "814865",
        "contact": {"type": "contact", "id": CONTACT},
        "source": {"id": "1234", "type": "user", "subject": "Login page times out",
                   "body": "The login page spins forever in Safari.",
                   "author": {"type": "user", "id": CONTACT, "email": "ada@acme.com"}},
        "tags": {"tags": [{"id": TAG, "name": "vip"}]},
        "conversation_parts": {
            "type": "conversation_part_list",
            "conversation_parts": [
                {"type": "conversation_part", "id": "part-1", "part_type": "comment",
                 "body": "The login page spins forever in Safari.",
                 "created_at": 1_767_225_600,
                 "author": {"type": "user", "id": CONTACT, "name": "Ada Lovelace"}},
                {"type": "conversation_part", "id": "part-2", "part_type": "comment",
                 "body": "We shipped a fix for Safari.",
                 "created_at": 1_767_312_000,
                 "author": {"type": "admin", "id": ADMIN, "name": "Support"}}
            ],
            "pages": {"type": "pages", "page": 1, "per_page": 50, "total_pages": 1}
        }
    })
}

#[test]
fn intercom_actions_are_ported() {
    let a = adapter();
    let cases: Vec<Case> = vec![
        Case {
            tool: "INTERCOM_LIST_ADMINS",
            args: json!({}),
            method: "GET",
            url_contains: &[BASE, "/admins"],
            body_eq: ptrs([]),
            response_status: 200,
            response: json!({"type": "admin.list", "admins": [
                {"type": "admin", "id": ADMIN, "name": "Support", "email": "support@acme.com",
                 "job_title": "Support Lead", "away_mode_enabled": false, "has_inbox_seat": true,
                 "team_ids": [814865]},
                {"type": "admin", "id": "90210", "name": "Grace", "email": "grace@acme.com",
                 "away_mode_enabled": true, "has_inbox_seat": false, "team_ids": []}
            ]}),
            result_eq: ptrs([
                ("/items/0/id", json!(ADMIN)),
                ("/items/0/name", json!("Support")),
                ("/items/0/away_mode_enabled", json!(false)),
                ("/items/1/away_mode_enabled", json!(true)),
            ]),
        },
        Case {
            tool: "INTERCOM_LIST_TEAMS",
            args: json!({}),
            method: "GET",
            url_contains: &[BASE, "/teams"],
            body_eq: ptrs([]),
            response_status: 200,
            response: json!({"type": "team.list", "teams": [
                {"type": "team", "id": "814865", "name": "Customer Support",
                 "admin_ids": [ADMIN, "90210"]}
            ]}),
            result_eq: ptrs([
                ("/items/0/name", json!("Customer Support")),
                ("/items/0/admin_ids/1", json!("90210")),
            ]),
        },
        Case {
            tool: "INTERCOM_LIST_TAGS",
            args: json!({}),
            method: "GET",
            url_contains: &[BASE, "/tags"],
            body_eq: ptrs([]),
            response_status: 200,
            response: json!({"type": "list", "data": [
                {"type": "tag", "id": TAG, "name": "vip"},
                {"type": "tag", "id": "67890", "name": "bug"}
            ], "total_count": 2}),
            result_eq: ptrs([("/data/0/name", json!("vip"))]),
        },
        Case {
            tool: "INTERCOM_LIST_TICKET_TYPES",
            args: json!({}),
            method: "GET",
            url_contains: &[BASE, "/ticket_types"],
            body_eq: ptrs([]),
            response_status: 200,
            response: json!({"type": "ticket_type.list", "data": [
                {"type": "ticket_type", "id": "1", "name": "Billing", "description": "Billing questions",
                 "category": "Customer", "archived": false,
                 "ticket_type_attributes": {"type": "ticket_type_attribute.list", "data": [
                     {"type": "ticket_type_attribute", "name": "due_date", "data_type": "datetime",
                      "required_to_create": false, "visible_to_contacts": false},
                     {"type": "ticket_type_attribute", "name": "severity", "data_type": "list",
                      "required_to_create": true, "visible_to_contacts": true,
                      "input_options": {"list_options": [{"id": "low", "label": "Low"},
                                                          {"id": "high", "label": "High"}]}}
                 ]}}
            ]}),
            result_eq: ptrs([
                ("/items/0/name", json!("Billing")),
                ("/items/0/attributes/0/data_type", json!("datetime")),
                ("/items/0/attributes/1/required_to_create", json!(true)),
                ("/items/0/attributes/1/options/1/id", json!("high")),
            ]),
        },
        Case {
            tool: "INTERCOM_LIST_DATA_ATTRIBUTES",
            args: json!({"model": "contact", "include_archived": true}),
            method: "GET",
            url_contains: &[BASE, "/data_attributes", "model=contact", "include_archived=true"],
            body_eq: ptrs([]),
            response_status: 200,
            response: json!({"type": "data_attribute.list", "data": [
                {"type": "data_attribute", "name": "plan", "full_name": "contact.plan",
                 "label": "Plan", "data_type": "string", "model": "contact", "custom": true,
                 "api_writable": true, "options": []},
                {"type": "data_attribute", "name": "email", "label": "Email",
                 "data_type": "string", "model": "contact", "custom": false, "api_writable": true}
            ]}),
            result_eq: ptrs([
                ("/items/0/name", json!("plan")),
                ("/items/0/custom", json!(true)),
                ("/items/1/custom", json!(false)),
            ]),
        },
        Case {
            tool: "INTERCOM_LIST_COMPANIES",
            args: json!({"page": 1, "per_page": 50}),
            method: "POST",
            url_contains: &[BASE, "/companies/list", "page=1", "per_page=50", "order=desc"],
            body_eq: ptrs([]),
            response_status: 200,
            response: json!({"type": "company.list", "data": [
                {"type": "company", "id": COMPANY, "name": "Acme GmbH", "company_id": "acme",
                 "created_at": 1_757_222_400}
            ], "total_count": 1, "pages": {"type": "pages", "page": 1, "per_page": 50, "total_pages": 1}}),
            result_eq: ptrs([
                ("/items/0/name", json!("Acme GmbH")),
                ("/total_count", json!(1)),
                ("/has_more", json!(false)),
            ]),
        },
        Case {
            tool: "INTERCOM_FIND_COMPANY",
            args: json!({"search_field": "name", "search_value": "Acme GmbH"}),
            method: "GET",
            url_contains: &[BASE, "/companies?", "name=Acme+GmbH", "per_page=1"],
            body_eq: ptrs([]),
            response_status: 200,
            response: json!({"type": "company.list", "data": [
                {"type": "company", "id": COMPANY, "name": "Acme GmbH", "company_id": "acme"}
            ], "total_count": 1}),
            result_eq: ptrs([
                ("/found", json!(true)),
                ("/company/name", json!("Acme GmbH")),
            ]),
        },
        Case {
            tool: "INTERCOM_GET_COMPANY",
            args: json!({"company_id": COMPANY}),
            method: "GET",
            url_contains: &[BASE, "/companies/5f2d3d4e5a1b2c3d4e5f6a7b"],
            body_eq: ptrs([]),
            response_status: 200,
            response: json!({"type": "company", "id": COMPANY, "name": "Acme GmbH",
                             "company_id": "acme", "plan": "enterprise", "size": 120,
                             "created_at": 1_757_222_400}),
            result_eq: ptrs([
                ("/type", json!("company")),
                ("/company_id", json!("acme")),
                ("/plan", json!("enterprise")),
            ]),
        },
        Case {
            tool: "INTERCOM_GET_CONTACT",
            args: json!({"contact_id": CONTACT}),
            method: "GET",
            url_contains: &[BASE, "/contacts/5ba682d23d7cf935befca3b7"],
            body_eq: ptrs([]),
            response_status: 200,
            response: contact_json(),
            result_eq: ptrs([
                ("/email", json!("ada@acme.com")),
                ("/role", json!("user")),
                ("/owner/id", json!(ADMIN)),
            ]),
        },
        Case {
            tool: "INTERCOM_LIST_CONTACT_NOTES",
            args: json!({"contact_id": CONTACT, "per_page": 25}),
            method: "GET",
            url_contains: &[BASE, "/contacts/5ba682d23d7cf935befca3b7/notes", "per_page=25"],
            body_eq: ptrs([]),
            response_status: 200,
            response: json!({"type": "list", "data": [
                {"type": "note", "id": "9", "created_at": 1_767_225_600,
                 "body": "Called about the Safari bug."}
            ], "total_count": 1, "pages": {"type": "pages", "page": 1, "per_page": 25, "total_pages": 3}}),
            result_eq: ptrs([
                ("/items/0/body", json!("Called about the Safari bug.")),
                ("/has_more", json!(true)),
                ("/next_page", json!(2)),
            ]),
        },
        Case {
            tool: "INTERCOM_SEARCH_CONTACTS",
            args: json!({"email": "ada@acme.com", "per_page": 5, "starting_after": "cursor-1"}),
            method: "POST",
            url_contains: &[BASE, "/contacts/search"],
            body_eq: ptrs([
                ("/query/field", json!("email")),
                ("/query/operator", json!("=")),
                ("/query/value", json!("ada@acme.com")),
                ("/pagination/per_page", json!(5)),
                ("/pagination/starting_after", json!("cursor-1")),
            ]),
            response_status: 200,
            response: json!({"type": "list", "data": [contact_json()], "total_count": 1,
                             "pages": {"type": "pages", "next": {"page": 2, "starting_after": "cursor-2"}}}),
            result_eq: ptrs([
                ("/items/0/id", json!(CONTACT)),
                ("/next_starting_after", json!("cursor-2")),
                ("/has_more", json!(true)),
            ]),
        },
        Case {
            tool: "INTERCOM_GET_CONVERSATION",
            args: json!({"conversation_id": CONVERSATION}),
            method: "GET",
            url_contains: &[BASE, "/conversations/1234567890"],
            body_eq: ptrs([]),
            response_status: 200,
            response: conversation_json(),
            result_eq: ptrs([
                ("/state", json!("open")),
                ("/source/subject", json!("Login page times out")),
                ("/conversation_parts/conversation_parts/1/author/id", json!(ADMIN)),
            ]),
        },
        Case {
            tool: "INTERCOM_FIND_CONVERSATION",
            args: json!({
                "search_field": "source.subject", "match_type": "contains",
                "search_term": "login", "state": "open"
            }),
            method: "POST",
            url_contains: &[BASE, "/conversations/search"],
            body_eq: ptrs([
                ("/query/operator", json!("AND")),
                ("/query/value/0/field", json!("source.subject")),
                ("/query/value/0/operator", json!("~")),
                ("/query/value/0/value", json!("login")),
                ("/query/value/1/field", json!("state")),
                ("/query/value/1/value", json!("open")),
            ]),
            response_status: 200,
            response: json!({"type": "conversation.list", "conversations": [conversation_json()],
                             "total_count": 1, "pages": {"next": null}}),
            result_eq: ptrs([
                ("/found", json!(true)),
                ("/conversation/id", json!(CONVERSATION)),
            ]),
        },
        Case {
            tool: "INTERCOM_SEARCH_CONVERSATIONS",
            args: json!({"state": "open", "tag_id": TAG, "updated_after": "2026-01-01",
                         "per_page": 20, "starting_after": "cursor-9"}),
            method: "POST",
            url_contains: &[BASE, "/conversations/search"],
            body_eq: ptrs([
                ("/query/operator", json!("AND")),
                ("/query/value/0/field", json!("state")),
                ("/query/value/1/field", json!("tag_ids")),
                ("/query/value/1/value", json!(TAG)),
                ("/query/value/2/field", json!("updated_at")),
                ("/query/value/2/operator", json!(">")),
                ("/query/value/2/value", json!(1_767_225_600)),
                ("/pagination/per_page", json!(20)),
                ("/pagination/starting_after", json!("cursor-9")),
            ]),
            response_status: 200,
            response: json!({"type": "conversation.list", "conversations": [conversation_json()],
                             "total_count": 1, "pages": {"next": null}}),
            result_eq: ptrs([
                ("/items/0/id", json!(CONVERSATION)),
                ("/next_starting_after", json!(null)),
            ]),
        },
        Case {
            tool: "INTERCOM_GET_TICKET",
            args: json!({"ticket_id": TICKET}),
            method: "GET",
            url_contains: &[BASE, "/tickets/27000012345678"],
            body_eq: ptrs([]),
            response_status: 200,
            response: json!({"type": "ticket", "id": TICKET, "title": "Invoice question",
                             "ticket_type_id": "1", "state": "in_progress", "open": true,
                             "created_at": 1_767_225_600}),
            result_eq: ptrs([
                ("/id", json!(TICKET)),
                ("/state", json!("in_progress")),
            ]),
        },
        Case {
            tool: "INTERCOM_SEARCH_TICKETS",
            args: json!({"ticket_type_id": "1", "open": "open", "title_contains": "invoice"}),
            method: "POST",
            url_contains: &[BASE, "/tickets/search"],
            body_eq: ptrs([
                ("/query/value/0/field", json!("ticket_type_id")),
                ("/query/value/1/field", json!("open")),
                ("/query/value/1/value", json!(true)),
                ("/query/value/2/field", json!("title")),
                ("/query/value/2/operator", json!("~")),
            ]),
            response_status: 200,
            response: json!({"type": "ticket.list", "tickets": [
                {"type": "ticket", "id": TICKET, "title": "Invoice question", "open": true}
            ], "total_count": 1, "pages": {"next": {"starting_after": "ticket-cursor"}}}),
            result_eq: ptrs([
                ("/items/0/title", json!("Invoice question")),
                ("/next_starting_after", json!("ticket-cursor")),
            ]),
        },
        Case {
            tool: "INTERCOM_SEARCH_ARTICLES",
            args: json!({"phrase": "password reset", "state": "published"}),
            method: "GET",
            url_contains: &[BASE, "/articles/search", "phrase=password+reset", "state=published"],
            body_eq: ptrs([]),
            response_status: 200,
            response: json!({"type": "list", "total_count": 1, "data": {"articles": [
                {"type": "article", "id": "42", "title": "Reset your password",
                 "description": "How to reset your password",
                 "body": "<p>Open settings …</p>", "state": "published",
                 "url": "https://help.acme.com/en/articles/42", "updated_at": 1_760_000_000}
            ]}}),
            result_eq: ptrs([
                ("/items/0/title", json!("Reset your password")),
                ("/total_count", json!(1)),
            ]),
        },
        Case {
            tool: "INTERCOM_CREATE_USER",
            args: json!({"email": "ada@acme.com", "name": "Ada Lovelace",
                         "external_id": "usr_1024", "created_at": "2025-09-09T08:00:00Z",
                         "custom_attributes": {"plan": "enterprise"}}),
            method: "POST",
            url_contains: &[BASE, "/contacts"],
            body_eq: ptrs([
                ("/role", json!("user")),
                ("/email", json!("ada@acme.com")),
                ("/name", json!("Ada Lovelace")),
                ("/external_id", json!("usr_1024")),
                ("/signed_up_at", json!(1_757_404_800)),
                ("/custom_attributes/plan", json!("enterprise")),
            ]),
            response_status: 200,
            response: contact_json(),
            result_eq: ptrs([("/id", json!(CONTACT)), ("/email", json!("ada@acme.com"))]),
        },
        Case {
            tool: "INTERCOM_UPDATE_CONTACT",
            args: json!({"contact_id": CONTACT, "name": "Ada L.", "unsubscribed": "unsubscribed",
                         "custom_attributes": {"plan": "pro"}}),
            method: "PUT",
            url_contains: &[BASE, "/contacts/5ba682d23d7cf935befca3b7"],
            body_eq: ptrs([
                ("/name", json!("Ada L.")),
                ("/unsubscribed_from_emails", json!(true)),
                ("/custom_attributes/plan", json!("pro")),
            ]),
            response_status: 200,
            response: contact_json(),
            result_eq: ptrs([("/id", json!(CONTACT))]),
        },
        Case {
            tool: "INTERCOM_CREATE_OR_UPDATE_COMPANY",
            args: json!({"company_id": "acme", "name": "Acme GmbH", "size": 120,
                         "monthly_spend": 5000, "remote_created_at": "2025-09-09T08:00:00Z",
                         "custom_attributes": {"tier": "gold"}}),
            method: "POST",
            url_contains: &[BASE, "/companies"],
            body_eq: ptrs([
                ("/company_id", json!("acme")),
                ("/name", json!("Acme GmbH")),
                ("/size", json!(120)),
                ("/monthly_spend", json!(5000)),
                ("/remote_created_at", json!(1_757_404_800)),
                ("/custom_attributes/tier", json!("gold")),
            ]),
            response_status: 200,
            response: json!({"type": "company", "id": COMPANY, "name": "Acme GmbH",
                             "company_id": "acme", "created_at": 1_767_225_600}),
            result_eq: ptrs([("/company_id", json!("acme")), ("/id", json!(COMPANY))]),
        },
        Case {
            tool: "INTERCOM_CREATE_CONVERSATION",
            args: json!({"contact_id": CONTACT, "contact_type": "user", "body": "How can we help?"}),
            method: "POST",
            url_contains: &[BASE, "/conversations"],
            body_eq: ptrs([
                ("/body", json!("How can we help?")),
                ("/from/type", json!("user")),
                ("/from/id", json!(CONTACT)),
            ]),
            response_status: 200,
            response: conversation_json(),
            result_eq: ptrs([("/id", json!(CONVERSATION)), ("/state", json!("open"))]),
        },
        Case {
            tool: "INTERCOM_REPLY_TO_CONVERSATION",
            args: json!({"conversation_id": CONVERSATION, "admin_id": ADMIN,
                         "body": "We shipped a fix for Safari."}),
            method: "POST",
            url_contains: &[BASE, "/conversations/1234567890/reply"],
            body_eq: ptrs([
                ("/type", json!("admin")),
                ("/message_type", json!("comment")),
                ("/body", json!("We shipped a fix for Safari.")),
                ("/admin_id", json!(ADMIN)),
            ]),
            response_status: 200,
            response: conversation_json(),
            result_eq: ptrs([("/id", json!(CONVERSATION))]),
        },
        Case {
            tool: "INTERCOM_ADD_NOTE_TO_CONVERSATION",
            args: json!({"conversation_id": CONVERSATION, "admin_id": ADMIN, "body": "Tier 2 owns this."}),
            method: "POST",
            url_contains: &[BASE, "/conversations/1234567890/reply"],
            body_eq: ptrs([
                ("/message_type", json!("note")),
                ("/body", json!("Tier 2 owns this.")),
                ("/admin_id", json!(ADMIN)),
            ]),
            response_status: 200,
            response: conversation_json(),
            result_eq: ptrs([("/id", json!(CONVERSATION))]),
        },
        Case {
            tool: "INTERCOM_ADD_NOTE_TO_CONTACT",
            args: json!({"contact_id": CONTACT, "body": "Prefers email over chat."}),
            method: "POST",
            url_contains: &[BASE, "/notes"],
            body_eq: ptrs([
                ("/contact_id", json!(CONTACT)),
                ("/body", json!("Prefers email over chat.")),
            ]),
            response_status: 200,
            response: json!({"type": "note", "id": "17", "created_at": 1_767_312_000,
                             "body": "Prefers email over chat."}),
            result_eq: ptrs([("/id", json!("17")), ("/body", json!("Prefers email over chat."))]),
        },
        Case {
            tool: "INTERCOM_SET_CONVERSATION_STATE",
            args: json!({"conversation_id": CONVERSATION, "state": "close", "admin_id": ADMIN,
                         "closing_message": "Glad we got this sorted."}),
            method: "POST",
            url_contains: &[BASE, "/conversations/1234567890/parts"],
            body_eq: ptrs([
                ("/message_type", json!("close")),
                ("/type", json!("admin")),
                ("/admin_id", json!(ADMIN)),
                ("/body", json!("Glad we got this sorted.")),
            ]),
            response_status: 200,
            response: conversation_json(),
            result_eq: ptrs([("/id", json!(CONVERSATION))]),
        },
        Case {
            tool: "INTERCOM_ASSIGN_CONVERSATION",
            args: json!({"conversation_id": CONVERSATION, "admin_id": ADMIN,
                         "assignee_id": "814865", "body": "Handing over to the support team."}),
            method: "POST",
            url_contains: &[BASE, "/conversations/1234567890/manage"],
            body_eq: ptrs([
                ("/type", json!("admin")),
                ("/message_type", json!("assignment")),
                ("/admin_id", json!(ADMIN)),
                ("/assignee_id", json!("814865")),
                ("/body", json!("Handing over to the support team.")),
            ]),
            response_status: 200,
            response: conversation_json(),
            result_eq: ptrs([("/id", json!(CONVERSATION))]),
        },
        Case {
            tool: "INTERCOM_SEND_MESSAGE",
            args: json!({"message_type": "email", "admin_id": ADMIN, "contact_id": CONTACT,
                         "contact_role": "user", "subject": "Your invoice", "template": "personal",
                         "body": "Here is your invoice.",
                         "create_conversation_without_contact_reply": true}),
            method: "POST",
            url_contains: &[BASE, "/messages"],
            body_eq: ptrs([
                ("/message_type", json!("email")),
                ("/from/id", json!(ADMIN)),
                ("/from/type", json!("admin")),
                ("/to/id", json!(CONTACT)),
                ("/to/type", json!("user")),
                ("/subject", json!("Your invoice")),
                ("/template", json!("personal")),
                ("/body", json!("Here is your invoice.")),
                ("/create_conversation_without_contact_reply", json!(true)),
            ]),
            response_status: 202,
            response: json!({"type": "conversation_message", "id": "2001",
                             "created_at": 1_767_312_000}),
            result_eq: ptrs([("/id", json!("2001"))]),
        },
        Case {
            tool: "INTERCOM_TAG_CONTACT",
            args: json!({"contact_id": CONTACT, "tag_id": TAG}),
            method: "POST",
            url_contains: &[BASE, "/contacts/5ba682d23d7cf935befca3b7/tags"],
            body_eq: ptrs([("/id", json!(TAG))]),
            response_status: 200,
            response: json!({"type": "contact", "id": CONTACT,
                             "tags": {"tags": [{"id": TAG, "name": "vip"}]}}),
            result_eq: ptrs([("/tags/tags/0/name", json!("vip"))]),
        },
        Case {
            tool: "INTERCOM_TAG_CONVERSATION",
            args: json!({"conversation_id": CONVERSATION, "tag_id": TAG, "admin_id": ADMIN}),
            method: "POST",
            url_contains: &[BASE, "/conversations/1234567890/tags"],
            body_eq: ptrs([("/id", json!(TAG)), ("/admin_id", json!(ADMIN))]),
            response_status: 200,
            response: json!({"type": "conversation", "id": CONVERSATION,
                             "tags": {"tags": [{"id": TAG, "name": "vip"}]}}),
            result_eq: ptrs([("/tags/tags/0/id", json!(TAG))]),
        },
    ];
    support::assert_full_coverage(&a, &cases);
    for c in &cases {
        run(&a, c);
    }
}

#[test]
fn two_step_upserts_are_not_offered() {
    use ctox_toolhub::toolkits::ToolkitAdapter;

    // The piece's create/update-by-email actions search first and then write;
    // one tool call builds exactly one request, so only the single-request
    // variants exist. They must not silently disappear into a wrong endpoint.
    let a = adapter();
    assert!(a
        .build_request("INTERCOM_CREATE_OR_UPDATE_USER", &json!({"email": "a@b.de"}))
        .is_err());
    assert!(a.spec().tools.iter().all(|t| t.slug != "INTERCOM_CREATE_OR_UPDATE_USER"));
}

/// Live read-only calls through the hub exec path (Bearer access token).
/// Run with `CTOX_TOOLHUB_LIVE_INTERCOM_TOKEN=… cargo test --offline --test
/// port_intercom -- --ignored`.
#[test]
#[ignore]
fn live_read_only_actions() {
    // The real US base URL (`REGION_BASE_URLS['intercom']`); use
    // `Intercom::with_region("eu.intercom"|"au.intercom")` for other regions.
    let a = Intercom::default();
    for (tool, args) in [
        ("INTERCOM_LIST_ADMINS", json!({})),
        ("INTERCOM_LIST_TEAMS", json!({})),
        ("INTERCOM_SEARCH_CONTACTS", json!({"per_page": 1})),
    ] {
        let Some(result) = live(&a, tool, args) else { return };
        let v = result.unwrap_or_else(|e| panic!("{tool}: {e}"));
        assert!(v.is_object(), "{tool}: expected an object, got {v}");
    }
}