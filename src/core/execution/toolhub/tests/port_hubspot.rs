//! Port tests for the HubSpot toolkit.
//! DE: Ein aufgezeichneter Fall pro Aktion, abgeleitet aus dem Activepieces-run()-Code.
//! EN: one recorded case per action, derived from the Activepieces run() code.
mod support;

use ctox_toolhub::toolkits::hubspot::HubSpot;
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

fn cases() -> Vec<Case<'static>> {
    vec![
        // ---- search / find -------------------------------------------------
        Case {
            tool: "HUBSPOT_SEARCH_OBJECTS",
            args: json!({
                "object_type": "deals",
                "filter_groups": [{"filters": [{"propertyName": "amount", "operator": "GT", "value": "10000"}]}],
                "properties": ["dealname", "amount"],
                "sort_property_name": "createdate",
                "sort_direction": "ASCENDING",
                "limit": 2,
                "after": "cursor-1"
            }),
            method: "POST",
            url_contains: sts(vec!["/crm/v3/objects/deals/search"]),
            body_eq: st(vec![
                ("/filterGroups/0/filters/0/operator", json!("GT")),
                ("/properties", json!(["dealname", "amount"])),
                ("/sorts/0/direction", json!("ASCENDING")),
                ("/limit", json!(2)),
                ("/after", json!("cursor-1")),
            ]),
            response_status: 200,
            response: json!({
                "total": 2,
                "results": [
                    {"id": "51", "properties": {"dealname": "ACV", "amount": "25000.00"}},
                    {"id": "52", "properties": {"dealname": "Renewal", "amount": "12000.00"}}
                ],
                "paging": {"next": {"after": "cursor-2"}}
            }),
            result_eq: st(vec![
                ("/total", json!(2)),
                ("/results/0/id", json!("51")),
                ("/paging/next/after", json!("cursor-2")),
            ]),
        },
        Case {
            tool: "HUBSPOT_FIND_CONTACT",
            args: json!({"property_name": "email", "value": "ada@example.com"}),
            method: "POST",
            url_contains: sts(vec!["/crm/v3/objects/contacts/search"]),
            body_eq: st(vec![
                ("/limit", json!(200)),
                ("/filterGroups/0/filters/0/propertyName", json!("email")),
                ("/filterGroups/0/filters/0/operator", json!("EQ")),
                ("/filterGroups/0/filters/0/value", json!("ada@example.com")),
            ]),
            response_status: 200,
            response: json!({
                "total": 1,
                "results": [{"id": "101", "properties": {
                    "email": "ada@example.com", "firstname": "Ada", "lastname": "Lovelace",
                    "hs_object_id": "101", "lifecyclestage": "customer"}}],
                "paging": null
            }),
            result_eq: st(vec![
                ("/total", json!(1)),
                ("/results/0/id", json!("101")),
                ("/results/0/properties/firstname", json!("Ada")),
            ]),
        },
        Case {
            tool: "HUBSPOT_FIND_COMPANY",
            args: json!({"property_name": "domain", "value": "example.com"}),
            method: "POST",
            url_contains: sts(vec!["/crm/v3/objects/companies/search"]),
            body_eq: st(vec![
                ("/filterGroups/0/filters/0/propertyName", json!("domain")),
                ("/filterGroups/0/filters/0/value", json!("example.com")),
            ]),
            response_status: 200,
            response: json!({
                "total": 1,
                "results": [{"id": "201", "properties": {
                    "domain": "example.com", "name": "Example GmbH", "numberofemployees": "42"}}]
            }),
            result_eq: st(vec![("/results/0/properties/name", json!("Example GmbH"))]),
        },
        Case {
            tool: "HUBSPOT_FIND_DEAL",
            args: json!({"value": "Website Relaunch"}),
            method: "POST",
            url_contains: sts(vec!["/crm/v3/objects/deals/search"]),
            body_eq: st(vec![("/filterGroups/0/filters/0/propertyName", json!("dealname"))]),
            response_status: 200,
            response: json!({
                "total": 1,
                "results": [{"id": "301", "properties": {
                    "dealname": "Website Relaunch", "amount": "18000.00",
                    "dealstage": "decisionmakerboughtin", "hubspot_owner_id": "12"}}]
            }),
            result_eq: st(vec![
                ("/results/0/properties/dealname", json!("Website Relaunch")),
                ("/results/0/properties/amount", json!("18000.00")),
            ]),
        },
        Case {
            tool: "HUBSPOT_FIND_TICKET",
            args: json!({"property_name": "subject", "value": "Login fails after SSO rollout"}),
            method: "POST",
            url_contains: sts(vec!["/crm/v3/objects/tickets/search"]),
            body_eq: st(vec![("/filterGroups/0/filters/0/propertyName", json!("subject"))]),
            response_status: 200,
            response: json!({
                "total": 1,
                "results": [{"id": "401", "properties": {
                    "subject": "Login fails after SSO rollout", "hs_ticket_priority": "HIGH",
                    "hs_pipeline_stage": "2"}}]
            }),
            result_eq: st(vec![("/results/0/properties/hs_ticket_priority", json!("HIGH"))]),
        },
        // ---- get ----------------------------------------------------------
        Case {
            tool: "HUBSPOT_GET_CONTACT",
            args: json!({"contact_id": "101", "properties": ["hs_lead_status"]}),
            method: "GET",
            url_contains: sts(vec!["/crm/v3/objects/contacts/101?properties=", "email"]),
            body_eq: st(vec![]),
            response_status: 200,
            response: json!({"id": "101", "properties": {
                "email": "ada@example.com", "firstname": "Ada", "hs_lead_status": "OPEN",
                "createdate": "2026-01-14T09:12:00Z", "lastmodifieddate": "2026-03-02T11:00:00Z"}}),
            result_eq: st(vec![
                ("/id", json!("101")),
                ("/properties/hs_lead_status", json!("OPEN")),
                ("/properties/email", json!("ada@example.com")),
            ]),
        },
        Case {
            tool: "HUBSPOT_GET_COMPANY",
            args: json!({"company_id": "201"}),
            method: "GET",
            url_contains: sts(vec!["/crm/v3/objects/companies/201?properties=", "domain"]),
            body_eq: st(vec![]),
            response_status: 200,
            response: json!({"id": "201", "properties": {
                "domain": "example.com", "name": "Example GmbH", "annualrevenue": "5000000",
                "industry": "SOFTWARE"}}),
            result_eq: st(vec![("/properties/industry", json!("SOFTWARE"))]),
        },
        Case {
            tool: "HUBSPOT_GET_DEAL",
            args: json!({"deal_id": "301"}),
            method: "GET",
            url_contains: sts(vec!["/crm/v3/objects/deals/301?properties=", "amount"]),
            body_eq: st(vec![]),
            response_status: 200,
            response: json!({"id": "301", "properties": {
                "dealname": "Website Relaunch", "amount": "18000.00", "closedate": "2026-12-31T00:00:00Z"}}),
            result_eq: st(vec![("/properties/dealname", json!("Website Relaunch"))]),
        },
        Case {
            tool: "HUBSPOT_GET_TICKET",
            args: json!({"ticket_id": "401"}),
            method: "GET",
            url_contains: sts(vec!["/crm/v3/objects/tickets/401?properties=", "subject"]),
            body_eq: st(vec![]),
            response_status: 200,
            response: json!({"id": "401", "properties": {
                "subject": "Login fails after SSO rollout", "hs_ticket_id": "9001",
                "hs_resolution": "Fixed in 3.2"}}),
            result_eq: st(vec![("/properties/hs_ticket_id", json!("9001"))]),
        },
        // ---- list / read --------------------------------------------------
        Case {
            tool: "HUBSPOT_LIST_OBJECT_PROPERTIES",
            args: json!({"object_type": "contacts"}),
            method: "GET",
            url_contains: sts(vec!["/crm/v3/properties/contacts"]),
            body_eq: st(vec![]),
            response_status: 200,
            response: json!({"results": [
                {"name": "email", "label": "Email", "type": "string", "hubspotDefined": true},
                {"name": "seats_cancelled", "label": "Seats cancelled", "type": "number",
                 "hubspotDefined": false}]}),
            result_eq: st(vec![
                ("/count", json!(2)),
                ("/properties/0/name", json!("email")),
                ("/properties/1/hubspotDefined", json!(false)),
            ]),
        },
        Case {
            tool: "HUBSPOT_LIST_OWNERS",
            args: json!({"limit": 50, "after": "cur"}),
            method: "GET",
            url_contains: sts(vec!["/crm/v3/owners?", "limit=50", "after=cur"]),
            body_eq: st(vec![]),
            response_status: 200,
            response: json!({"results": [
                {"id": "12", "email": "grace@example.com", "firstName": "Grace", "lastName": "Hopper"},
                {"id": "34", "email": "alan@example.com", "firstName": "Alan", "lastName": "Turing"}],
                "paging": {"next": {"after": "cur2"}}}),
            result_eq: st(vec![("/count", json!(2)), ("/owners/0/email", json!("grace@example.com"))]),
        },
        Case {
            tool: "HUBSPOT_GET_OWNER_BY_EMAIL",
            args: json!({"email": "grace@example.com"}),
            method: "GET",
            url_contains: sts(vec!["/crm/v3/owners?email=grace%40example.com"]),
            body_eq: st(vec![]),
            response_status: 200,
            response: json!({"results": [
                {"id": "12", "email": "grace@example.com", "firstName": "Grace", "lastName": "Hopper"}]}),
            result_eq: st(vec![("/count", json!(1)), ("/owners/0/id", json!("12"))]),
        },
        Case {
            tool: "HUBSPOT_GET_OWNER_BY_ID",
            args: json!({"owner_id": "12"}),
            method: "GET",
            url_contains: sts(vec!["/crm/v3/owners/12"]),
            body_eq: st(vec![]),
            response_status: 200,
            response: json!({"id": "12", "email": "grace@example.com", "firstName": "Grace",
                "lastName": "Hopper", "userId": "55", "archived": false}),
            result_eq: st(vec![("/id", json!("12")), ("/userId", json!("55"))]),
        },
        Case {
            tool: "HUBSPOT_LIST_PIPELINES",
            args: json!({"object_type": "deals"}),
            method: "GET",
            url_contains: sts(vec!["/crm/v3/pipelines/deals"]),
            body_eq: st(vec![]),
            response_status: 200,
            response: json!({"results": [
                {"id": "default", "label": "Sales Pipeline", "displayOrder": 0, "stages": [
                    {"id": "appointmentscheduled", "label": "Appointment Scheduled"},
                    {"id": "decisionmakerboughtin", "label": "Decision Maker Bought-In"}]}]}),
            result_eq: st(vec![
                ("/count", json!(1)),
                ("/pipelines/0/label", json!("Sales Pipeline")),
                ("/pipelines/0/stages/1/id", json!("decisionmakerboughtin")),
            ]),
        },
        Case {
            tool: "HUBSPOT_LIST_PIPELINE_STAGES",
            args: json!({"object_type": "deals", "pipeline_id": "default"}),
            method: "GET",
            url_contains: sts(vec!["/crm/v3/pipelines/deals/default/stages"]),
            body_eq: st(vec![]),
            response_status: 200,
            response: json!({"results": [
                {"id": "appointmentscheduled", "label": "Appointment Scheduled", "displayOrder": 0},
                {"id": "closedwon", "label": "Closed Won", "displayOrder": 4}]}),
            result_eq: st(vec![
                ("/count", json!(2)),
                ("/stages/1/label", json!("Closed Won")),
                ("/stages/1/displayOrder", json!(4)),
            ]),
        },
        Case {
            tool: "HUBSPOT_GET_PIPELINE_STAGE",
            args: json!({"object_type": "deals", "pipeline_id": "default", "stage_id": "closedwon"}),
            method: "GET",
            url_contains: sts(vec!["/crm/v3/pipelines/deals/default/stages/closedwon"]),
            body_eq: st(vec![]),
            response_status: 200,
            response: json!({"id": "closedwon", "label": "Closed Won", "displayOrder": 4,
                "metadata": {"probability": "1.0"}, "createdAt": "2020-01-01T00:00:00Z"}),
            result_eq: st(vec![("/label", json!("Closed Won")), ("/metadata/probability", json!("1.0"))]),
        },
        Case {
            tool: "HUBSPOT_LIST_ASSOCIATION_TYPES",
            args: json!({"from_object_type": "contact", "to_object_type": "company"}),
            method: "GET",
            url_contains: sts(vec!["/crm/v4/associations/contact/company/labels"]),
            body_eq: st(vec![]),
            response_status: 200,
            response: json!({"results": [
                {"category": "HUBSPOT_DEFINED", "typeId": 1, "label": "Primary"},
                {"category": "USER_DEFINED", "typeId": 6, "label": "Billing Contact"}]}),
            result_eq: st(vec![
                ("/count", json!(2)),
                ("/association_types/0/typeId", json!(1)),
                ("/association_types/1/category", json!("USER_DEFINED")),
            ]),
        },
        Case {
            tool: "HUBSPOT_FIND_ASSOCIATIONS",
            args: json!({"from_object_id": "101", "from_object_type": "contact",
                "to_object_type": "deal"}),
            method: "GET",
            url_contains: sts(vec!["/crm/v4/objects/contact/101/associations/deal", "limit=100"]),
            body_eq: st(vec![]),
            response_status: 200,
            response: json!({"results": [{"toObjectId": "301", "associationTypes": [
                {"typeId": 3, "category": "HUBSPOT_DEFINED"}]}],
                "paging": {"next": {"after": "a1"}}}),
            result_eq: st(vec![("/count", json!(1)), ("/associations/0/toObjectId", json!("301"))]),
        },
        Case {
            tool: "HUBSPOT_LIST_STATIC_LISTS",
            args: json!({"query": "Newsletter", "count": 10, "offset": 0}),
            method: "POST",
            url_contains: sts(vec!["/crm/v3/lists/search"]),
            body_eq: st(vec![("/query", json!("Newsletter")), ("/count", json!(10))]),
            response_status: 200,
            response: json!({"lists": [
                {"listId": "77", "name": "Newsletter DE", "updatedAt": "2026-02-01T10:00:00Z",
                 "processingType": "DYNAMIC"}],
                "total": 1, "offset": 0, "hasMore": false}),
            result_eq: st(vec![
                ("/total", json!(1)),
                ("/has_more", json!(false)),
                ("/lists/0/listId", json!("77")),
            ]),
        },
        // ---- writes -------------------------------------------------------
        Case {
            tool: "HUBSPOT_CREATE_CONTACT",
            args: json!({"properties": {"email": "new@example.com", "firstname": "New",
                "tags": ["a", "b"]}}),
            method: "POST",
            url_contains: sts(vec!["/crm/v3/objects/contacts"]),
            body_eq: st(vec![
                ("/properties/email", json!("new@example.com")),
                ("/properties/firstname", json!("New")),
                ("/properties/tags", json!("a;b")),
            ]),
            response_status: 201,
            response: json!({"id": "501", "createdAt": "2026-10-01T08:00:00Z",
                "updatedAt": "2026-10-01T08:00:00Z",
                "properties": {"email": "new@example.com", "tags": "a;b"}}),
            result_eq: st(vec![("/id", json!("501")), ("/properties/email", json!("new@example.com"))]),
        },
        Case {
            tool: "HUBSPOT_CREATE_COMPANY",
            args: json!({"properties": {"name": "New Co", "domain": "new.example"}}),
            method: "POST",
            url_contains: sts(vec!["/crm/v3/objects/companies"]),
            body_eq: st(vec![("/properties/name", json!("New Co"))]),
            response_status: 201,
            response: json!({"id": "601", "properties": {"name": "New Co", "domain": "new.example"}}),
            result_eq: st(vec![("/id", json!("601"))]),
        },
        Case {
            tool: "HUBSPOT_UPDATE_CONTACT",
            args: json!({"contact_id": "101", "properties": {"jobtitle": "CTO"}}),
            method: "PATCH",
            url_contains: sts(vec!["/crm/v3/objects/contacts/101"]),
            body_eq: st(vec![("/properties/jobtitle", json!("CTO"))]),
            response_status: 200,
            response: json!({"id": "101", "updatedAt": "2026-10-02T09:00:00Z",
                "properties": {"email": "ada@example.com", "jobtitle": "CTO"}}),
            result_eq: st(vec![("/id", json!("101")), ("/properties/jobtitle", json!("CTO"))]),
        },
        Case {
            tool: "HUBSPOT_UPSERT_CONTACT",
            args: json!({"email": "ada@example.com", "properties": {"firstname": "Ada L."}}),
            method: "POST",
            url_contains: sts(vec!["/crm/v3/objects/contacts/batch/upsert"]),
            body_eq: st(vec![
                ("/idProperty", json!("email")),
                ("/inputs/0/id", json!("ada@example.com")),
                ("/inputs/0/properties/email", json!("ada@example.com")),
                ("/inputs/0/properties/firstname", json!("Ada L.")),
            ]),
            response_status: 200,
            response: json!({"results": [{"id": "101", "properties": {
                "email": "ada@example.com", "firstname": "Ada L."}}],
                "requestedAt": "2026-10-02T10:00:00Z"}),
            result_eq: st(vec![("/id", json!("101")), ("/properties/firstname", json!("Ada L."))]),
        },
        Case {
            tool: "HUBSPOT_CREATE_DEAL",
            args: json!({"dealname": "Rollout Q1", "pipeline_id": "default",
                "pipeline_stage_id": "appointmentscheduled", "properties": {"amount": "5000"}}),
            method: "POST",
            url_contains: sts(vec!["/crm/v3/objects/deals"]),
            body_eq: st(vec![
                ("/properties/dealname", json!("Rollout Q1")),
                ("/properties/pipeline", json!("default")),
                ("/properties/dealstage", json!("appointmentscheduled")),
                ("/properties/amount", json!("5000")),
            ]),
            response_status: 201,
            response: json!({"id": "701", "properties": {"dealname": "Rollout Q1",
                "amount": "5000", "pipeline": "default", "dealstage": "appointmentscheduled"}}),
            result_eq: st(vec![("/id", json!("701")), ("/properties/dealstage", json!("appointmentscheduled"))]),
        },
        Case {
            tool: "HUBSPOT_UPDATE_DEAL",
            args: json!({"deal_id": "701", "pipeline_stage_id": "closedwon"}),
            method: "PATCH",
            url_contains: sts(vec!["/crm/v3/objects/deals/701"]),
            body_eq: st(vec![("/properties/dealstage", json!("closedwon"))]),
            response_status: 200,
            response: json!({"id": "701", "properties": {"dealname": "Rollout Q1",
                "dealstage": "closedwon", "amount": "5000"}}),
            result_eq: st(vec![("/properties/dealstage", json!("closedwon"))]),
        },
        Case {
            tool: "HUBSPOT_CREATE_TICKET",
            args: json!({"ticket_name": "Printer offline", "pipeline_id": "0",
                "pipeline_stage_id": "1"}),
            method: "POST",
            url_contains: sts(vec!["/crm/v3/objects/tickets"]),
            body_eq: st(vec![
                ("/properties/subject", json!("Printer offline")),
                ("/properties/hs_pipeline", json!("0")),
                ("/properties/hs_pipeline_stage", json!("1")),
            ]),
            response_status: 201,
            response: json!({"id": "801", "properties": {"subject": "Printer offline",
                "hs_ticket_id": "9002", "hs_pipeline": "0", "hs_pipeline_stage": "1"}}),
            result_eq: st(vec![("/id", json!("801")), ("/properties/hs_ticket_id", json!("9002"))]),
        },
        Case {
            tool: "HUBSPOT_CREATE_NOTE",
            args: json!({"note_body": "Called back, <b>interested</b>", "object_type": "deal",
                "object_id": "701", "timestamp": "2026-09-14T12:00:00Z"}),
            method: "POST",
            url_contains: sts(vec!["/crm/v3/objects/notes"]),
            body_eq: st(vec![
                ("/properties/hs_note_body", json!("Called back, <b>interested</b>")),
                ("/properties/hs_timestamp", json!("2026-09-14T12:00:00Z")),
                ("/associations/0/to/id", json!("701")),
                ("/associations/0/types/0/associationTypeId", json!(214)),
                ("/associations/0/types/0/associationCategory", json!("HUBSPOT_DEFINED")),
            ]),
            response_status: 201,
            response: json!({"id": "901", "properties": {"hs_note_body": "Called back",
                "hs_timestamp": "2026-09-14T12:00:00Z"},
                "associations": [{"to": {"id": "701", "type": "deal_to_contact"}}]}),
            result_eq: st(vec![("/id", json!("901"))]),
        },
        Case {
            tool: "HUBSPOT_CREATE_TASK",
            args: json!({"subject": "Send proposal", "object_type": "company", "object_id": "601",
                "due_date": "2026-09-20T09:00:00Z", "body": "Include pricing", "priority": "HIGH",
                "owner_id": "12"}),
            method: "POST",
            url_contains: sts(vec!["/crm/v3/objects/tasks"]),
            body_eq: st(vec![
                ("/properties/hs_task_subject", json!("Send proposal")),
                ("/properties/hs_timestamp", json!("2026-09-20T09:00:00Z")),
                ("/properties/hs_task_status", json!("NOT_STARTED")),
                ("/properties/hs_task_priority", json!("HIGH")),
                ("/properties/hs_task_body", json!("Include pricing")),
                ("/properties/hubspot_owner_id", json!("12")),
                ("/associations/0/types/0/associationTypeId", json!(192)),
            ]),
            response_status: 201,
            response: json!({"id": "902", "properties": {"hs_task_subject": "Send proposal",
                "hs_task_status": "NOT_STARTED", "hs_task_priority": "HIGH",
                "hs_timestamp": "2026-09-20T09:00:00Z"}}),
            result_eq: st(vec![("/id", json!("902")), ("/properties/hs_task_subject", json!("Send proposal"))]),
        },
        Case {
            tool: "HUBSPOT_ADD_CONTACT_TO_LIST",
            args: json!({"list_id": "77", "email": "ada@example.com"}),
            method: "POST",
            url_contains: sts(vec!["/contacts/v1/lists/77/add"]),
            body_eq: st(vec![("/emails/0", json!("ada@example.com"))]),
            response_status: 200,
            response: json!({"updated": ["101"], "ids": ["101"], "created": []}),
            result_eq: st(vec![("/updated/0", json!("101"))]),
        },
        Case {
            tool: "HUBSPOT_UNSUBSCRIBE_EMAIL",
            args: json!({"email": "ada@example.com"}),
            method: "PUT",
            url_contains: sts(vec!["/email/public/v1/subscriptions/ada%40example.com"]),
            body_eq: st(vec![("/unsubscribeFromAll", json!(true))]),
            response_status: 200,
            response: json!({"isUnsubscribed": true}),
            result_eq: st(vec![("/isUnsubscribed", json!(true))]),
        },
        Case {
            tool: "HUBSPOT_ADD_CONTACT_TO_WORKFLOW",
            args: json!({"workflow_id": "9001", "email": "ada@example.com"}),
            method: "POST",
            url_contains: sts(vec!["/automation/v2/workflows/9001/enrollments/contacts/ada%40example.com"]),
            body_eq: st(vec![]),
            response_status: 204,
            response: Value::Null,
            result_eq: st(vec![]),
        },
        Case {
            tool: "HUBSPOT_CREATE_ASSOCIATIONS",
            args: json!({"from_object_id": "101", "from_object_type": "contact",
                "to_object_type": "company", "association_type_id": "1",
                "to_object_ids": ["201", "202"]}),
            method: "POST",
            url_contains: sts(vec!["/crm/v4/associations/contact/company/batch/create"]),
            body_eq: st(vec![
                ("/inputs/0/_from/id", json!("101")),
                ("/inputs/0/to/id", json!("201")),
                ("/inputs/0/types/0/associationTypeId", json!("1")),
                ("/inputs/0/types/0/associationCategory", json!("HUBSPOT_DEFINED")),
                ("/inputs/1/to/id", json!("202")),
            ]),
            response_status: 201,
            response: json!({"results": [{"from": {"id": "101"}, "to": {"id": "201"}},
                {"from": {"id": "101"}, "to": {"id": "202"}}],
                "status": "COMPLETE"}),
            result_eq: st(vec![("/total_associations", json!(2)), ("/batch_count", json!(1)),
                ("/status", json!("COMPLETE"))]),
        },
    ]
}

