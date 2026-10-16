//! Salesforce port: one recorded case per action. The expectations are taken from the
//! Activepieces sources — `src/lib/common/index.ts` (`querySalesforceApi`,
//! `callSalesforceApi`, the dropdown queries), `src/lib/common/report.ts` and the
//! `run()` bodies in `src/lib/action/*.ts`:
//!
//! * SOQL calls are `GET {instance}/services/data/v56.0/query?q=…`
//! * object/record writes are `POST|PATCH|DELETE {instance}/services/data/v56.0/sobjects/…`
//! * child records are `GET {instance}/services/data/v56.0/ui-api/records/{id}/child-relationships/{rel}`
//! * reports are `POST {instance}/services/data/v56.0/analytics/reports/{id}?includeDetails=true`
//! * the composite upsert is `PATCH {instance}/services/data/v55.0/composite/sobjects/{o}/{field}`
mod support;

use ctox_toolhub::toolkits::salesforce::Salesforce;
use ctox_toolhub::toolkits::ToolkitAdapter as _;
use serde_json::{json, Value};
use support::{assert_full_coverage, live, run, Case};

/// Instance URL stand-in for the recorded cases; real orgs use their My Domain
/// (`instance_url` of the OAuth token response).
const BASE: &str = "https://acme.my.salesforce.test";

const ACCOUNT: &str = "001300000000001AAA";
const CONTACT: &str = "003300000000001AAA";
const LEAD: &str = "00Q300000000001AAA";
const CASE: &str = "500300000000001AAA";
const OPPORTUNITY: &str = "006300000000001AAA";
const TASK: &str = "00T300000000001AAA";
const NOTE: &str = "02N300000000001AAA";
const ATTACHMENT: &str = "00P300000000001AAA";
const DOCUMENT: &str = "069300000000001AAA";
const CAMPAIGN: &str = "701300000000001AAA";
const REPORT: &str = "00O300000000001AAA";
const USER: &str = "005300000000001AAA";

/// Own a `String` as a `&'static str` so a case can carry a formatted URL part.
fn leak(s: String) -> &'static str {
    Box::leak(s.into_boxed_str())
}

fn adapter() -> Salesforce {
    Salesforce::with_api_base(BASE)
}

