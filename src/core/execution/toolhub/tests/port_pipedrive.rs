//! Port tests for the Pipedrive toolkit.
//! DE: Ein aufgezeichneter Fall pro Aktion, abgeleitet aus dem Activepieces-run()-Code.
//! EN: one recorded case per action, derived from the Activepieces run() code.
mod support;

use ctox_toolhub::model::AuthScheme;
use ctox_toolhub::toolkits::pipedrive::Pipedrive;
use ctox_toolhub::toolkits::ToolkitAdapter;
use serde_json::{json, Value};
use support::{assert_full_coverage, live, run, Case};

/// Leak the expectation slices so the `Case`s can borrow them for `'static`.
fn st(v: Vec<(&'static str, Value)>) -> &'static [(&'static str, Value)] {
    Box::leak(v.into_boxed_slice())
}
fn sts(v: Vec<&'static str>) -> &'static [&'static str] {
    Box::leak(v.into_boxed_slice())
}

const BASE: &str = "https://api.pipedrive.test";

/// `DEAL_OPTIONAL_FIELDS` in request order, sent as `include_fields`.
const DEAL_FIELDS: &str = "include_fields=next_activity_id%2Clast_activity_id%2Cfirst_won_time%2Cproducts_count%2Cfiles_count%2Cnotes_count%2Cfollowers_count%2Cemail_messages_count%2Cactivities_count%2Cdone_activities_count%2Cundone_activities_count%2Cparticipants_count%2Clast_incoming_mail_time%2Clast_outgoing_mail_time%2Csmart_bcc_email";
/// `PERSON_OPTIONAL_FIELDS` in request order.
const PERSON_FIELDS: &str = "include_fields=next_activity_id%2Clast_activity_id%2Copen_deals_count%2Crelated_open_deals_count%2Cclosed_deals_count%2Crelated_closed_deals_count%2Cparticipant_open_deals_count%2Cparticipant_closed_deals_count%2Cemail_messages_count%2Cactivities_count%2Cdone_activities_count%2Cundone_activities_count%2Cfiles_count%2Cnotes_count%2Cfollowers_count%2Crelated_won_deals_count%2Clost_deals_count%2Crelated_lost_deals_count%2Clast_incoming_mail_time%2Clast_outgoing_mail_time%2Cwon_deals_count";
/// `ORGANIZATION_OPTIONAL_FIELDS` in request order.
const ORG_FIELDS: &str = "include_fields=next_activity_id%2Clast_activity_id%2Copen_deals_count%2Crelated_open_deals_count%2Cclosed_deals_count%2Crelated_closed_deals_count%2Cemail_messages_count%2Cpeople_count%2Cactivities_count%2Cdone_activities_count%2Cundone_activities_count%2Cfiles_count%2Cnotes_count%2Cfollowers_count%2Cwon_deals_count%2Crelated_won_deals_count%2Clost_deals_count%2Crelated_lost_deals_count";

fn cases() -> Vec<Case<'static>> {
    vec![
        // ---- reads ---------------------------------------------------------
        // AP: src/lib/actions/get-current-user.ts
        Case {
            tool: "PIPEDRIVE_GET_CURRENT_USER",
            args: json!({}),
            method: "GET",
            url_contains: sts(vec!["/api/v1/users/me"]),
            body_eq: &[],
            response_status: 200,
            response: json!({
                "success": true,
                "data": {"id": 12, "name": "Ada Lovelace", "email": "ada@example.com",
                    "company_id": 401, "is_admin": 1, "locale": "de_DE",
                    "timezone_name": "Europe/Berlin", "default_currency": "EUR"}
            }),
            result_eq: st(vec![
                ("/id", json!(12)),
                ("/email", json!("ada@example.com")),
                ("/timezone_name", json!("Europe/Berlin")),
            ]),
        },
        // AP: src/lib/actions/list-users.ts
        Case {
            tool: "PIPEDRIVE_LIST_USERS",
            args: json!({}),
            method: "GET",
            url_contains: sts(vec!["/api/v1/users"]),
            body_eq: &[],
            response_status: 200,
            response: json!({
                "success": true,
                "data": [
                    {"id": 12, "name": "Ada Lovelace", "email": "ada@example.com",
                        "active_flag": true, "is_admin": 1, "default_currency": "EUR",
                        "timezone_name": "Europe/Berlin"},
                    {"id": 15, "name": "Grace Hopper", "email": "grace@example.com",
                        "active_flag": true, "is_admin": 0, "default_currency": "EUR",
                        "timezone_name": "Europe/Berlin"}
                ],
                "additional_data": {"pagination": {"start": 0, "limit": 100,
                    "more_items_in_collection": false}}
            }),
            result_eq: st(vec![
                ("/found", json!(true)),
                ("/data/0/id", json!(12)),
                ("/data/1/email", json!("grace@example.com")),
                ("/more_items", json!(false)),
            ]),
        },
        // AP: src/lib/actions/list-pipelines.ts
        Case {
            tool: "PIPEDRIVE_LIST_PIPELINES",
            args: json!({"limit": 2, "cursor": "cur-pipelines-1"}),
            method: "GET",
            url_contains: sts(vec![
                "/api/v2/pipelines?",
                "cursor=cur-pipelines-1",
                "limit=2",
            ]),
            body_eq: &[],
            response_status: 200,
            response: json!({
                "success": true,
                "data": [{"id": 1, "name": "Sales Pipeline", "url_title": "Sales-Pipeline",
                    "order_nr": 1, "active": true, "deal_probability": false,
                    "add_time": "2024-01-05T10:00:00Z", "update_time": "2026-08-02T09:12:00Z"}],
                "additional_data": {"next_cursor": "cur-pipelines-2"}
            }),
            result_eq: st(vec![
                ("/data/0/name", json!("Sales Pipeline")),
                ("/next_cursor", json!("cur-pipelines-2")),
            ]),
        },
        // AP: src/lib/actions/list-stages.ts
        Case {
            tool: "PIPEDRIVE_LIST_STAGES",
            args: json!({"pipeline_id": 3, "limit": 100}),
            method: "GET",
            url_contains: sts(vec!["/api/v2/stages?pipeline_id=3&limit=100"]),
            body_eq: &[],
            response_status: 200,
            response: json!({
                "success": true,
                "data": [
                    {"id": 7, "name": "Qualification", "pipeline_id": 3, "order_nr": 1, "active": true},
                    {"id": 8, "name": "Proposal", "pipeline_id": 3, "order_nr": 2, "active": true}
                ],
                "additional_data": {}
            }),
            result_eq: st(vec![
                ("/found", json!(true)),
                ("/data/1/name", json!("Proposal")),
                ("/next_cursor", json!(null)),
            ]),
        },
        // AP: src/lib/actions/get-deal.ts
        Case {
            tool: "PIPEDRIVE_GET_DEAL",
            args: json!({"deal_id": 209}),
            method: "GET",
            url_contains: sts(vec!["/api/v2/deals/209?", DEAL_FIELDS]),
            body_eq: &[],
            response_status: 200,
            response: json!({
                "success": true,
                "data": {"id": 209, "title": "Website Relaunch", "value": 18000,
                    "currency": "EUR", "status": "open", "stage_id": 8, "pipeline_id": 1,
                    "person_id": 34, "org_id": 12, "owner_id": 12,
                    "expected_close_date": "2026-11-30", "products_count": 1,
                    "notes_count": 3, "done_activities_count": 1,
                    "undone_activities_count": 2, "followers_count": 2}
            }),
            result_eq: st(vec![
                ("/title", json!("Website Relaunch")),
                ("/value", json!(18000)),
                ("/products_count", json!(1)),
            ]),
        },
        // AP: src/lib/actions/list-deals.ts
        Case {
            tool: "PIPEDRIVE_LIST_DEALS",
            args: json!({"owner_id": 12, "status": "open", "updated_since": "2026-09-01", "limit": 50}),
            method: "GET",
            url_contains: sts(vec![
                "/api/v2/deals?owner_id=12&status=open&updated_since=2026-09-01T00%3A00%3A00Z&limit=50",
            ]),
            body_eq: &[],
            response_status: 200,
            response: json!({
                "success": true,
                "data": [
                    {"id": 209, "title": "Website Relaunch", "value": 18000, "currency": "EUR",
                        "status": "open", "stage_id": 8, "person_id": 34, "org_id": 12, "owner_id": 12},
                    {"id": 233, "title": "Maintenance 2027", "value": 4800, "currency": "EUR",
                        "status": "open", "stage_id": 5, "person_id": 41, "owner_id": 12}
                ],
                "additional_data": {}
            }),
            result_eq: st(vec![
                ("/found", json!(true)),
                ("/data/1/id", json!(233)),
                ("/data/0/currency", json!("EUR")),
                ("/next_cursor", json!(null)),
            ]),
        },
        // AP: src/lib/actions/search-deals.ts
        Case {
            tool: "PIPEDRIVE_SEARCH_DEALS",
            args: json!({"term": "Relaunch", "fields": ["title", "notes"], "status": "open", "limit": 50}),
            method: "GET",
            url_contains: sts(vec![
                "/api/v2/deals/search?term=Relaunch&fields=title%2Cnotes&status=open&limit=50",
            ]),
            body_eq: &[],
            response_status: 200,
            response: json!({
                "success": true,
                "data": {"items": [
                    {"result_score": 0.85, "item": {"id": 209, "title": "Website Relaunch",
                        "value": 18000, "currency": "EUR", "status": "open",
                        "person_name": "Ada Lovelace"}}
                ]},
                "additional_data": {}
            }),
            result_eq: st(vec![
                ("/found", json!(true)),
                ("/data/0/id", json!(209)),
                ("/data/0/title", json!("Website Relaunch")),
                ("/data/0/result_score", json!(0.85)),
            ]),
        },
        // AP: src/lib/actions/list-deal-products.ts
        Case {
            tool: "PIPEDRIVE_LIST_DEAL_PRODUCTS",
            args: json!({"deal_id": 209, "limit": 100}),
            method: "GET",
            url_contains: sts(vec!["/api/v2/deals/209/products?limit=100"]),
            body_eq: &[],
            response_status: 200,
            response: json!({
                "success": true,
                "data": [{"id": 912, "deal_id": 209, "product_id": 88, "name": "CRM Seats",
                    "item_price": 2500.5, "quantity": 3, "currency": "EUR",
                    "enabled_flag": true, "add_time": "2026-09-10T08:00:00Z"}],
                "additional_data": {}
            }),
            result_eq: st(vec![
                ("/data/0/product_id", json!(88)),
                ("/data/0/quantity", json!(3)),
            ]),
        },
        // AP: src/lib/actions/get-person.ts
        Case {
            tool: "PIPEDRIVE_GET_PERSON",
            args: json!({"person_id": 34}),
            method: "GET",
            url_contains: sts(vec!["/api/v2/persons/34?", PERSON_FIELDS]),
            body_eq: &[],
            response_status: 200,
            response: json!({
                "success": true,
                "data": {"id": 34, "name": "Ada Lovelace", "first_name": "Ada",
                    "last_name": "Lovelace", "owner_id": 12, "org_id": 12,
                    "email": [{"value": "ada@example.com", "label": "work", "primary": true}],
                    "phone": [{"value": "+49 30 123456", "label": "work", "primary": true}],
                    "open_deals_count": 2, "activities_count": 5, "notes_count": 2,
                    "last_incoming_mail_time": "2026-09-30T12:00:00Z"}
            }),
            result_eq: st(vec![
                ("/name", json!("Ada Lovelace")),
                ("/email/0/value", json!("ada@example.com")),
                ("/open_deals_count", json!(2)),
            ]),
        },
        // AP: src/lib/actions/search-persons.ts
        Case {
            tool: "PIPEDRIVE_SEARCH_PERSONS",
            args: json!({"term": "ada@example.com", "exact_match": true, "limit": 50}),
            method: "GET",
            url_contains: sts(vec![
                "/api/v2/persons/search?term=ada%40example.com&exact_match=true&limit=50",
            ]),
            body_eq: &[],
            response_status: 200,
            response: json!({
                "success": true,
                "data": {"items": [
                    {"result_score": 1.0, "item": {"id": 34, "name": "Ada Lovelace",
                        "emails": [{"value": "ada@example.com"}], "owner_id": 12}}
                ]},
                "additional_data": {}
            }),
            result_eq: st(vec![
                ("/found", json!(true)),
                ("/data/0/id", json!(34)),
                ("/data/0/result_score", json!(1.0)),
            ]),
        },
        // AP: src/lib/actions/get-organization.ts
        Case {
            tool: "PIPEDRIVE_GET_ORGANIZATION",
            args: json!({"organization_id": 12}),
            method: "GET",
            url_contains: sts(vec!["/api/v2/organizations/12?", ORG_FIELDS]),
            body_eq: &[],
            response_status: 200,
            response: json!({
                "success": true,
                "data": {"id": 12, "name": "Example GmbH",
                    "address": {"value": "Alexanderplatz 1, Berlin"}, "owner_id": 12,
                    "people_count": 4, "open_deals_count": 2, "closed_deals_count": 5,
                    "won_deals_count": 4, "activities_count": 9}
            }),
            result_eq: st(vec![
                ("/name", json!("Example GmbH")),
                ("/address/value", json!("Alexanderplatz 1, Berlin")),
                ("/people_count", json!(4)),
            ]),
        },
        // AP: src/lib/actions/search-organizations.ts
        Case {
            tool: "PIPEDRIVE_SEARCH_ORGANIZATIONS",
            args: json!({"term": "Example", "fields": ["name"], "limit": 50}),
            method: "GET",
            url_contains: sts(vec![
                "/api/v2/organizations/search?term=Example&fields=name&limit=50",
            ]),
            body_eq: &[],
            response_status: 200,
            response: json!({
                "success": true,
                "data": {"items": [
                    {"result_score": 0.9, "item": {"id": 12, "name": "Example GmbH",
                        "address": {"value": "Alexanderplatz 1, Berlin"}}}
                ]},
                "additional_data": {}
            }),
            result_eq: st(vec![
                ("/data/0/id", json!(12)),
                ("/data/0/name", json!("Example GmbH")),
            ]),
        },
        // AP: src/lib/actions/list-activities.ts
        Case {
            tool: "PIPEDRIVE_LIST_ACTIVITIES",
            args: json!({"deal_id": 209, "done": false, "sort_by": "due_date",
                "sort_direction": "asc", "limit": 50}),
            method: "GET",
            url_contains: sts(vec![
                "/api/v2/activities?deal_id=209&done=false&sort_by=due_date&sort_direction=asc&limit=50",
            ]),
            body_eq: &[],
            response_status: 200,
            response: json!({
                "success": true,
                "data": [
                    {"id": 71, "subject": "Kickoff call", "type": "call",
                        "due_date": "2026-10-20", "due_time": "10:00", "done": false,
                        "deal_id": 209, "org_id": 12, "owner_id": 12,
                        "person_id": {"id": 34, "name": "Ada Lovelace"}}
                ],
                "additional_data": {}
            }),
            result_eq: st(vec![
                ("/found", json!(true)),
                ("/data/0/subject", json!("Kickoff call")),
                ("/data/0/due_date", json!("2026-10-20")),
            ]),
        },
        // AP: src/lib/actions/get-lead.ts
        Case {
            tool: "PIPEDRIVE_GET_LEAD",
            args: json!({"lead_id": "55b04240-6ec5-4d5d-a10c-0bdeb61e4b78"}),
            method: "GET",
            url_contains: sts(vec!["/api/v1/leads/55b04240-6ec5-4d5d-a10c-0bdeb61e4b78"]),
            body_eq: &[],
            response_status: 200,
            response: json!({
                "success": true,
                "data": {"id": "55b04240-6ec5-4d5d-a10c-0bdeb61e4b78",
                    "title": "Inbound: Website Relaunch", "owner_id": 12, "creator_id": 12,
                    "value": {"amount": 5000, "currency": "EUR"},
                    "person_id": {"id": 34, "name": "Ada Lovelace"}, "organization_id": null,
                    "is_archived": false, "was_seen": true, "add_time": "2026-09-28T11:22:00Z"}
            }),
            result_eq: st(vec![
                ("/title", json!("Inbound: Website Relaunch")),
                ("/value/amount", json!(5000)),
                ("/person_id/id", json!(34)),
            ]),
        },
        // AP: src/lib/actions/find-leads.ts
        Case {
            tool: "PIPEDRIVE_FIND_LEAD",
            args: json!({"term": "Inbound"}),
            method: "GET",
            url_contains: sts(vec!["/api/v2/leads/search?term=Inbound&fields=title&limit=100"]),
            body_eq: &[],
            response_status: 200,
            response: json!({
                "success": true,
                "data": {"items": [
                    {"result_score": 0.61, "item": {"id": "55b04240-6ec5-4d5d-a10c-0bdeb61e4b78",
                        "title": "Inbound: Website Relaunch",
                        "person_id": {"id": 34, "name": "Ada Lovelace"},
                        "value": {"amount": 5000, "currency": "EUR"}}}
                ]},
                "additional_data": {}
            }),
            result_eq: st(vec![
                ("/found", json!(true)),
                ("/data/0/id", json!("55b04240-6ec5-4d5d-a10c-0bdeb61e4b78")),
                ("/data/0/result_score", json!(0.61)),
            ]),
        },
        // AP: src/lib/actions/find-notes.ts
        Case {
            tool: "PIPEDRIVE_FIND_NOTES",
            args: json!({"object_type": "deal_id", "object_id": "209"}),
            method: "GET",
            url_contains: sts(vec![
                "/api/v1/notes?sort=update_time+DESC&deal_id=209&start=0&limit=500",
            ]),
            body_eq: &[],
            response_status: 200,
            response: json!({
                "success": true,
                "data": [
                    {"id": 501, "content": "Budget approved by CFO", "deal_id": 209,
                        "user_id": 12, "pinned_to_deal_flag": 1,
                        "add_time": "2026-10-01T09:15:00Z", "update_time": "2026-10-02T10:00:00Z"},
                    {"id": 478, "content": "Initial requirements call", "deal_id": 209,
                        "user_id": 15, "pinned_to_deal_flag": 0,
                        "add_time": "2026-09-28T11:30:00Z", "update_time": "2026-09-28T11:30:00Z"}
                ],
                "additional_data": {"pagination": {"start": 0, "limit": 500,
                    "more_items_in_collection": false}}
            }),
            result_eq: st(vec![
                ("/found", json!(true)),
                ("/data/1/id", json!(478)),
                ("/data/0/content", json!("Budget approved by CFO")),
                ("/more_items", json!(false)),
            ]),
        },
        // AP: src/lib/actions/find-products.ts
        Case {
            tool: "PIPEDRIVE_FIND_PRODUCTS",
            args: json!({"field": "code", "field_value": "CRM-100"}),
            method: "GET",
            url_contains: sts(vec![
                "/api/v2/products/search?term=CRM-100&fields=code&exact_match=true",
            ]),
            body_eq: &[],
            response_status: 200,
            response: json!({
                "success": true,
                "data": {"items": [
                    {"result_score": 1, "item": {"id": 88, "name": "CRM Seats", "code": "CRM-100",
                        "prices": [{"price": 2500.5, "currency": "EUR", "cost": 0,
                            "direct_cost": 0, "overhead_cost": 0}], "active_flag": true}}
                ]},
                "additional_data": {}
            }),
            result_eq: st(vec![
                ("/found", json!(true)),
                ("/data/0/id", json!(88)),
                ("/data/0/code", json!("CRM-100")),
            ]),
        },
        // ---- writes --------------------------------------------------------
        // AP: src/lib/actions/create-deal.ts
        Case {
            tool: "PIPEDRIVE_CREATE_DEAL",
            args: json!({
                "title": "Website Relaunch", "value": 18000, "currency": "EUR",
                "pipeline_id": 1, "stage_id": 8, "person_id": 34, "organization_id": 12,
                "owner_id": 12, "label_ids": [3, 5], "visible_to": 3,
                "expected_close_date": "2026-11-30", "creation_time": "2026-10-14T08:30:00Z"
            }),
            method: "POST",
            url_contains: sts(vec!["/api/v2/deals"]),
            body_eq: st(vec![
                ("/title", json!("Website Relaunch")),
                ("/value", json!(18000)),
                ("/currency", json!("EUR")),
                ("/pipeline_id", json!(1)),
                ("/stage_id", json!(8)),
                ("/person_id", json!(34)),
                ("/org_id", json!(12)),
                ("/owner_id", json!(12)),
                ("/label_ids", json!([3, 5])),
                ("/visible_to", json!(3)),
                ("/expected_close_date", json!("2026-11-30")),
                ("/add_time", json!("2026-10-14T08:30:00Z")),
            ]),
            response_status: 200,
            response: json!({
                "success": true,
                "data": {"id": 209, "title": "Website Relaunch", "value": 18000,
                    "currency": "EUR", "stage_id": 8, "status": "open", "owner_id": 12,
                    "org_id": 12, "person_id": 34, "label_ids": [3, 5], "visible_to": 3,
                    "expected_close_date": "2026-11-30", "add_time": "2026-10-14T08:30:00Z"}
            }),
            result_eq: st(vec![
                ("/id", json!(209)),
                ("/add_time", json!("2026-10-14T08:30:00Z")),
                ("/status", json!("open")),
            ]),
        },
        // AP: src/lib/actions/update-deal.ts
        Case {
            tool: "PIPEDRIVE_UPDATE_DEAL",
            args: json!({"deal_id": 209, "value": 21000, "status": "won"}),
            method: "PATCH",
            url_contains: sts(vec!["/api/v2/deals/209"]),
            body_eq: st(vec![("/value", json!(21000)), ("/status", json!("won"))]),
            response_status: 200,
            response: json!({
                "success": true,
                "data": {"id": 209, "title": "Website Relaunch", "value": 21000,
                    "currency": "EUR", "status": "won", "stage_id": 8,
                    "update_time": "2026-10-14T09:00:00Z"}
            }),
            result_eq: st(vec![
                ("/id", json!(209)),
                ("/status", json!("won")),
                ("/value", json!(21000)),
            ]),
        },
        // AP: src/lib/actions/delete-deal.ts
        Case {
            tool: "PIPEDRIVE_DELETE_DEAL",
            args: json!({"deal_id": 209}),
            method: "DELETE",
            url_contains: sts(vec!["/api/v2/deals/209"]),
            body_eq: &[],
            response_status: 200,
            response: json!({"success": true, "data": {"id": 209}}),
            result_eq: st(vec![("/id", json!(209))]),
        },
        // AP: src/lib/actions/create-person.ts
        Case {
            tool: "PIPEDRIVE_CREATE_PERSON",
            args: json!({
                "first_name": "Ada", "last_name": "Lovelace", "owner_id": 12,
                "organization_id": 12, "email": ["ada@example.com", "ada@calc.org"],
                "phone": ["+49 30 123456"], "label_ids": [7]
            }),
            method: "POST",
            url_contains: sts(vec!["/api/v2/persons"]),
            body_eq: st(vec![
                ("/first_name", json!("Ada")),
                ("/last_name", json!("Lovelace")),
                ("/owner_id", json!(12)),
                ("/org_id", json!(12)),
                ("/label_ids", json!([7])),
                ("/emails/0", json!({"value": "ada@example.com", "label": "work", "primary": true})),
                ("/emails/1", json!({"value": "ada@calc.org", "label": "work", "primary": false})),
                ("/phones/0", json!({"value": "+49 30 123456", "label": "work", "primary": true})),
            ]),
            response_status: 200,
            response: json!({
                "success": true,
                "data": {"id": 34, "name": "Ada Lovelace", "first_name": "Ada",
                    "last_name": "Lovelace", "owner_id": 12, "org_id": 12,
                    "email": [{"value": "ada@example.com", "label": "work", "primary": true}],
                    "phone": [{"value": "+49 30 123456", "label": "work", "primary": true}],
                    "label_ids": [7]}
            }),
            result_eq: st(vec![
                ("/id", json!(34)),
                ("/name", json!("Ada Lovelace")),
                ("/email/0/value", json!("ada@example.com")),
            ]),
        },
        // AP: src/lib/actions/update-person.ts
        Case {
            tool: "PIPEDRIVE_UPDATE_PERSON",
            args: json!({"person_id": 34, "marketing_status": "subscribed"}),
            method: "PATCH",
            url_contains: sts(vec!["/api/v2/persons/34"]),
            body_eq: st(vec![("/marketing_status", json!("subscribed"))]),
            response_status: 200,
            response: json!({
                "success": true,
                "data": {"id": 34, "name": "Ada Lovelace",
                    "marketing_status": "subscribed", "update_time": "2026-10-14T09:05:00Z"}
            }),
            result_eq: st(vec![
                ("/id", json!(34)),
                ("/marketing_status", json!("subscribed")),
            ]),
        },
        // AP: src/lib/actions/create-organization.ts
        Case {
            tool: "PIPEDRIVE_CREATE_ORGANIZATION",
            args: json!({
                "name": "Example GmbH", "address": "Alexanderplatz 1, Berlin",
                "owner_id": 12, "visible_to": 3
            }),
            method: "POST",
            url_contains: sts(vec!["/api/v2/organizations"]),
            body_eq: st(vec![
                ("/name", json!("Example GmbH")),
                ("/address", json!({"value": "Alexanderplatz 1, Berlin"})),
                ("/owner_id", json!(12)),
                ("/visible_to", json!(3)),
            ]),
            response_status: 200,
            response: json!({
                "success": true,
                "data": {"id": 12, "name": "Example GmbH",
                    "address": {"value": "Alexanderplatz 1, Berlin"}, "owner_id": 12,
                    "visible_to": 3, "add_time": "2026-10-14T08:00:00Z"}
            }),
            result_eq: st(vec![
                ("/id", json!(12)),
                ("/name", json!("Example GmbH")),
                ("/address/value", json!("Alexanderplatz 1, Berlin")),
            ]),
        },
        // AP: src/lib/actions/update-organization.ts
        Case {
            tool: "PIPEDRIVE_UPDATE_ORGANIZATION",
            args: json!({"organization_id": 12, "address": "Marienplatz 2, München"}),
            method: "PATCH",
            url_contains: sts(vec!["/api/v2/organizations/12"]),
            body_eq: st(vec![("/address", json!({"value": "Marienplatz 2, München"}))]),
            response_status: 200,
            response: json!({
                "success": true,
                "data": {"id": 12, "name": "Example GmbH",
                    "address": {"value": "Marienplatz 2, München"},
                    "update_time": "2026-10-14T09:10:00Z"}
            }),
            result_eq: st(vec![
                ("/id", json!(12)),
                ("/address/value", json!("Marienplatz 2, München")),
            ]),
        },
        // AP: src/lib/actions/create-lead.ts
        Case {
            tool: "PIPEDRIVE_CREATE_LEAD",
            args: json!({
                "title": "Inbound: Website Relaunch", "person_id": 34, "value": 5000,
                "currency": "EUR", "owner_id": 12, "expected_close_date": "2026-12-15"
            }),
            method: "POST",
            url_contains: sts(vec!["/api/v1/leads"]),
            body_eq: st(vec![
                ("/title", json!("Inbound: Website Relaunch")),
                ("/person_id", json!(34)),
                ("/owner_id", json!(12)),
                ("/expected_close_date", json!("2026-12-15")),
                ("/value", json!({"amount": 5000.0, "currency": "EUR"})),
            ]),
            response_status: 200,
            response: json!({
                "success": true,
                "data": {"id": "9f1c2a34-77c1-4d2e-9c3a-4b2f9f2c1111",
                    "title": "Inbound: Website Relaunch", "owner_id": 12,
                    "person_id": {"id": 34, "name": "Ada Lovelace"},
                    "value": {"amount": 5000, "currency": "EUR"},
                    "expected_close_date": "2026-12-15", "is_archived": false,
                    "add_time": "2026-10-14T08:20:00Z"}
            }),
            result_eq: st(vec![
                ("/id", json!("9f1c2a34-77c1-4d2e-9c3a-4b2f9f2c1111")),
                ("/value/currency", json!("EUR")),
                ("/value/amount", json!(5000)),
            ]),
        },
        // AP: src/lib/actions/create-activity.ts
        Case {
            tool: "PIPEDRIVE_CREATE_ACTIVITY",
            args: json!({
                "subject": "Kickoff call", "type": "call", "due_date": "2026-10-20",
                "due_time": "10:00", "duration": "00:30", "assign_to": 12,
                "deal_id": 209, "person_id": 34, "busy": "busy",
                "note": "Agenda: scope and timeline"
            }),
            method: "POST",
            url_contains: sts(vec!["/api/v2/activities"]),
            body_eq: st(vec![
                ("/subject", json!("Kickoff call")),
                ("/type", json!("call")),
                ("/due_date", json!("2026-10-20")),
                ("/due_time", json!("10:00")),
                ("/duration", json!("00:30")),
                ("/owner_id", json!(12)),
                ("/deal_id", json!(209)),
                ("/note", json!("Agenda: scope and timeline")),
                ("/participants", json!([{"person_id": 34, "primary": true}])),
                ("/busy", json!(true)),
            ]),
            response_status: 200,
            response: json!({
                "success": true,
                "data": {"id": 71, "subject": "Kickoff call", "type": "call",
                    "due_date": "2026-10-20", "due_time": "10:00", "duration": "00:30",
                    "busy_flag": true, "done": false, "deal_id": 209, "org_id": 12,
                    "owner_id": 12, "participants": [{"person_id": 34, "primary": true}]}
            }),
            result_eq: st(vec![
                ("/id", json!(71)),
                ("/subject", json!("Kickoff call")),
                ("/participants/0/person_id", json!(34)),
            ]),
        },
        // AP: src/lib/actions/update-activity.ts
        Case {
            tool: "PIPEDRIVE_UPDATE_ACTIVITY",
            args: json!({"activity_id": 71, "is_done": true}),
            method: "PATCH",
            url_contains: sts(vec!["/api/v2/activities/71"]),
            body_eq: st(vec![("/done", json!(true))]),
            response_status: 200,
            response: json!({
                "success": true,
                "data": {"id": 71, "subject": "Kickoff call", "type": "call",
                    "due_date": "2026-10-20", "due_time": "10:00", "done": true,
                    "update_time": "2026-10-14T09:15:00Z"}
            }),
            result_eq: st(vec![("/id", json!(71)), ("/done", json!(true))]),
        },
        // AP: src/lib/actions/create-note.ts
        Case {
            tool: "PIPEDRIVE_CREATE_NOTE",
            args: json!({"content": "Budget approved by CFO", "deal_id": 209,
                "pinned_to_deal": true}),
            method: "POST",
            url_contains: sts(vec!["/api/v1/notes"]),
            body_eq: st(vec![
                ("/content", json!("Budget approved by CFO")),
                ("/deal_id", json!(209)),
                ("/pinned_to_deal_flag", json!(1)),
                ("/pinned_to_person_flag", json!(0)),
                ("/pinned_to_organization_flag", json!(0)),
                ("/pinned_to_lead_flag", json!(0)),
            ]),
            response_status: 200,
            response: json!({
                "success": true,
                "data": {"id": 501, "content": "Budget approved by CFO", "deal_id": 209,
                    "user_id": 12, "pinned_to_deal_flag": 1,
                    "add_time": "2026-10-14T09:20:00Z"}
            }),
            result_eq: st(vec![
                ("/id", json!(501)),
                ("/content", json!("Budget approved by CFO")),
                ("/pinned_to_deal_flag", json!(1)),
            ]),
        },
        // AP: src/lib/actions/update-note.ts
        Case {
            tool: "PIPEDRIVE_UPDATE_NOTE",
            args: json!({"note_id": 501, "content": "Budget approved (updated)",
                "pinned_to_deal": 0}),
            method: "PUT",
            url_contains: sts(vec!["/api/v1/notes/501"]),
            body_eq: st(vec![
                ("/content", json!("Budget approved (updated)")),
                ("/pinned_to_deal_flag", json!(0)),
            ]),
            response_status: 200,
            response: json!({
                "success": true,
                "data": {"id": 501, "content": "Budget approved (updated)", "deal_id": 209,
                    "user_id": 12, "pinned_to_deal_flag": 0,
                    "update_time": "2026-10-14T09:25:00Z"}
            }),
            result_eq: st(vec![
                ("/id", json!(501)),
                ("/content", json!("Budget approved (updated)")),
            ]),
        },
        // AP: src/lib/actions/add-product-to-deal.ts
        Case {
            tool: "PIPEDRIVE_ADD_PRODUCT_TO_DEAL",
            args: json!({
                "deal_id": 209, "product_id": 88, "item_price": 2500.5, "quantity": 3,
                "discount": 10, "discount_type": "percentage", "tax": 19,
                "tax_method": "inclusive", "comments": "Annual license"
            }),
            method: "POST",
            url_contains: sts(vec!["/api/v2/deals/209/products"]),
            body_eq: st(vec![
                ("/product_id", json!(88)),
                ("/item_price", json!(2500.5)),
                ("/quantity", json!(3)),
                ("/discount", json!(10)),
                ("/discount_type", json!("percentage")),
                ("/tax", json!(19)),
                ("/tax_method", json!("inclusive")),
                ("/comments", json!("Annual license")),
            ]),
            response_status: 200,
            response: json!({
                "success": true,
                "data": {"id": 912, "deal_id": 209, "product_id": 88, "item_price": 2500.5,
                    "quantity": 3, "discount": 10, "discount_type": "percentage",
                    "currency": "EUR", "tax": 19, "tax_method": "inclusive",
                    "comments": "Annual license", "is_enabled": true}
            }),
            result_eq: st(vec![
                ("/id", json!(912)),
                ("/product_id", json!(88)),
                ("/quantity", json!(3)),
            ]),
        },
    ]
}

#[test]
fn recorded_requests_match_activepieces() {
    let adapter = adapter();
    let all = cases();
    run(&adapter, &all[0]);
    for c in &all {
        run(&adapter, c);
    }
    assert_full_coverage(&adapter, &all);
}

fn adapter() -> Pipedrive {
    Pipedrive::with_api_base(BASE)
}

#[test]
fn catalog_matches_piece_auth_and_policy() {
    let adapter = adapter();
    let spec = adapter.spec();
    assert_eq!(spec.slug, "pipedrive");
    assert_eq!(spec.tools.len(), 30, "expected 30 ported actions");
    let mut slugs: Vec<&str> = spec.tools.iter().map(|t| t.slug.as_str()).collect();
    slugs.sort_unstable();
    let unique = slugs.windows(2).all(|w| w[0] != w[1]);
    assert!(unique, "duplicate tool slugs");

    let oauth = spec
        .auth_schemes
        .iter()
        .find_map(|a| match a {
            AuthScheme::OAuth2(o) => Some(o),
            _ => None,
        })
        .expect("pipedrive is oauth2-only");
    assert_eq!(oauth.authorize_url, "https://oauth.pipedrive.com/oauth/authorize");
    assert_eq!(oauth.token_url, "https://oauth.pipedrive.com/oauth/token");
    assert!(!oauth.pkce);
    let defaults: Vec<&str> = oauth.default_scopes.iter().map(String::as_str).collect();
    for expected in [
        "base",
        "admin",
        "contacts:full",
        "users:read",
        "deals:full",
        "activities:full",
        "leads:full",
        "products:full",
        "webhooks:full",
    ] {
        assert!(defaults.contains(&expected), "missing piece scope {expected}");
    }

    let mutating = [
        "PIPEDRIVE_CREATE_DEAL",
        "PIPEDRIVE_UPDATE_DEAL",
        "PIPEDRIVE_DELETE_DEAL",
        "PIPEDRIVE_CREATE_PERSON",
        "PIPEDRIVE_UPDATE_PERSON",
        "PIPEDRIVE_CREATE_ORGANIZATION",
        "PIPEDRIVE_UPDATE_ORGANIZATION",
        "PIPEDRIVE_CREATE_LEAD",
        "PIPEDRIVE_CREATE_ACTIVITY",
        "PIPEDRIVE_UPDATE_ACTIVITY",
        "PIPEDRIVE_CREATE_NOTE",
        "PIPEDRIVE_UPDATE_NOTE",
        "PIPEDRIVE_ADD_PRODUCT_TO_DEAL",
    ];
    for t in &spec.tools {
        let should_write = mutating.contains(&t.slug.as_str());
        assert_eq!(t.mutating, should_write, "{}", t.slug);
        assert!(!t.scopes.is_empty(), "{} has no scopes", t.slug);
        assert!(!t.title.de.is_empty() && !t.title.en.is_empty(), "{}", t.slug);
        assert!(!t.description.de.is_empty() && !t.description.en.is_empty(), "{}", t.slug);
        for sc in &t.scopes {
            assert!(defaults.contains(&sc.as_str()), "{} needs unknown scope {sc}", t.slug);
        }
    }
}

/// Live read-only calls against the real API:
/// `CTOX_TOOLHUB_LIVE_PIPEDRIVE_TOKEN=… cargo test --test port_pipedrive -- --ignored`.
#[test]
#[ignore]
fn live_read_only_calls() {
    let adapter = adapter();
    let probes: [(&str, Value); 3] = [
        ("PIPEDRIVE_GET_CURRENT_USER", json!({})),
        ("PIPEDRIVE_LIST_USERS", json!({})),
        ("PIPEDRIVE_LIST_DEALS", json!({"limit": 2})),
    ];
    let mut ran = 0;
    for (tool, args) in &probes {
        let Some(outcome) = live(&adapter, tool, args.clone()) else {
            return; // no token configured -> skip silently
        };
        ran += 1;
        outcome.unwrap_or_else(|e| panic!("{tool} failed live: {e}"));
    }
    assert_eq!(ran, probes.len());
}