#[test]
fn recorded_cases_match_the_typescript_actions() {
    let adapter = HubSpot::with_api_base("https://api.hubapi.com");
    let cases = cases();
    assert_full_coverage(&adapter, &cases);
    support::spec_parity("hubspot", &adapter, &cases);
    for c in &cases {
        run(&adapter, c);
    }
}

#[test]
fn mutating_tools_are_flagged() {
    let adapter = HubSpot::default();
    for t in &adapter.spec().tools {
        let should_write = matches!(
            t.slug.as_str(),
            "HUBSPOT_CREATE_CONTACT"
                | "HUBSPOT_CREATE_COMPANY"
                | "HUBSPOT_UPDATE_CONTACT"
                | "HUBSPOT_UPSERT_CONTACT"
                | "HUBSPOT_CREATE_DEAL"
                | "HUBSPOT_UPDATE_DEAL"
                | "HUBSPOT_CREATE_TICKET"
                | "HUBSPOT_CREATE_NOTE"
                | "HUBSPOT_CREATE_TASK"
                | "HUBSPOT_ADD_CONTACT_TO_LIST"
                | "HUBSPOT_UNSUBSCRIBE_EMAIL"
                | "HUBSPOT_ADD_CONTACT_TO_WORKFLOW"
                | "HUBSPOT_CREATE_ASSOCIATIONS"
        );
        assert_eq!(t.mutating, should_write, "{}", t.slug);
        assert!(!t.scopes.is_empty(), "{} has no scopes", t.slug);
        assert!(!t.title.de.is_empty() && !t.title.en.is_empty(), "{}", t.slug);
    }
    // Every scope used by a tool is part of the OAuth2 default scope list.
    let spec = adapter.spec();
    let oauth = spec
        .auth_schemes
        .iter()
        .find_map(|a| match a {
            ctox_toolhub::model::AuthScheme::OAuth2(o) => Some(o),
            _ => None,
        })
        .expect("oauth2");
    let defaults: Vec<&str> = oauth.default_scopes.iter().map(String::as_str).collect();
    for t in &spec.tools {
        for sc in &t.scopes {
            assert!(defaults.contains(&sc.as_str()), "{} needs unknown scope {sc}", t.slug);
        }
    }
}

/// Live read-only calls against the real API:
/// `CTOX_TOOLHUB_LIVE_HUBSPOT_TOKEN=… cargo test --test port_hubspot -- --ignored`.
#[test]
#[ignore]
fn live_read_only_calls() {
    let adapter = HubSpot::default();
    let probes: [(&str, Value); 3] = [
        ("HUBSPOT_LIST_OWNERS", json!({"limit": 3})),
        ("HUBSPOT_LIST_PIPELINES", json!({"object_type": "deals"})),
        (
            "HUBSPOT_SEARCH_OBJECTS",
            json!({"object_type": "companies", "limit": 3, "properties": ["name", "domain"]}),
        ),
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