/// `&[(ptr, value)]` for a `Case` field: leaked so it lives as long as the case.
fn ptrs<const N: usize>(pairs: [(&'static str, Value); N]) -> &'static [(&'static str, Value)] {
    Box::leak(Box::new(pairs))
}

fn sts<const N: usize>(parts: [&'static str; N]) -> &'static [&'static str] {
    Box::leak(Box::new(parts))
}

/// Salesforce query envelope with the `attributes` node the API adds to each record.
fn query_result(records: Value, done: bool, next: Option<&str>) -> Value {
    json!({
        "totalSize": records.as_array().map(Vec::len).unwrap_or(0),
        "done": done,
        "nextRecordsUrl": next,
        "records": records,
    })
}

fn account() -> Value {
    json!({"attributes": {"type": "Account", "url": format!("/services/data/v56.0/sobjects/Account/{ACCOUNT}")},
        "Id": ACCOUNT, "Name": "Acme GmbH", "Industry": "Technology"})
}

fn contact() -> Value {
    json!({"attributes": {"type": "Contact", "url": format!("/services/data/v56.0/sobjects/Contact/{CONTACT}")},
        "Id": CONTACT, "FirstName": "Ada", "LastName": "Lovelace", "Email": "ada@acme.example"})
}

fn cases() -> Vec<Case<'static>> {
    vec![
        // ---------------------------------------------------------- SOQL reads
        Case {
            tool: "SALESFORCE_RUN_SOQL_QUERY",
            args: json!({"query": "SELECT Id, Name FROM Account ORDER BY Name LIMIT 100"}),
            method: "GET",
            url_contains: sts([
                leak(format!("{BASE}/services/data/v56.0/query?q=")),
                "SELECT+Id%2C+Name+FROM+Account+ORDER+BY+Name+LIMIT+100",
            ]),
            body_eq: ptrs([]),
            response_status: 200,
            response: query_result(json!([account()]), true, None),
            result_eq: ptrs([
                ("/count", json!(1)),
                ("/records/0/Name", json!("Acme GmbH")),
                ("/done", json!(true)),
            ]),
        },
        Case {
            tool: "SALESFORCE_FIND_RECORDS_BY_QUERY",
            args: json!({"object": "Account", "where_clause": "Industry = 'Technology' AND Name LIKE 'A%'"}),
            method: "GET",
            url_contains: sts([
                leak(format!("{BASE}/services/data/v56.0/query?q=")),
                "SELECT+FIELDS%28ALL%29+FROM+Account+WHERE+Industry+%3D+%27Technology%27+AND+Name+LIKE+%27A%25%27+LIMIT+200",
            ]),
            body_eq: ptrs([]),
            response_status: 200,
            response: query_result(json!([account()]), false, Some("/services/data/v56.0/query/01g0000000abcde-2000")),
            result_eq: ptrs([
                ("/total_size", json!(1)),
                ("/done", json!(false)),
                ("/next_records_url", json!("/services/data/v56.0/query/01g0000000abcde-2000")),
            ]),
        },
        Case {
            tool: "SALESFORCE_FIND_RECORD",
            args: json!({"object": "Contact", "field": "Email", "search_value": "o'brien@acme.example"}),
            method: "GET",
            url_contains: sts([
                leak(format!("{BASE}/services/data/v56.0/query?q=")),
                "WHERE+Email+%3D+%27o%5C%27brien%40acme.example%27+LIMIT+200",
            ]),
            body_eq: ptrs([]),
            response_status: 200,
            response: query_result(json!([contact()]), true, None),
            result_eq: ptrs([
                ("/count", json!(1)),
                ("/records/0/LastName", json!("Lovelace")),
            ]),
        },
        Case {
            tool: "SALESFORCE_FIND_CHILD_RECORDS",
            args: json!({"parent_id": ACCOUNT, "child_relationship": "Contacts"}),
            method: "GET",
            url_contains: sts([leak(format!(
                "{BASE}/services/data/v56.0/ui-api/records/{ACCOUNT}/child-relationships/Contacts"
            ))]),
            body_eq: ptrs([]),
            response_status: 200,
            response: json!({"records": [
                {"attributes": {"type": "Contact", "url": format!("/services/data/v56.0/sobjects/Contact/{CONTACT}")},
                 "Id": CONTACT, "Name": "Ada Lovelace", "fields": {"Name": {"value": "Ada Lovelace"}}}]}),
            result_eq: ptrs([
                ("/count", json!(1)),
                ("/records/0/Id", json!(CONTACT)),
            ]),
        },
        Case {
            tool: "SALESFORCE_RUN_REPORT",
            args: json!({"report_id": REPORT,
                "filters": [{"column": "ACCOUNT.NAME", "operator": "equals", "value": "Acme GmbH"}]}),
            method: "POST",
            url_contains: sts([leak(format!("{BASE}/services/data/v56.0/analytics/reports/{REPORT}?includeDetails=true"))]),
            body_eq: ptrs([
                ("/reportMetadata/reportFilters/0/column", json!("ACCOUNT.NAME")),
                ("/reportMetadata/reportFilters/0/operator", json!("equals")),
            ]),
            response_status: 200,
            response: json!({
                "attributes": {"reportId": REPORT, "reportName": "Open Pipeline", "status": "Success"},
                "reportMetadata": {"name": "Open Pipeline", "detailColumns": ["AMOUNT", "STAGE"],
                    "groupingsDown": [{"name": "ACCOUNT_NAME"}, {"name": "REGION_NAME"}]},
                "reportExtendedMetadata": {
                    "detailColumnInfo": {"AMOUNT": {"label": "Amount", "dataType": "int"},
                                         "STAGE": {"label": "Stage", "dataType": "picklist"}},
                    "groupingColumnInfo": {"ACCOUNT_NAME": {"label": "Account"},
                                           "REGION_NAME": {"label": "Region"}}},
                "groupingsDown": {"groupings": [{"key": "0", "label": "Acme GmbH",
                    "groupings": [{"key": "0_1", "label": "EMEA"}]}]},
                "factMap": {"0_1!T": {"rows": [{"dataCells": [{"label": "120000", "value": 120000},
                                                             {"label": "Prospecting", "value": "Prospecting"}]}]}},
            }),
            result_eq: ptrs([
                ("/report_name", json!("Open Pipeline")),
                ("/report_id", json!(REPORT)),
                ("/total_rows", json!(1)),
                ("/rows/0/Account", json!("Acme GmbH")),
                ("/rows/0/Region", json!("EMEA")),
                ("/rows/0/Amount", json!("120000")),
            ]),
        },
        Case {
            tool: "SALESFORCE_LIST_CLASSIC_ATTACHMENTS",
            args: json!({"record_id": ACCOUNT}),
            method: "GET",
            url_contains: sts([
                leak(format!("{BASE}/services/data/v56.0/query?q=")),
                "SELECT+Id%2C+Name%2C+BodyLength%2C+ContentType+FROM+Attachment+WHERE+ParentId+%3D+%27001300000000001AAA%27",
            ]),
            body_eq: ptrs([]),
            response_status: 200,
            response: query_result(json!([
                {"attributes": {"type": "Attachment"}, "Id": ATTACHMENT,
                 "Name": "contract.pdf", "BodyLength": 20480, "ContentType": "application/pdf"}]), true, None),
            result_eq: ptrs([
                ("/count", json!(1)),
                ("/records/0/Name", json!("contract.pdf")),
                ("/records/0/ContentType", json!("application/pdf")),
            ]),
        },
        Case {
            tool: "SALESFORCE_LIST_RECORD_FILES",
            args: json!({"record_id": ACCOUNT}),
            method: "GET",
            url_contains: sts([
                leak(format!("{BASE}/services/data/v56.0/query?q=")),
                "SELECT+ContentDocument.Id%2C+ContentDocument.Title+FROM+ContentDocumentLink+WHERE+LinkedEntityId+%3D+%27001300000000001AAA%27",
            ]),
            body_eq: ptrs([]),
            response_status: 200,
            response: query_result(json!([
                {"attributes": {"type": "ContentDocumentLink"},
                 "ContentDocument": {"attributes": {"type": "ContentDocument"}, "Id": DOCUMENT, "Title": "logo.png"}}]), true, None),
            result_eq: ptrs([
                ("/count", json!(1)),
                ("/files/0/Id", json!(DOCUMENT)),
                ("/files/0/Title", json!("logo.png")),
            ]),
        },
        // ------------------------------------------------------- list helpers
        Case {
            tool: "SALESFORCE_LIST_OBJECTS",
            args: json!({}),
            method: "GET",
            url_contains: sts([leak(format!("{BASE}/services/data/v56.0/sobjects"))]),
            body_eq: ptrs([]),
            response_status: 200,
            response: json!({"encoding": "UTF-8", "sobjects": [
                {"name": "Account", "label": "Account", "custom": false, "queryable": true},
                {"name": "Contact", "label": "Contact", "custom": false, "queryable": true},
                {"name": "My_Object__c", "label": "My Object", "custom": true, "queryable": true}]}),
            result_eq: ptrs([
                ("/count", json!(3)),
                ("/sobjects/2/name", json!("My_Object__c")),
            ]),
        },
        Case {
            tool: "SALESFORCE_DESCRIBE_OBJECT",
            args: json!({"object": "Opportunity"}),
            method: "GET",
            url_contains: sts([leak(format!("{BASE}/services/data/v56.0/sobjects/Opportunity/describe"))]),
            body_eq: ptrs([]),
            response_status: 200,
            response: json!({"name": "Opportunity", "label": "Opportunity", "fields": [
                {"name": "Name", "label": "Opportunity Name", "type": "string", "picklistValues": []},
                {"name": "StageName", "label": "Stage", "type": "picklist", "picklistValues": [
                    {"value": "Prospecting", "label": "Prospecting", "active": true},
                    {"value": "Closed Won", "label": "Closed Won", "active": true}]}],
                "childRelationships": [{"relationshipName": "Contacts", "childSObject": "Contact"}]}),
            result_eq: ptrs([
                ("/name", json!("Opportunity")),
                ("/fields/1/picklistValues/1/value", json!("Closed Won")),
                ("/child_relationships/0/relationshipName", json!("Contacts")),
            ]),
        },
        Case {
            tool: "SALESFORCE_LIST_ACCOUNTS",
            args: json!({}),
            method: "GET",
            url_contains: sts([
                leak(format!("{BASE}/services/data/v56.0/query?q=")),
                "SELECT+Id%2C+Name+FROM+Account+ORDER+BY+Name+LIMIT+100",
            ]),
            body_eq: ptrs([]),
            response_status: 200,
            response: query_result(json!([account()]), true, None),
            result_eq: ptrs([
                ("/count", json!(1)),
                ("/records/0/Id", json!(ACCOUNT)),
            ]),
        },
        Case {
            tool: "SALESFORCE_LIST_CONTACTS",
            args: json!({}),
            method: "GET",
            url_contains: sts([
                leak(format!("{BASE}/services/data/v56.0/query?q=")),
                "SELECT+Id%2C+Name%2C+Email+FROM+Contact+ORDER+BY+Name+LIMIT+200",
            ]),
            body_eq: ptrs([]),
            response_status: 200,
            response: query_result(json!([contact()]), true, None),
            result_eq: ptrs([
                ("/count", json!(1)),
                ("/records/0/Email", json!("ada@acme.example")),
            ]),
        },
        Case {
            tool: "SALESFORCE_LIST_USERS",
            args: json!({}),
            method: "GET",
            url_contains: sts([
                leak(format!("{BASE}/services/data/v56.0/query?q=")),
                "SELECT+Id%2C+Name+FROM+User+WHERE+IsActive+%3D+true+ORDER+BY+Name",
            ]),
            body_eq: ptrs([]),
            response_status: 200,
            response: query_result(
                json!([{"attributes": {"type": "User"}, "Id": USER, "Name": "Ada Lovelace"}]),
                true,
                None,
            ),
            result_eq: ptrs([
                ("/count", json!(1)),
                ("/records/0/Name", json!("Ada Lovelace")),
            ]),
        },
        Case {
            tool: "SALESFORCE_LIST_REPORTS",
            args: json!({"name_prefix": "Open"}),
            method: "GET",
            url_contains: sts([
                leak(format!("{BASE}/services/data/v56.0/query?q=")),
                "SELECT+Id%2C+Name+FROM+Report+WHERE+Name+LIKE+%27Open%25%27+ORDER+BY+Name",
            ]),
            body_eq: ptrs([]),
            response_status: 200,
            response: query_result(
                json!([{"attributes": {"type": "Report"}, "Id": REPORT, "Name": "Open Pipeline"}]),
                true,
                None,
            ),
            result_eq: ptrs([
                ("/count", json!(1)),
                ("/records/0/Name", json!("Open Pipeline")),
            ]),
        },
        // -------------------------------------------------------------- writes
        Case {
            tool: "SALESFORCE_CREATE_CONTACT",
            args: json!({"last_name": "Lovelace", "first_name": "Ada", "email": "ada@acme.example",
                "account_id": ACCOUNT, "other_fields": {"Title": "CTO", "Department": ""}}),
            method: "POST",
            url_contains: sts([leak(format!("{BASE}/services/data/v56.0/sobjects/Contact"))]),
            body_eq: ptrs([
                ("/LastName", json!("Lovelace")),
                ("/FirstName", json!("Ada")),
                ("/Email", json!("ada@acme.example")),
                ("/AccountId", json!(ACCOUNT)),
                ("/Title", json!("CTO")),
            ]),
            response_status: 201,
            response: json!({"id": CONTACT, "success": true, "errors": []}),
            result_eq: ptrs([
                ("/id", json!(CONTACT)),
                ("/success", json!(true)),
            ]),
        },
        Case {
            tool: "SALESFORCE_CREATE_LEAD",
            args: json!({"last_name": "Hopper", "company": "Acme GmbH", "first_name": "Grace",
                "email": "grace@acme.example", "lead_source": "Web"}),
            method: "POST",
            url_contains: sts([leak(format!("{BASE}/services/data/v56.0/sobjects/Lead"))]),
            body_eq: ptrs([
                ("/LastName", json!("Hopper")),
                ("/Company", json!("Acme GmbH")),
                ("/LeadSource", json!("Web")),
            ]),
            response_status: 201,
            response: json!({"id": LEAD, "success": true, "errors": []}),
            result_eq: ptrs([
                ("/id", json!(LEAD)),
                ("/success", json!(true)),
            ]),
        },
        Case {
            tool: "SALESFORCE_CREATE_CASE",
            args: json!({"subject": "Login times out", "description": "Safari spinner forever.",
                "status": "New", "priority": "High", "origin": "Web", "account_id": ACCOUNT,
                "contact_id": CONTACT}),
            method: "POST",
            url_contains: sts([leak(format!("{BASE}/services/data/v56.0/sobjects/Case"))]),
            body_eq: ptrs([
                ("/Subject", json!("Login times out")),
                ("/Status", json!("New")),
                ("/Priority", json!("High")),
                ("/Origin", json!("Web")),
                ("/ContactId", json!(CONTACT)),
            ]),
            response_status: 201,
            response: json!({"id": CASE, "success": true, "errors": []}),
            result_eq: ptrs([("/id", json!(CASE))]),
        },
        Case {
            tool: "SALESFORCE_CREATE_OPPORTUNITY",
            args: json!({"name": "Acme renewal", "close_date": "2026-12-31", "stage_name": "Prospecting",
                "account_id": ACCOUNT, "amount": 120000}),
            method: "POST",
            url_contains: sts([leak(format!("{BASE}/services/data/v56.0/sobjects/Opportunity"))]),
            body_eq: ptrs([
                ("/Name", json!("Acme renewal")),
                ("/CloseDate", json!("2026-12-31")),
                ("/StageName", json!("Prospecting")),
                ("/Amount", json!(120000)),
            ]),
            response_status: 201,
            response: json!({"id": OPPORTUNITY, "success": true, "errors": []}),
            result_eq: ptrs([
                ("/id", json!(OPPORTUNITY)),
                ("/success", json!(true)),
            ]),
        },
        Case {
            tool: "SALESFORCE_CREATE_TASK",
            args: json!({"subject": "Call Acme", "owner_id": USER, "status": "Not Started",
                "priority": "Normal", "who_id": CONTACT, "what_id": OPPORTUNITY}),
            method: "POST",
            url_contains: sts([leak(format!("{BASE}/services/data/v56.0/sobjects/Task"))]),
            body_eq: ptrs([
                ("/Subject", json!("Call Acme")),
                ("/OwnerId", json!(USER)),
                ("/WhoId", json!(CONTACT)),
                ("/WhatId", json!(OPPORTUNITY)),
            ]),
            response_status: 201,
            response: json!({"id": TASK, "success": true, "errors": []}),
            result_eq: ptrs([("/id", json!(TASK))]),
        },
        Case {
            tool: "SALESFORCE_CREATE_NOTE",
            args: json!({"parent_id": ACCOUNT, "title": "Kickoff", "body": "Met on 2026-10-01."}),
            method: "POST",
            url_contains: sts([leak(format!("{BASE}/services/data/v56.0/sobjects/Note"))]),
            body_eq: ptrs([
                ("/ParentId", json!(ACCOUNT)),
                ("/Title", json!("Kickoff")),
                ("/Body", json!("Met on 2026-10-01.")),
            ]),
            response_status: 201,
            response: json!({"id": NOTE, "success": true, "errors": []}),
            result_eq: ptrs([
                ("/id", json!(NOTE)),
                ("/success", json!(true)),
            ]),
        },
        Case {
            tool: "SALESFORCE_ADD_CONTACT_TO_CAMPAIGN",
            args: json!({"campaign_id": CAMPAIGN, "contact_id": CONTACT, "status": "Sent"}),
            method: "POST",
            url_contains: sts([leak(format!("{BASE}/services/data/v56.0/sobjects/CampaignMember"))]),
            body_eq: ptrs([
                ("/CampaignId", json!(CAMPAIGN)),
                ("/ContactId", json!(CONTACT)),
                ("/Status", json!("Sent")),
            ]),
            response_status: 201,
            response: json!({"id": "00v300000000001AAA", "success": true, "errors": []}),
            result_eq: ptrs([("/success", json!(true))]),
        },
        Case {
            tool: "SALESFORCE_ADD_LEAD_TO_CAMPAIGN",
            args: json!({"campaign_id": CAMPAIGN, "lead_id": LEAD, "status": "Responded"}),
            method: "POST",
            url_contains: sts([leak(format!("{BASE}/services/data/v56.0/sobjects/CampaignMember"))]),
            body_eq: ptrs([
                ("/CampaignId", json!(CAMPAIGN)),
                ("/LeadId", json!(LEAD)),
                ("/Status", json!("Responded")),
            ]),
            response_status: 201,
            response: json!({"id": "00v300000000002AAA", "success": true, "errors": []}),
            result_eq: ptrs([("/id", json!("00v300000000002AAA"))]),
        },
        Case {
            tool: "SALESFORCE_SEND_EMAIL",
            args: json!({"recipient_id": CONTACT, "subject": "Your invoice", "body": "<p>Thanks!</p>",
                "related_to_id": OPPORTUNITY}),
            method: "POST",
            url_contains: sts([leak(format!("{BASE}/services/data/v56.0/sobjects/EmailMessage"))]),
            body_eq: ptrs([
                ("/ToIds/0", json!(CONTACT)),
                ("/Subject", json!("Your invoice")),
                ("/HtmlBody", json!("<p>Thanks!</p>")),
                ("/Status", json!("3")),
                ("/RelatedToId", json!(OPPORTUNITY)),
            ]),
            response_status: 201,
            response: json!({"id": "02s300000000001AAA", "success": true, "errors": []}),
            result_eq: ptrs([
                ("/id", json!("02s300000000001AAA")),
                ("/success", json!(true)),
            ]),
        },
        Case {
            tool: "SALESFORCE_CREATE_RECORD",
            args: json!({"object": "Account", "fields": {"Name": "Acme GmbH", "Industry": "Technology"}}),
            method: "POST",
            url_contains: sts([leak(format!("{BASE}/services/data/v56.0/sobjects/Account"))]),
            body_eq: ptrs([
                ("/Name", json!("Acme GmbH")),
                ("/Industry", json!("Technology")),
            ]),
            response_status: 201,
            response: json!({"id": ACCOUNT, "success": true, "errors": []}),
            result_eq: ptrs([("/id", json!(ACCOUNT))]),
        },
        Case {
            tool: "SALESFORCE_UPDATE_RECORD",
            args: json!({"object": "Account", "record_id": ACCOUNT, "fields": {"Industry": "Software"}}),
            method: "PATCH",
            url_contains: sts([leak(format!(
                "{BASE}/services/data/v56.0/sobjects/Account/{ACCOUNT}"
            ))]),
            body_eq: ptrs([("/Industry", json!("Software"))]),
            response_status: 204,
            response: json!({}),
            result_eq: ptrs([]),
        },
        Case {
            tool: "SALESFORCE_UPDATE_CONTACT",
            args: json!({"contact_id": CONTACT, "title": "CTO", "other_fields": {"Department": "Engineering"}}),
            method: "PATCH",
            url_contains: sts([leak(format!(
                "{BASE}/services/data/v56.0/sobjects/Contact/{CONTACT}"
            ))]),
            body_eq: ptrs([
                ("/Title", json!("CTO")),
                ("/Department", json!("Engineering")),
            ]),
            response_status: 204,
            response: json!({}),
            result_eq: ptrs([]),
        },
        Case {
            tool: "SALESFORCE_UPDATE_LEAD",
            args: json!({"lead_id": LEAD, "company": "Acme AG", "lead_source": "Web"}),
            method: "PATCH",
            url_contains: sts([leak(format!("{BASE}/services/data/v56.0/sobjects/Lead/{LEAD}"))]),
            body_eq: ptrs([
                ("/Company", json!("Acme AG")),
                ("/LeadSource", json!("Web")),
            ]),
            response_status: 204,
            response: json!({}),
            result_eq: ptrs([]),
        },
        Case {
            tool: "SALESFORCE_DELETE_RECORD",
            args: json!({"object": "Contact", "record_id": CONTACT}),
            method: "DELETE",
            url_contains: sts([leak(format!(
                "{BASE}/services/data/v56.0/sobjects/Contact/{CONTACT}"
            ))]),
            body_eq: ptrs([]),
            response_status: 204,
            response: json!({}),
            result_eq: ptrs([("/success", json!(true))]),
        },
        Case {
            tool: "SALESFORCE_DELETE_OPPORTUNITY",
            args: json!({"opportunity_id": OPPORTUNITY}),
            method: "DELETE",
            url_contains: sts([leak(format!(
                "{BASE}/services/data/v56.0/sobjects/Opportunity/{OPPORTUNITY}"
            ))]),
            body_eq: ptrs([]),
            response_status: 204,
            response: json!({}),
            result_eq: ptrs([("/success", json!(true))]),
        },
        Case {
            tool: "SALESFORCE_UPSERT_BY_EXTERNAL_ID",
            args: json!({"object": "Account", "external_field": "External_Id__c",
                "records": [{"External_Id__c": "A-1", "Name": "Acme GmbH"},
                            {"External_Id__c": "A-2", "Name": "Acme AG"}]}),
            method: "PATCH",
            url_contains: sts([leak(format!(
                "{BASE}/services/data/v55.0/composite/sobjects/Account/External_Id__c"
            ))]),
            body_eq: ptrs([
                ("/allOrNone", json!(false)),
                ("/records/0/External_Id__c", json!("A-1")),
                ("/records/1/Name", json!("Acme AG")),
            ]),
            response_status: 200,
            response: json!([{"id": ACCOUNT, "success": true, "created": true, "errors": []},
                             {"id": "001300000000002AAA", "success": true, "created": false, "errors": []}]),
            result_eq: ptrs([
                ("/0/id", json!(ACCOUNT)),
                ("/0/created", json!(true)),
                ("/1/created", json!(false)),
            ]),
        },
    ]
}

#[test]
fn recorded_cases_match_the_typescript_actions() {
    let adapter = adapter();
    let cases = cases();
    assert_full_coverage(&adapter, &cases);
    for c in &cases {
        run(&adapter, c);
    }
}

#[test]
fn mutating_tools_are_flagged() {
    let adapter = Salesforce::default();
    for t in &adapter.spec().tools {
        let should_write = matches!(
            t.slug.as_str(),
            "SALESFORCE_CREATE_CONTACT"
                | "SALESFORCE_CREATE_LEAD"
                | "SALESFORCE_CREATE_CASE"
                | "SALESFORCE_CREATE_OPPORTUNITY"
                | "SALESFORCE_CREATE_TASK"
                | "SALESFORCE_CREATE_NOTE"
                | "SALESFORCE_CREATE_RECORD"
                | "SALESFORCE_UPDATE_RECORD"
                | "SALESFORCE_UPDATE_CONTACT"
                | "SALESFORCE_UPDATE_LEAD"
                | "SALESFORCE_DELETE_RECORD"
                | "SALESFORCE_DELETE_OPPORTUNITY"
                | "SALESFORCE_ADD_CONTACT_TO_CAMPAIGN"
                | "SALESFORCE_ADD_LEAD_TO_CAMPAIGN"
                | "SALESFORCE_SEND_EMAIL"
                | "SALESFORCE_UPSERT_BY_EXTERNAL_ID"
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

/// Live read-only calls against the real API. The REST API lives on the org's
/// instance URL, so both env vars are needed:
/// `CTOX_TOOLHUB_LIVE_SALESFORCE_TOKEN=… CTOX_TOOLHUB_LIVE_SALESFORCE_INSTANCE_URL=https://acme.my.salesforce.com
///  cargo test --test port_salesforce -- --ignored`
#[test]
#[ignore]
fn live_read_only_calls() {
    let adapter = Salesforce::default();
    let instance = std::env::var("CTOX_TOOLHUB_LIVE_SALESFORCE_INSTANCE_URL").ok();
    let with_instance = |mut args: Value| -> Value {
        if let (Value::Object(m), Some(url)) = (&mut args, &instance) {
            m.insert("instance_url".into(), json!(url));
        }
        args
    };
    let probes: [(&str, Value); 3] = [
        ("SALESFORCE_LIST_ACCOUNTS", with_instance(json!({}))),
        ("SALESFORCE_LIST_USERS", with_instance(json!({}))),
        (
            "SALESFORCE_RUN_SOQL_QUERY",
            with_instance(json!({"query": "SELECT Id, Name FROM Contact LIMIT 3"})),
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
