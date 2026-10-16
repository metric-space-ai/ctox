//! Zendesk port: one recorded case per tool, expectations derived from the
//! Activepieces `run()` code (`httpClient.sendRequest` urls/bodies in
//! `src/lib/actions/*.ts` and `src/lib/common/props.ts`).
mod support;

use ctox_toolhub::toolkits::zendesk::Zendesk;
use serde_json::{json, Value};
use support::{live, run, Case};

const BASE: &str = "https://acme.zendesk.com/api/v2";

/// `&[(ptr, value)]` for a `Case` field: leaked so it lives as long as the case.
fn ptrs<const N: usize>(pairs: [(&'static str, Value); N]) -> &'static [(&'static str, Value)] {
    Box::leak(Box::new(pairs))
}

fn adapter() -> Zendesk {
    Zendesk::with_subdomain("acme")
}

fn ticket() -> Value {
    json!({
        "id": 35436,
        "url": "https://acme.zendesk.com/api/v2/tickets/35436.json",
        "subject": "Printer jams every second page",
        "raw_subject": "Printer jams every second page",
        "description": "The office printer jams on page 2.",
        "status": "open",
        "type": "problem",
        "priority": "high",
        "requester_id": 1234,
        "assignee_id": 5678,
        "organization_id": 9101,
        "group_id": 3210,
        "brand_id": 360000000001i64,
        "external_id": "pr-42",
        "forum_topic_id": null,
        "problem_id": null,
        "tags": ["printer", "hardware"],
        "comment_count": 3,
        "created_at": "2026-09-30T08:12:00Z",
        "updated_at": "2026-10-02T09:41:00Z",
        "due_at": "2026-10-10T12:00:00Z"
    })
}

fn envelope(key: &str, items: Value) -> Value {
    json!({key: items, "next_page": null, "previous_page": null, "count": 1})
}

#[test]
fn zendesk_actions_are_ported() {
    let a = adapter();
    let cases: Vec<Case> = vec![
        // ---------------------------------------------------------------- read
        Case {
            tool: "ZENDESK_LIST_TICKETS",
            args: json!({}),
            method: "GET",
            url_contains: &[BASE, "/api/v2/tickets.json?per_page=100"],
            body_eq: ptrs([]),
            response_status: 200,
            response: envelope("tickets", json!([ticket()])),
            result_eq: ptrs([
                ("/count", json!(1)),
                ("/items/0/id", json!(35436)),
                ("/items/0/status", json!("open")),
                ("/items/0/tags", json!("printer, hardware")),
                ("/items/0/requester_id", json!(1234)),
                ("/has_more", json!(false)),
            ]),
        },
        Case {
            tool: "ZENDESK_FIND_TICKETS",
            args: json!({"search_type": "status", "status": "open", "sort_by": "relevance",
                         "sort_order": "desc"}),
            method: "GET",
            url_contains: &[
                BASE,
                "/api/v2/search.json?",
                "query=type%3Aticket+status%3Aopen",
                "sort_order=desc",
            ],
            body_eq: ptrs([]),
            response_status: 200,
            response: json!({"results": [
                    { "result_type": "ticket", "id": 35436, "subject": "Printer jams every second page",
                      "status": "open", "type": "problem", "priority": "high", "requester_id": 1234,
                      "assignee_id": 5678, "organization_id": 9101, "group_id": 3210, "tags": ["printer"],
                      "comment_count": 3, "created_at": "2026-09-30T08:12:00Z",
                      "updated_at": "2026-10-02T09:41:00Z", "due_at": null },
                    { "result_type": "user", "id": 1234 }
                ],
                "facets": null, "next_page": null, "previous_page": null, "count": 2}),
            result_eq: ptrs([
                ("/count", json!(1)),
                ("/total_count", json!(2)),
                ("/items/0/id", json!(35436)),
                ("/items/0/tags", json!("printer")),
            ]),
        },
        Case {
            tool: "ZENDESK_FIND_LATEST_COMMENT",
            args: json!({"ticket_id": "35436"}),
            method: "GET",
            url_contains: &[BASE, "/api/v2/tickets/35436/comments?sort_order=desc"],
            body_eq: ptrs([]),
            response_status: 200,
            response: json!({"comments": [
                    {"id": 9002, "type": "Comment", "body": "Shipping the replacement today.",
                     "html_body": "<p>Shipping the replacement today.</p>",
                     "plain_body": "Shipping the replacement today.", "public": false,
                     "author_id": 5678, "attachments": [], "via": {"channel": "api"},
                     "created_at": "2026-10-02T09:40:00Z"},
                    {"id": 9001, "type": "Comment", "body": "Thanks for the report.",
                     "html_body": "<p>Thanks for the report.</p>",
                     "plain_body": "Thanks for the report.", "public": true,
                     "author_id": 5678, "attachments": [], "via": {"channel": "web"},
                     "created_at": "2026-10-01T12:00:00Z"}
                ],
                "next_page": null, "previous_page": null, "count": 2}),
            result_eq: ptrs([
                // comments[0] is the newest because of sort_order=desc
                ("/comment/id", json!(9002)),
                ("/comment/is_public", json!(false)),
                ("/public_comment/id", json!(9001)),
                ("/count", json!(2)),
            ]),
        },
        Case {
            tool: "ZENDESK_FIND_USER",
            args: json!({"search_type": "email", "email": "ada@acme.test", "sort_order": "asc"}),
            method: "GET",
            url_contains: &[
                BASE,
                "/api/v2/search.json?query=type%3Auser+email%3Aada%40acme.test&sort_order=asc",
            ],
            body_eq: ptrs([]),
            response_status: 200,
            response: json!({"results": [
                    {"result_type": "user", "id": 1234, "name": "Ada Lovelace",
                     "email": "ada@acme.test", "role": "end-user", "active": true,
                     "organization_id": 9101, "tags": ["vip"],
                     "created_at": "2026-01-04T09:00:00Z", "updated_at": "2026-10-01T09:00:00Z"}
                ],
                "facets": null, "next_page": null, "previous_page": null, "count": 1}),
            result_eq: ptrs([
                ("/count", json!(1)),
                ("/items/0/email", json!("ada@acme.test")),
                ("/items/0/role", json!("end-user")),
                ("/items/0/tags", json!("vip")),
            ]),
        },
        Case {
            tool: "ZENDESK_FIND_ORGANIZATION",
            args: json!({"search_type": "domain", "domain": "acme.test"}),
            method: "GET",
            url_contains: &[BASE, "/api/v2/search.json?query=type%3Aorganization+domain%3Aacme.test"],
            body_eq: ptrs([]),
            response_status: 200,
            response: json!({"results": [
                    {"result_type": "organization", "id": 9101, "name": "Acme GmbH",
                     "domain_names": ["acme.test"], "tags": ["enterprise"],
                     "shared_tickets": true, "shared_comments": true,
                     "created_at": "2025-11-02T09:00:00Z", "updated_at": "2026-10-01T09:00:00Z"}
                ],
                "facets": null, "next_page": null, "previous_page": null, "count": 1}),
            result_eq: ptrs([
                ("/count", json!(1)),
                ("/items/0/name", json!("Acme GmbH")),
                ("/items/0/domain_names", json!("acme.test")),
                ("/items/0/shared_tickets", json!(true)),
            ]),
        },
        Case {
            tool: "ZENDESK_FIND_USER_BY_EMAIL",
            args: json!({"email": "ada@acme.test"}),
            method: "GET",
            url_contains: &[BASE, "/api/v2/users/search.json?query=email%3Aada%40acme.test"],
            body_eq: ptrs([]),
            response_status: 200,
            response: json!({"users": [
                    {"id": 5678, "name": "Ada Lovelace", "email": "ada@acme.test",
                     "role": "agent", "active": true, "tags": []}
                ],
                "next_page": null, "previous_page": null, "count": 1}),
            result_eq: ptrs([
                ("/user_id", json!(5678)),
                ("/count", json!(1)),
                ("/users/0/role", json!("agent")),
            ]),
        },
        Case {
            tool: "ZENDESK_LIST_AGENTS",
            args: json!({}),
            method: "GET",
            url_contains: &[BASE, "/api/v2/users.json?role=agent&per_page=100"],
            body_eq: ptrs([]),
            response_status: 200,
            response: envelope("users", json!([
                {"id": 5678, "name": "Ada Lovelace", "email": "ada@acme.test", "role": "agent",
                 "active": true, "verified": true, "suspended": false, "organization_id": 9101,
                 "tags": ["support"], "created_at": "2024-02-01T09:00:00Z",
                 "updated_at": "2026-09-01T09:00:00Z"}
            ])),
            result_eq: ptrs([
                ("/count", json!(1)),
                ("/items/0/email", json!("ada@acme.test")),
                ("/items/0/role", json!("agent")),
                ("/items/0/tags", json!("support")),
            ]),
        },
        Case {
            tool: "ZENDESK_LIST_USERS",
            args: json!({"per_page": 25}),
            method: "GET",
            url_contains: &[BASE, "/api/v2/users.json?per_page=25"],
            body_eq: ptrs([]),
            response_status: 200,
            response: envelope("users", json!([
                {"id": 1234, "name": "Ada Lovelace", "email": "ada@acme.test", "role": "end-user",
                 "active": true, "verified": true, "suspended": false, "organization_id": 9101,
                 "tags": ["vip"], "created_at": "2026-01-04T09:00:00Z",
                 "updated_at": "2026-10-01T09:00:00Z"}
            ])),
            result_eq: ptrs([
                ("/items/0/id", json!(1234)),
                ("/items/0/organization_id", json!(9101)),
                ("/items/0/tags", json!("vip")),
            ]),
        },
        Case {
            tool: "ZENDESK_LIST_ORGANIZATIONS",
            args: json!({}),
            method: "GET",
            url_contains: &[BASE, "/api/v2/organizations.json?per_page=100"],
            body_eq: ptrs([]),
            response_status: 200,
            response: envelope("organizations", json!([
                {"id": 9101, "name": "Acme GmbH", "details": "German entity", "notes": "Key account",
                 "domain_names": ["acme.test", "acme.de"], "tags": ["enterprise"],
                 "shared_tickets": true, "shared_comments": true, "group_id": 3210,
                 "created_at": "2025-11-02T09:00:00Z", "updated_at": "2026-10-01T09:00:00Z"}
            ])),
            result_eq: ptrs([
                ("/items/0/name", json!("Acme GmbH")),
                ("/items/0/domain_names", json!("acme.test, acme.de")),
                ("/items/0/tags", json!("enterprise")),
                ("/items/0/group_id", json!(3210)),
            ]),
        },
        Case {
            tool: "ZENDESK_LIST_GROUPS",
            args: json!({}),
            method: "GET",
            url_contains: &[BASE, "/api/v2/groups.json?per_page=100"],
            body_eq: ptrs([]),
            response_status: 200,
            response: envelope("groups", json!([
                {"id": 3210, "name": "Hardware Support", "description": "Devices and printers",
                 "is_public": true, "default": false,
                 "created_at": "2025-03-01T09:00:00Z", "updated_at": "2026-09-01T09:00:00Z"}
            ])),
            result_eq: ptrs([
                ("/items/0/name", json!("Hardware Support")),
                ("/items/0/is_public", json!(true)),
            ]),
        },
        Case {
            tool: "ZENDESK_LIST_BRANDS",
            args: json!({}),
            method: "GET",
            url_contains: &[BASE, "/api/v2/brands.json"],
            body_eq: ptrs([]),
            response_status: 200,
            response: envelope("brands", json!([
                {"id": 360000000001i64, "name": "Acme Support", "subdomain": "acme",
                 "active": true, "default": true, "url": "https://acme.zendesk.com",
                 "created_at": "2025-01-05T09:00:00Z", "updated_at": "2026-01-05T09:00:00Z"}
            ])),
            result_eq: ptrs([
                ("/items/0/name", json!("Acme Support")),
                ("/items/0/subdomain", json!("acme")),
                ("/items/0/default", json!(true)),
            ]),
        },
        Case {
            tool: "ZENDESK_LIST_CUSTOM_ROLES",
            args: json!({}),
            method: "GET",
            url_contains: &[BASE, "/api/v2/custom_roles.json"],
            body_eq: ptrs([]),
            response_status: 200,
            response: envelope("custom_roles", json!([
                {"id": 4200, "name": "Tier 2", "description": "Escalated support",
                 "role_type": 0, "team_member_count": 7,
                 "created_at": "2025-06-01T09:00:00Z", "updated_at": "2026-06-01T09:00:00Z"}
            ])),
            result_eq: ptrs([
                ("/items/0/name", json!("Tier 2")),
                ("/items/0/team_member_count", json!(7)),
            ]),
        },
        Case {
            tool: "ZENDESK_LIST_TICKET_FIELDS",
            args: json!({}),
            method: "GET",
            url_contains: &[BASE, "/api/v2/ticket_fields.json"],
            body_eq: ptrs([]),
            response_status: 200,
            response: envelope("ticket_fields", json!([
                {"id": 36000, "key": "device_type", "title": "Device type", "type": "tagger",
                 "active": true, "required": false, "description": "Affected device",
                 "custom_field_options": [{"name": "Printer", "value": "printer"},
                                          {"name": "Router", "value": "router"}],
                 "created_at": "2025-04-01T09:00:00Z", "updated_at": "2026-04-01T09:00:00Z"}
            ])),
            result_eq: ptrs([
                ("/items/0/key", json!("device_type")),
                ("/items/0/type", json!("tagger")),
                ("/items/0/custom_field_options", json!("Printer, Router")),
            ]),
        },
        Case {
            tool: "ZENDESK_LIST_USER_FIELDS",
            args: json!({}),
            method: "GET",
            url_contains: &[BASE, "/api/v2/user_fields.json"],
            body_eq: ptrs([]),
            response_status: 200,
            response: envelope("user_fields", json!([
                {"id": 5100, "key": "department", "title": "Department", "type": "text",
                 "active": true, "required": false,
                 "created_at": "2025-04-01T09:00:00Z", "updated_at": "2026-04-01T09:00:00Z"}
            ])),
            result_eq: ptrs([
                ("/items/0/key", json!("department")),
                ("/items/0/active", json!(true)),
            ]),
        },
        Case {
            tool: "ZENDESK_LIST_ORGANIZATION_FIELDS",
            args: json!({}),
            method: "GET",
            url_contains: &[BASE, "/api/v2/organization_fields.json"],
            body_eq: ptrs([]),
            response_status: 200,
            response: envelope("organization_fields", json!([
                {"id": 6100, "key": "tier", "title": "Service tier", "type": "dropdown",
                 "active": true, "required": false,
                 "custom_field_options": [{"name": "Gold", "value": "gold"}],
                 "created_at": "2025-04-01T09:00:00Z", "updated_at": "2026-04-01T09:00:00Z"}
            ])),
            result_eq: ptrs([
                ("/items/0/key", json!("tier")),
                ("/items/0/custom_field_options", json!("Gold")),
            ]),
        },
        // ---------------------------------------------------------------- write
        Case {
            tool: "ZENDESK_CREATE_TICKET",
            args: json!({"comment_body": "The printer jams on page 2.", "subject": "Printer jams",
                         "comment_public": true, "requester_email": "ada@acme.test",
                         "assignee_id": "5678", "priority": "high", "status": "open",
                         "tags": ["printer"], "organization_id": "9101",
                         "custom_fields": [{"id": 36000, "value": "printer"}]}),
            method: "POST",
            url_contains: &[BASE, "/api/v2/tickets.json"],
            body_eq: ptrs([
                ("/ticket/comment/body", json!("The printer jams on page 2.")),
                ("/ticket/comment/public", json!(true)),
                ("/ticket/subject", json!("Printer jams")),
                ("/ticket/requester", json!({"email": "ada@acme.test", "name": "ada@acme.test"})),
                ("/ticket/assignee_id", json!(5678)),
                ("/ticket/priority", json!("high")),
                ("/ticket/tags", json!(["printer"])),
                ("/ticket/organization_id", json!(9101)),
                ("/ticket/custom_fields", json!([{"id": 36000, "value": "printer"}])),
            ]),
            response_status: 201,
            response: json!({"ticket": ticket()}),
            result_eq: ptrs([
                ("/success", json!(true)),
                ("/ticket/id", json!(35436)),
                ("/ticket/status", json!("open")),
            ]),
        },
        Case {
            tool: "ZENDESK_UPDATE_TICKET",
            args: json!({"ticket_id": "35436", "status": "solved", "tags": ["printer", "resolved"],
                         "comment_body": "Fixed in 4.2.", "comment_public": true,
                         "safe_update": true, "updated_stamp": "2026-10-02T09:41:00Z"}),
            method: "PUT",
            url_contains: &[BASE, "/api/v2/tickets/35436.json"],
            body_eq: ptrs([
                ("/ticket/status", json!("solved")),
                ("/ticket/tags", json!(["printer", "resolved"])),
                ("/ticket/comment/body", json!("Fixed in 4.2.")),
                ("/ticket/comment/public", json!(true)),
                ("/ticket/safe_update", json!(true)),
                ("/ticket/updated_stamp", json!("2026-10-02T09:41:00Z")),
            ]),
            response_status: 200,
            response: json!({"ticket": {
                "id": 35436, "subject": "Printer jams every second page", "status": "solved",
                "type": "problem", "priority": "high", "requester_id": 1234, "assignee_id": 5678,
                "organization_id": 9101, "group_id": 3210, "tags": ["printer", "resolved"],
                "comment_count": 4, "created_at": "2026-09-30T08:12:00Z",
                "updated_at": "2026-10-02T10:00:00Z", "due_at": null
            }}),
            result_eq: ptrs([
                ("/ticket/status", json!("solved")),
                ("/ticket/tags", json!("printer, resolved")),
            ]),
        },
        Case {
            tool: "ZENDESK_ADD_COMMENT_TO_TICKET",
            args: json!({"ticket_id": "35436", "comment_html_body": "<p>Shipping today.</p>",
                         "public": false, "author_id": "5678",
                         "via_followup_source_id": "35435"}),
            method: "PUT",
            url_contains: &[BASE, "/api/v2/tickets/35436.json"],
            body_eq: ptrs([
                ("/ticket/comment/html_body", json!("<p>Shipping today.</p>")),
                ("/ticket/comment/public", json!(false)),
                ("/ticket/comment/author_id", json!(5678)),
                ("/ticket/comment/via/followup_source_id", json!(35435)),
            ]),
            response_status: 200,
            response: json!({"ticket": {
                "id": 35436, "subject": "Printer jams every second page", "status": "open",
                "tags": ["printer"], "comment_count": 4,
                "comment": {"id": 9003, "body": null, "html_body": "<p>Shipping today.</p>",
                            "public": false, "author_id": 5678,
                            "uploads": ["7hlGUUJcVJHzEmP"], "created_at": "2026-10-02T11:00:00Z"}
            }}),
            result_eq: ptrs([
                ("/success", json!(true)),
                ("/comment_details/is_public", json!(false)),
                ("/comment_details/has_attachments", json!(true)),
                ("/comment_details/content_type", json!("html")),
            ]),
        },
        Case {
            tool: "ZENDESK_ADD_TAG_TO_TICKET",
            args: json!({"ticket_id": "35436", "tags": ["printer", "vip"], "safe_update": true,
                         "updated_stamp": "2026-10-02T09:41:00Z"}),
            method: "PUT",
            url_contains: &[BASE, "/api/v2/tickets/35436/tags.json"],
            body_eq: ptrs([
                ("/tags", json!(["printer", "vip"])),
                ("/updated_stamp", json!("2026-10-02T09:41:00Z")),
                // the piece sends the string "true", not a JSON boolean
                ("/safe_update", json!("true")),
            ]),
            response_status: 200,
            response: json!({"tags": ["printer", "hardware", "vip"]}),
            result_eq: ptrs([
                ("/success", json!(true)),
                ("/tags/2", json!("vip")),
                ("/count", json!(3)),
            ]),
        },
        Case {
            tool: "ZENDESK_REMOVE_TAG_FROM_TICKET",
            args: json!({"ticket_id": "35436", "tags": ["hardware"]}),
            method: "DELETE",
            url_contains: &[BASE, "/api/v2/tickets/35436/tags.json"],
            body_eq: ptrs([("/tags", json!(["hardware"]))]),
            response_status: 200,
            response: json!({"tags": ["printer"]}),
            result_eq: ptrs([
                ("/success", json!(true)),
                ("/tags", json!(["printer"])),
                ("/count", json!(1)),
            ]),
        },
        Case {
            tool: "ZENDESK_CREATE_USER",
            args: json!({"name": "Grace Hopper", "email": "grace@acme.test", "role": "agent",
                         "organization_id": "9101", "tags": ["vip"], "verified": true,
                         "skip_verify_email": true}),
            method: "POST",
            url_contains: &[BASE, "/api/v2/users.json"],
            body_eq: ptrs([
                ("/user/name", json!("Grace Hopper")),
                ("/user/email", json!("grace@acme.test")),
                ("/user/role", json!("agent")),
                ("/user/organization_id", json!(9101)),
                ("/user/tags", json!(["vip"])),
                ("/user/verified", json!(true)),
                ("/skip_verify_email", json!(true)),
            ]),
            response_status: 201,
            response: json!({"user": {
                "id": 7788, "name": "Grace Hopper", "email": "grace@acme.test", "role": "agent",
                "active": true, "verified": true, "suspended": false, "organization_id": 9101,
                "tags": ["vip"], "created_at": "2026-10-03T08:00:00Z", "updated_at": "2026-10-03T08:00:00Z"
            }}),
            result_eq: ptrs([
                ("/success", json!(true)),
                ("/user/id", json!(7788)),
                ("/user_role", json!("agent")),
            ]),
        },
        Case {
            tool: "ZENDESK_UPDATE_USER",
            args: json!({"user_id": "1234", "name": "Ada L.", "tags": ["vip", "eu"]}),
            method: "PUT",
            url_contains: &[BASE, "/api/v2/users/1234.json"],
            body_eq: ptrs([
                ("/user/name", json!("Ada L.")),
                ("/user/tags", json!(["vip", "eu"])),
            ]),
            response_status: 200,
            response: json!({"user": {
                "id": 1234, "name": "Ada L.", "email": "ada@acme.test", "role": "end-user",
                "active": true, "verified": true, "suspended": false, "organization_id": 9101,
                "tags": ["vip", "eu"], "created_at": "2026-01-04T09:00:00Z",
                "updated_at": "2026-10-03T08:05:00Z"
            }}),
            result_eq: ptrs([
                ("/success", json!(true)),
                ("/user/name", json!("Ada L.")),
                ("/user/tags", json!("vip, eu")),
            ]),
        },
        Case {
            tool: "ZENDESK_DELETE_USER",
            args: json!({"user_id": "1234", "confirmation": true}),
            method: "DELETE",
            url_contains: &[BASE, "/api/v2/users/1234.json"],
            body_eq: ptrs([]),
            response_status: 200,
            response: json!({"id": 1234, "name": "Ada L.", "email": "ada@acme.test",
                             "role": "end-user", "active": false, "tags": []}),
            result_eq: ptrs([("/success", json!(true)), ("/id", json!(1234))]),
        },
        Case {
            tool: "ZENDESK_CREATE_ORGANIZATION",
            args: json!({"name": "Acme SE", "domain_names": ["acme.test"], "tags": ["enterprise"],
                         "group_id": "3210", "shared_tickets": true,
                         "organization_fields": {"tier": "gold"}}),
            method: "POST",
            url_contains: &[BASE, "/api/v2/organizations.json"],
            body_eq: ptrs([
                ("/organization/name", json!("Acme SE")),
                ("/organization/domain_names", json!(["acme.test"])),
                ("/organization/tags", json!(["enterprise"])),
                ("/organization/group_id", json!(3210)),
                ("/organization/shared_tickets", json!(true)),
                ("/organization/organization_fields", json!({"tier": "gold"})),
            ]),
            response_status: 201,
            response: json!({"organization": {
                "id": 9555, "name": "Acme SE", "domain_names": ["acme.test"], "tags": ["enterprise"],
                "group_id": 3210, "shared_tickets": true, "shared_comments": false,
                "created_at": "2026-10-03T08:10:00Z", "updated_at": "2026-10-03T08:10:00Z"
            }}),
            result_eq: ptrs([
                ("/success", json!(true)),
                ("/organization/id", json!(9555)),
                ("/organization/name", json!("Acme SE")),
            ]),
        },
        Case {
            tool: "ZENDESK_UPDATE_ORGANIZATION",
            args: json!({"organization_id": "9101", "name": "Acme SE", "details": "Merged entity"}),
            method: "PUT",
            url_contains: &[BASE, "/api/v2/organizations/9101.json"],
            body_eq: ptrs([
                ("/organization/name", json!("Acme SE")),
                ("/organization/details", json!("Merged entity")),
            ]),
            response_status: 200,
            response: json!({"organization": {
                "id": 9101, "name": "Acme SE", "details": "Merged entity",
                "domain_names": ["acme.test", "acme.de"], "tags": ["enterprise"],
                "group_id": 3210, "shared_tickets": true, "shared_comments": true,
                "created_at": "2025-11-02T09:00:00Z", "updated_at": "2026-10-03T08:15:00Z"
            }}),
            result_eq: ptrs([
                ("/success", json!(true)),
                ("/organization/name", json!("Acme SE")),
                ("/organization/domain_names", json!("acme.test, acme.de")),
            ]),
        },
    ];
    support::assert_full_coverage(&a, &cases);
    for c in &cases {
        run(&a, c);
    }
}

/// Live read-only calls through the hub exec path. Requires
/// `CTOX_TOOLHUB_LIVE_ZENDESK_TOKEN` (an OAuth2 token) and an account whose
/// subdomain matches the registered adapter's base URL.
#[test]
#[ignore]
fn live_read_only_actions() {
    let a = adapter();
    for (tool, args) in [
        ("ZENDESK_LIST_TICKETS", json!({"per_page": 1})),
        ("ZENDESK_LIST_USERS", json!({"per_page": 1})),
        ("ZENDESK_FIND_TICKETS", json!({"search_type": "status", "status": "open"})),
    ] {
        let Some(result) = live(&a, tool, args) else {
            return;
        };
        let v = result.unwrap_or_else(|e| panic!("{tool}: {e}"));
        assert!(v.is_object(), "{tool}: expected an object, got {v}");
    }
}