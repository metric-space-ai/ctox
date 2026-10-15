//! Airtable port: one recorded case per action. Every expected method + URL comes
//! from the Activepieces piece — the record endpoints from the `*_ai` actions that
//! build their own `httpClient.sendRequest` URL, the legacy ones from the shared
//! helpers in `src/lib/common/index.ts`.
mod support;

use ctox_toolhub::toolkits::airtable::Airtable;
use ctox_toolhub::toolkits::ToolkitAdapter;
use serde_json::{json, Value};
use support::{live, run, Case};

const ENDPOINT: &str = "https://api.airtable.test/v0";
const BASE: &str = "appAcmeProject01";
const TABLE: &str = "tblTasks0001";
const RECORD: &str = "recAbc123XYZ";
const FIELD: &str = "fldStatus001";

fn adapter() -> Airtable {
    Airtable::with_api_base(ENDPOINT)
}

/// `&[(ptr, value)]` for a `Case` field: leaked so it lives as long as the case.
/// Leaked `&'static str` so a formatted URL fragment can live inside a `Case`.
fn frag(s: impl AsRef<str>) -> &'static str {
    Box::leak(s.as_ref().to_string().into_boxed_str())
}

/// Leaked slice for a `Case`'s `url_contains` field, so a formatted URL fragment
/// can live inside a `Case` literal.
fn urls(parts: &[&'static str]) -> &'static [&'static str] {
    Box::leak(parts.to_vec().into_boxed_slice())
}

fn ptrs<const N: usize>(pairs: [(&'static str, Value); N]) -> &'static [(&'static str, Value)] {
    Box::leak(Box::new(pairs))
}

/// A record as Airtable returns it from `GET /v0/{base}/{table}`.
fn raw_record(id: &str, name: &str) -> Value {
    json!({
        "id": id,
        "createdTime": "2026-01-14T09:12:33.000Z",
        "fields": {
            "Name": name,
            "Status": "Todo",
            "Due": "2026-02-01",
            "Done": false,
            "Tags": ["a", "b"],
            "Estimate": 3,
        }
    })
}

fn table_schema() -> Value {
    json!({
        "id": TABLE,
        "name": "Tasks",
        "description": "Everything we promised",
        "primaryFieldId": "fldName00001",
        "fields": [
            {"id": "fldName00001", "name": "Name", "type": "singleLineText", "description": "Task name"},
            {"id": FIELD, "name": "Status", "type": "singleSelect", "description": null,
             "options": {"choices": [
                 {"id": "selTodo", "name": "Todo", "color": "redLight2"},
                 {"id": "selDone", "name": "Done", "color": "greenLight2"}]}}
        ],
        "views": [
            {"id": "viwGrid00001", "name": "Grid view", "type": "grid"},
            {"id": "viwKanban001", "name": "Kanban", "type": "kanban"}
        ]
    })
}

#[test]
fn airtable_actions_are_ported() {
    let a = adapter();
    let cases: Vec<Case> = vec![
        // src/lib/actions/get-current-user.ts -> GET https://api.airtable.com/v0/meta/whoami
        Case {
            tool: "AIRTABLE_GET_CURRENT_USER",
            args: json!({}),
            method: "GET",
            url_contains: urls(&[ENDPOINT, "/meta/whoami"]),
            body_eq: ptrs([]),
            response_status: 200,
            response: json!({
                "id": "usrAdaLovelace01",
                "email": "ada@acme.com",
                "scopes": ["data.records:read", "data.records:write", "schema.bases:read", "schema.bases:write"]}),
            result_eq: ptrs([
                ("/id", json!("usrAdaLovelace01")),
                ("/email", json!("ada@acme.com")),
                ("/scopes/1", json!("data.records:write")),
            ]),
        },
        // src/lib/actions/list-bases.ts -> fetchAllBases: GET /v0/meta/bases
        Case {
            tool: "AIRTABLE_LIST_BASES",
            args: json!({}),
            method: "GET",
            url_contains: urls(&[ENDPOINT, "/meta/bases"]),
            body_eq: ptrs([]),
            response_status: 200,
            response: json!({
                "bases": [
                    {"id": BASE, "name": "Acme Projects", "permissionLevel": "create", "workspaceId": "wspAcme000001"},
                    {"id": "appPeople00002", "name": "People", "permissionLevel": "read"}
                ],
                "offset": "appNextPage001"}),
            result_eq: ptrs([
                ("/count", json!(2)),
                ("/bases/0/id", json!(BASE)),
                ("/bases/0/permission_level", json!("create")),
                ("/bases/0/workspace_id", json!("wspAcme000001")),
                ("/bases/1/name", json!("People")),
                ("/offset", json!("appNextPage001")),
            ]),
        },
        // src/lib/common/index.ts fetchBase: GET /v0/meta/bases/{baseId}
        Case {
            tool: "AIRTABLE_GET_BASE",
            args: json!({"base_id": BASE}),
            method: "GET",
            url_contains: urls(&[ENDPOINT, frag(format!("/meta/bases/{BASE}"))]),
            body_eq: ptrs([]),
            response_status: 200,
            response: json!({
                "id": BASE, "name": "Acme Projects", "permissionLevel": "create", "workspaceId": "wspAcme000001"}),
            result_eq: ptrs([
                ("/id", json!(BASE)),
                ("/name", json!("Acme Projects")),
                ("/permission_level", json!("create")),
                ("/workspace_id", json!("wspAcme000001")),
            ]),
        },
        // src/lib/actions/get-base-schema.ts -> fetchTableList: GET /v0/meta/bases/{baseId}/tables
        Case {
            tool: "AIRTABLE_GET_BASE_SCHEMA",
            args: json!({"base_id": BASE}),
            method: "GET",
            url_contains: urls(&[ENDPOINT, frag(format!("/meta/bases/{BASE}/tables"))]),
            body_eq: ptrs([]),
            response_status: 200,
            response: json!({"tables": [table_schema()]}),
            result_eq: ptrs([
                ("/count", json!(1)),
                ("/tables/0/id", json!(TABLE)),
                ("/tables/0/primary_field_id", json!("fldName00001")),
                ("/tables/0/fields/1/name", json!("Status")),
                ("/tables/0/fields/1/type", json!("singleSelect")),
                ("/tables/0/views/1/name", json!("Kanban")),
            ]),
        },
        // src/lib/actions/list-records.ts: GET /v0/{base}/{table}?filterByFormula&view&maxRecords&pageSize&sort[i][…]&fields[i]
        Case {
            tool: "AIRTABLE_LIST_RECORDS",
            args: json!({
                "base_id": BASE, "table_id_or_name": "Sales Pipeline",
                "filter_by_formula": "AND({Status}=\"Todo\",{Estimate}>2)",
                "view": "viwKanban001",
                "max_records": 250, "page_size": 50,
                "sort": [{"field": "Due", "direction": "asc"}],
                "fields": ["Name", "Status"]}),
            method: "GET",
            url_contains: urls(&[
                ENDPOINT,
                "/appAcmeProject01/Sales%20Pipeline?",
                "filterByFormula=",
                "view=viwKanban001",
                "maxRecords=250",
                "pageSize=50",
                "sort%5B0%5D%5Bfield%5D=Due",
                "sort%5B0%5D%5Bdirection%5D=asc",
                "fields%5B0%5D=Name",
                "fields%5B1%5D=Status",
            ]),
            body_eq: ptrs([]),
            response_status: 200,
            response: json!({
                "records": [raw_record(RECORD, "Renew Acme contract"), raw_record("recSecond002", "Ship v2")],
                "offset": "itrPageTwo0001"}),
            result_eq: ptrs([
                ("/count", json!(2)),
                ("/records/0/id", json!(RECORD)),
                ("/records/0/created_time", json!("2026-01-14T09:12:33.000Z")),
                ("/records/0/fields/Name", json!("Renew Acme contract")),
                ("/records/0/fields/Estimate", json!(3)),
                ("/records/1/fields/Tags/1", json!("b")),
                ("/offset", json!("itrPageTwo0001")),
            ]),
        },
        // src/lib/actions/search-records.ts: GET /v0/{base}/{table}?filterByFormula=FIND("acme",{Name})
        Case {
            tool: "AIRTABLE_SEARCH_RECORDS",
            args: json!({
                "base_id": BASE, "table_id_or_name": TABLE,
                "search_field": "Name", "search_value": "Acme \"big\""}),
            method: "GET",
            url_contains: urls(&[
                ENDPOINT,
                frag(format!("/{BASE}/{TABLE}?")),
                "filterByFormula=FIND%28%22Acme+%5C%22big%5C%22%22%2C%7BName%7D%29",
            ]),
            body_eq: ptrs([]),
            response_status: 200,
            response: json!({"records": [raw_record(RECORD, "Renew Acme \"big\" contract")]}),
            result_eq: ptrs([
                ("/count", json!(1)),
                ("/records/0/id", json!(RECORD)),
                ("/records/0/fields/Name", json!("Renew Acme \"big\" contract")),
            ]),
        },
        // src/lib/actions/find-record.ts -> findRecord: GET /v0/{base}/{table}?filterByFormula=FIND(...),pageSize=100
        Case {
            tool: "AIRTABLE_FIND_RECORD",
            args: json!({
                "base_id": BASE, "table_id_or_name": TABLE,
                "search_field": "Name", "search_value": "Acme", "view": "viwGrid00001"}),
            method: "GET",
            url_contains: urls(&[
                ENDPOINT,
                frag(format!("/{BASE}/{TABLE}?")),
                "filterByFormula=FIND%28%22Acme%22%2C%7BName%7D%29",
                "pageSize=100",
                "view=viwGrid00001",
            ]),
            body_eq: ptrs([]),
            response_status: 200,
            response: json!({"records": [raw_record(RECORD, "Renew Acme contract")]}),
            result_eq: ptrs([
                ("/count", json!(1)),
                ("/records/0/id", json!(RECORD)),
            ]),
        },
        // src/lib/actions/get-record-ai.ts -> getRecordById: GET /v0/{base}/{table}/{recordId}
        Case {
            tool: "AIRTABLE_GET_RECORD",
            args: json!({"base_id": BASE, "table_id_or_name": TABLE, "record_id": RECORD}),
            method: "GET",
            url_contains: urls(&[ENDPOINT, frag(format!("/{BASE}/{TABLE}/{RECORD}"))]),
            body_eq: ptrs([]),
            response_status: 200,
            response: raw_record(RECORD, "Renew Acme contract"),
            result_eq: ptrs([
                ("/id", json!(RECORD)),
                ("/created_time", json!("2026-01-14T09:12:33.000Z")),
                ("/fields/Status", json!("Todo")),
            ]),
        },
        // src/lib/actions/find-record-by-id.ts -> getRecordById (same endpoint)
        Case {
            tool: "AIRTABLE_GET_RECORD_BY_ID",
            args: json!({"base_id": BASE, "table_id_or_name": "Tasks", "record_id": RECORD}),
            method: "GET",
            url_contains: urls(&[ENDPOINT, frag(format!("/{BASE}/Tasks/{RECORD}"))]),
            body_eq: ptrs([]),
            response_status: 200,
            response: raw_record(RECORD, "Renew Acme contract"),
            result_eq: ptrs([("/id", json!(RECORD))]),
        },
        // src/lib/actions/list-record-comments.ts: GET /v0/{base}/{table}/{recordId}/comments
        Case {
            tool: "AIRTABLE_LIST_RECORD_COMMENTS",
            args: json!({
                "base_id": BASE, "table_id_or_name": TABLE, "record_id": RECORD, "page_size": 25}),
            method: "GET",
            url_contains: urls(&[
                ENDPOINT,
                frag(format!("/{BASE}/{TABLE}/{RECORD}/comments?pageSize=25")),
            ]),
            body_eq: ptrs([]),
            response_status: 200,
            response: json!({
                "comments": [
                    {"id": "comFirst000001", "text": "Ping me when done", "createdTime": "2026-01-15T08:00:00.000Z",
                     "lastUpdatedTime": null, "parentCommentId": null,
                     "author": {"id": "usrAdaLovelace01", "email": "ada@acme.com", "name": "Ada Lovelace"}},
                    {"id": "comSecond00002", "text": "Done 👍", "createdTime": "2026-01-15T09:30:00.000Z",
                     "lastUpdatedTime": "2026-01-15T09:31:00.000Z", "parentCommentId": "comFirst000001",
                     "author": {"id": "usrBobMartin02", "email": "bob@acme.com", "name": "Bob Martin"}}
                ],
                "offset": "comNextPage001"}),
            result_eq: ptrs([
                ("/count", json!(2)),
                ("/comments/0/text", json!("Ping me when done")),
                ("/comments/0/author/email", json!("ada@acme.com")),
                ("/comments/1/parent_comment_id", json!("comFirst000001")),
                ("/offset", json!("comNextPage001")),
            ]),
        },
        // src/lib/actions/create-record-ai.ts -> createRecord: POST /v0/{base}/{table} {fields, typecast: true}
        Case {
            tool: "AIRTABLE_CREATE_RECORD",
            args: json!({
                "base_id": BASE, "table_id_or_name": TABLE,
                "fields": {"Name": "Renew Acme contract", "Estimate": 3, "Tags": ["a", "b"]}}),
            method: "POST",
            url_contains: urls(&[ENDPOINT, frag(format!("/{BASE}/{TABLE}"))]),
            body_eq: ptrs([
                ("/fields/Name", json!("Renew Acme contract")),
                ("/fields/Estimate", json!(3)),
                ("/fields/Tags/1", json!("b")),
                ("/typecast", json!(true)),
            ]),
            response_status: 200,
            response: raw_record("recNewRecord01", "Renew Acme contract"),
            result_eq: ptrs([
                ("/id", json!("recNewRecord01")),
                ("/fields/Name", json!("Renew Acme contract")),
            ]),
        },
        // src/lib/actions/update-record-ai.ts -> updateRecord: PATCH /v0/{base}/{table}/{recordId}
        Case {
            tool: "AIRTABLE_UPDATE_RECORD",
            args: json!({
                "base_id": BASE, "table_id_or_name": TABLE, "record_id": RECORD,
                "fields": {"Status": "Done", "Due": null}, "typecast": true}),
            method: "PATCH",
            url_contains: urls(&[ENDPOINT, frag(format!("/{BASE}/{TABLE}/{RECORD}"))]),
            body_eq: ptrs([
                ("/fields/Status", json!("Done")),
                ("/fields/Due", Value::Null),
                ("/typecast", json!(true)),
            ]),
            response_status: 200,
            response: json!({
                "id": RECORD, "createdTime": "2026-01-14T09:12:33.000Z",
                "fields": {"Name": "Renew Acme contract", "Status": "Done", "Due": "", "Estimate": 3}}),
            result_eq: ptrs([
                ("/id", json!(RECORD)),
                ("/fields/Status", json!("Done")),
                ("/fields/Due", json!("")),
            ]),
        },
        // src/lib/actions/clean-record.ts -> updateRecord(typecast: true) with blanks emptied
        Case {
            tool: "AIRTABLE_CLEAR_RECORD",
            args: json!({
                "base_id": BASE, "table_id_or_name": TABLE, "record_id": RECORD,
                "fields": {"Status": "Done", "Due": "", "Tags": [], "Estimate": 0}}),
            method: "PATCH",
            url_contains: urls(&[ENDPOINT, frag(format!("/{BASE}/{TABLE}/{RECORD}"))]),
            body_eq: ptrs([
                ("/fields/Status", json!("Done")),
                ("/fields/Due", Value::Null),
                ("/fields/Tags", json!([])),
                ("/fields/Estimate", json!(0)),
                ("/typecast", json!(true)),
            ]),
            response_status: 200,
            response: raw_record(RECORD, "Renew Acme contract"),
            result_eq: ptrs([("/id", json!(RECORD))]),
        },
        // src/lib/actions/delete-record-ai.ts -> deleteRecord: DELETE /v0/{base}/{table}/{recordId}
        Case {
            tool: "AIRTABLE_DELETE_RECORD",
            args: json!({"base_id": BASE, "table_id_or_name": TABLE, "record_id": RECORD}),
            method: "DELETE",
            url_contains: urls(&[ENDPOINT, frag(format!("/{BASE}/{TABLE}/{RECORD}"))]),
            body_eq: ptrs([]),
            response_status: 200,
            response: json!({"id": RECORD, "deleted": true}),
            result_eq: ptrs([("/id", json!(RECORD)), ("/deleted", json!(true))]),
        },
        // src/lib/actions/delete-records-batch.ts: DELETE /v0/{base}/{table}?records[0]=…&records[1]=…
        Case {
            tool: "AIRTABLE_DELETE_RECORDS_BATCH",
            args: json!({
                "base_id": BASE, "table_id_or_name": TABLE,
                "record_ids": [RECORD, "recSecond002"]}),
            method: "DELETE",
            url_contains: urls(&[
                ENDPOINT,
                frag(format!("/{BASE}/{TABLE}?")),
                "records%5B0%5D=recAbc123XYZ",
                "records%5B1%5D=recSecond002",
            ]),
            body_eq: ptrs([]),
            response_status: 200,
            response: json!({"records": [
                {"id": RECORD, "deleted": true},
                {"id": "recSecond002", "deleted": true}]}),
            result_eq: ptrs([
                ("/count", json!(2)),
                ("/records/0/id", json!(RECORD)),
                ("/records/1/deleted", json!(true)),
            ]),
        },
        // src/lib/actions/upsert-records.ts: PATCH /v0/{base}/{table} {records, typecast, performUpsert?}
        Case {
            tool: "AIRTABLE_UPSERT_RECORDS",
            args: json!({
                "base_id": BASE, "table_id_or_name": TABLE,
                "records": [
                    {"id": RECORD, "fields": {"Status": "Done"}},
                    {"fields": {"Name": "Ship v2", "Status": "Todo"}}],
                "fields_to_merge_on": ["Email"]}),
            method: "PATCH",
            url_contains: urls(&[ENDPOINT, frag(format!("/{BASE}/{TABLE}"))]),
            body_eq: ptrs([
                ("/records/0/id", json!(RECORD)),
                ("/records/0/fields/Status", json!("Done")),
                ("/records/1/fields/Name", json!("Ship v2")),
                ("/performUpsert/fieldsToMergeOn/0", json!("Email")),
                ("/typecast", json!(true)),
            ]),
            response_status: 200,
            response: json!({
                "records": [raw_record(RECORD, "Renew Acme contract"), raw_record("recNewRecord01", "Ship v2")],
                "createdRecords": ["recNewRecord01"],
                "updatedRecords": [RECORD]}),
            result_eq: ptrs([
                ("/count", json!(2)),
                ("/records/0/id", json!(RECORD)),
                ("/created_records/0", json!("recNewRecord01")),
                ("/updated_records/0", json!(RECORD)),
            ]),
        },
        // src/lib/actions/add-comment-to-record-ai.ts -> addCommentToRecord:
        // POST /v0/{base}/{table}/{recordId}/comments {text, parentCommentId?}
        Case {
            tool: "AIRTABLE_ADD_COMMENT_TO_RECORD",
            args: json!({
                "base_id": BASE, "table_id_or_name": TABLE, "record_id": RECORD,
                "text": "Blocked on legal @[usrAdaLovelace01]", "parent_comment_id": "comFirst000001"}),
            method: "POST",
            url_contains: urls(&[ENDPOINT, frag(format!("/{BASE}/{TABLE}/{RECORD}/comments"))]),
            body_eq: ptrs([
                ("/text", json!("Blocked on legal @[usrAdaLovelace01]")),
                ("/parentCommentId", json!("comFirst000001")),
            ]),
            response_status: 201,
            response: json!({
                "id": "comReply0000001", "text": "Blocked on legal @[usrAdaLovelace01]",
                "createdTime": "2026-02-01T10:00:00.000Z", "lastUpdatedTime": null,
                "parentCommentId": "comFirst000001",
                "author": {"id": "usrBobMartin02", "email": "bob@acme.com", "name": "Bob Martin"}}),
            result_eq: ptrs([
                ("/id", json!("comReply0000001")),
                ("/text", json!("Blocked on legal @[usrAdaLovelace01]")),
                ("/parent_comment_id", json!("comFirst000001")),
            ]),
        },
        // src/lib/actions/create-base-ai.ts -> createBase: POST /v0/meta/bases {name, workspaceId, tables}
        Case {
            tool: "AIRTABLE_CREATE_BASE",
            args: json!({
                "workspace_id": "wspAcme000001", "name": "Support",
                "tables": [{"name": "Tickets", "fields": [
                    {"name": "Subject", "type": "singleLineText"},
                    {"name": "Status", "type": "singleSelect", "options": {"choices": [{"name": "Open"}]}}]}]}),
            method: "POST",
            url_contains: urls(&[ENDPOINT, "/meta/bases"]),
            body_eq: ptrs([
                ("/name", json!("Support")),
                ("/workspaceId", json!("wspAcme000001")),
                ("/tables/0/name", json!("Tickets")),
                ("/tables/0/fields/1/type", json!("singleSelect")),
                ("/tables/0/fields/1/options/choices/0/name", json!("Open")),
            ]),
            response_status: 201,
            response: json!({"id": "appSupport0001", "tables": [table_schema()]}),
            result_eq: ptrs([
                ("/id", json!("appSupport0001")),
                ("/tables/0/name", json!("Tasks")),
                ("/tables/0/fields/0/name", json!("Name")),
            ]),
        },
        // src/lib/actions/create-table-ai.ts -> createTable: POST /v0/meta/bases/{baseId}/tables
        Case {
            tool: "AIRTABLE_CREATE_TABLE",
            args: json!({
                "base_id": BASE, "name": "Risks", "description": "Risk register",
                "fields": [{"name": "Risk", "type": "singleLineText"},
                           {"name": "Owner", "type": "collaborator"}]}),
            method: "POST",
            url_contains: urls(&[ENDPOINT, frag(format!("/meta/bases/{BASE}/tables"))]),
            body_eq: ptrs([
                ("/name", json!("Risks")),
                ("/description", json!("Risk register")),
                ("/fields/0/name", json!("Risk")),
                ("/fields/1/type", json!("collaborator")),
            ]),
            response_status: 201,
            response: json!({
                "id": "tblRisks00001", "name": "Risks", "description": "Risk register",
                "primaryFieldId": "fldRisk00001",
                "fields": [{"id": "fldRisk00001", "name": "Risk", "type": "singleLineText", "description": null}],
                "views": [{"id": "viwGrid00001", "name": "Grid view", "type": "grid"}]}),
            result_eq: ptrs([
                ("/id", json!("tblRisks00001")),
                ("/name", json!("Risks")),
                ("/primary_field_id", json!("fldRisk00001")),
                ("/fields/0/name", json!("Risk")),
            ]),
        },
        // src/lib/actions/update-table.ts: PATCH /v0/meta/bases/{base_id}/tables/{table}
        Case {
            tool: "AIRTABLE_UPDATE_TABLE",
            args: json!({
                "base_id": BASE, "table_id_or_name": TABLE,
                "name": "Open Tasks", "description": "Everything we promised"}),
            method: "PATCH",
            url_contains: urls(&[ENDPOINT, frag(format!("/meta/bases/{BASE}/tables/{TABLE}"))]),
            body_eq: ptrs([
                ("/name", json!("Open Tasks")),
                ("/description", json!("Everything we promised")),
            ]),
            response_status: 200,
            response: json!({
                "id": TABLE, "name": "Open Tasks", "description": "Everything we promised",
                "primaryFieldId": "fldName00001", "fields": [], "views": []}),
            result_eq: ptrs([
                ("/id", json!(TABLE)),
                ("/name", json!("Open Tasks")),
            ]),
        },
        // src/lib/actions/create-field.ts: POST /v0/meta/bases/{base_id}/tables/{table}/fields
        Case {
            tool: "AIRTABLE_CREATE_FIELD",
            args: json!({
                "base_id": BASE, "table_id_or_name": TABLE, "name": "Due", "type": "date",
                "description": "Target date", "options": {"dateFormat": {"name": "iso", "format": "YYYY-MM-DD"}}}),
            method: "POST",
            url_contains: urls(&[ENDPOINT, frag(format!("/meta/bases/{BASE}/tables/{TABLE}/fields"))]),
            body_eq: ptrs([
                ("/name", json!("Due")),
                ("/type", json!("date")),
                ("/description", json!("Target date")),
                ("/options/dateFormat/name", json!("iso")),
            ]),
            response_status: 201,
            response: json!({
                "id": "fldDue0000001", "name": "Due", "type": "date", "description": "Target date",
                "options": {"dateFormat": {"name": "iso", "format": "YYYY-MM-DD"}}}),
            result_eq: ptrs([
                ("/id", json!("fldDue0000001")),
                ("/name", json!("Due")),
                ("/type", json!("date")),
            ]),
        },
        // src/lib/actions/update-field.ts: PATCH /v0/meta/bases/{base_id}/tables/{table_id}/fields/{field_id}
        Case {
            tool: "AIRTABLE_UPDATE_FIELD",
            args: json!({
                "base_id": BASE, "table_id": TABLE, "field_id": FIELD, "description": "Ticket state"}),
            method: "PATCH",
            url_contains: urls(&[ENDPOINT, frag(format!("/meta/bases/{BASE}/tables/{TABLE}/fields/{FIELD}"))]),
            body_eq: ptrs([("/description", json!("Ticket state"))]),
            response_status: 200,
            response: json!({
                "id": FIELD, "name": "Status", "type": "singleSelect", "description": "Ticket state"}),
            result_eq: ptrs([
                ("/id", json!(FIELD)),
                ("/name", json!("Status")),
                ("/description", json!("Ticket state")),
            ]),
        },
    ];

    for c in &cases {
        run(&a, c);
    }
    support::assert_full_coverage(&a, &cases);
    support::spec_parity("airtable", &a, &cases);
}

/// Provider errors surface as an `Err`, not as data.
#[test]
fn provider_errors_are_reported() {
    let a = adapter();
    let resp = ctox_toolhub::http::HttpResponse {
        status: 404,
        body: serde_json::to_vec(&json!({
            "error": {"type": "MODEL_ID_NOT_FOUND", "message": "Could not find table 'Nope'"}}))
        .unwrap(),
        session_id: None,
    };
    let err = a
        .parse_response("AIRTABLE_LIST_RECORDS", &resp)
        .unwrap_err();
    assert!(err.to_string().contains("MODEL_ID_NOT_FOUND"), "{err}");
    assert!(err.to_string().contains("Could not find table 'Nope'"), "{err}");
}

/// Live read-only calls through the hub exec path (personal access token as
/// `Authorization: Bearer …`).
#[test]
#[ignore]
fn live_read_only_actions() {
    let a = adapter();
    for (tool, args) in [
        ("AIRTABLE_GET_CURRENT_USER", json!({})),
        ("AIRTABLE_LIST_BASES", json!({})),
    ] {
        let Some(result) = live(&a, tool, args) else { return };
        let v = result.unwrap_or_else(|e| panic!("{tool}: {e}"));
        assert!(v.is_object(), "{tool}: expected an object, got {v}");
    }
}