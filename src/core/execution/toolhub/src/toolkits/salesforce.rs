//! Salesforce toolkit (REST API). Action set modelled on Activepieces `piece-salesforce`
//! (MIT), restricted to the non-AI actions of `src/lib/action` plus the object/record
//! dropdown queries of `src/lib/common/index.ts`.
//!
//! Auth: OAuth2 like `salesforceAuth` in `src/index.ts` — authorize/token under
//! `https://<environment>/services/oauth2/…`, PKCE S256, scopes `refresh_token full api`.
//! Every API call then carries the access token as `Authorization: Bearer …`
//! (`AuthenticationType.BEARER_TOKEN` in `callSalesforceApi`).
//!
//! Base URL: the REST API lives on the *org's* instance URL, which Activepieces takes
//! from the OAuth token response (`instance_url`, e.g. `https://acme.my.salesforce.com`).
//! That host is account-specific, so every tool accepts an optional `instance_url`
//! argument (validated to be a bare `https://host`) and the adapter itself is built
//! with `Salesforce::with_api_base(instance_url)`. `Default` uses the piece's default
//! auth domain `https://login.salesforce.com`, which serves OAuth but **not**
//! `/services/data/…` — pass the instance URL (per call or via `with_api_base`).
//!
//! Endpoints (`src/lib/common/index.ts`, `src/lib/common/crm.ts`):
//! * `GET  {instance}/services/data/v56.0/query?q=<SOQL>`   (`querySalesforceApi`)
//! * `GET  {instance}/services/data/v56.0/sobjects`          (object dropdown)
//! * `GET  {instance}/services/data/v56.0/sobjects/{o}/describe` (field dropdown)
//! * `POST {instance}/services/data/v56.0/sobjects/{object}` and
//!   `PATCH|DELETE {instance}/services/data/v56.0/sobjects/{object}[/{id}]`
//! * `GET  {instance}/services/data/v56.0/ui-api/records/{id}/child-relationships/{rel}`
//! * `POST {instance}/services/data/v56.0/analytics/reports/{id}?includeDetails=true`
//! * `PATCH {instance}/services/data/v55.0/composite/sobjects/{object}/{externalField}`
use anyhow::{bail, Result};
use serde_json::{json, Map, Value};

use super::common::*;
use super::{path_segment, ToolkitAdapter};
use crate::http::{HttpRequest, HttpResponse};
use crate::model::*;

pub const SLUG: &str = "salesforce";

/// Scope the piece requests for the REST calls; `refresh_token`/`full` belong to
/// the OAuth flow itself and live in the auth scheme.
const API: &str = "api";

/// API version used by every action except the composite upsert (which the piece
/// still calls on v55.0).
const V56: &str = "/services/data/v56.0";
const V55: &str = "/services/data/v55.0";

/// The piece's default auth domain (`environment` prop in `src/index.ts`).
const DEFAULT_LOGIN_HOST: &str = "https://login.salesforce.com";

/// Per-call override of the instance URL (`instance_url` in the OAuth token response).
fn instance_prop() -> (&'static str, Value) {
    (
        "instance_url",
        s("Instanz-URL der Org (My Domain) aus der OAuth-Token-Antwort, z. B. https://acme.my.salesforce.com"),
    )
}

fn with_instance(mut props: Vec<(&'static str, Value)>) -> Vec<(&'static str, Value)> {
    props.insert(0, instance_prop());
    props
}

// ---------------------------------------------------------------- validation

/// Salesforce record id (`assertId` in `common/utils.ts`): 15 or 18 alphanumerics.
fn record_id(args: &Value, key: &str) -> Result<String> {
    let v = path_segment(args, key)?;
    if (v.len() != 15 && v.len() != 18) || !v.bytes().all(|b| b.is_ascii_alphanumeric()) {
        bail!("`{key}` must be a 15- or 18-character Salesforce record id");
    }
    Ok(v.to_string())
}

/// Object / field API name (`assertApiName`): letter, then letters, digits, `_`.
fn api_name(args: &Value, key: &str) -> Result<String> {
    let v = path_segment(args, key)?;
    let mut bytes = v.bytes();
    let starts_ok = bytes.next().is_some_and(|b| b.is_ascii_alphabetic());
    if !starts_ok || !bytes.all(|b| b.is_ascii_alphanumeric() || b == b'_') {
        bail!("`{key}` must be a Salesforce API name such as Account or My_Field__c");
    }
    Ok(v.to_string())
}

/// Bare `https://host` (the `instance_url` of the token response).
fn validate_instance_url(v: &str) -> Result<String> {
    let Some(host) = v.strip_prefix("https://") else {
        bail!("`instance_url` must start with https://, e.g. https://acme.my.salesforce.com");
    };
    if host.is_empty()
        || host.len() > 200
        || !host
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'-'))
    {
        bail!("`instance_url` must be a bare host such as https://acme.my.salesforce.com");
    }
    Ok(format!("https://{host}"))
}

/// Stored per-connection org host: must be a Salesforce domain so the token
/// can never be sent elsewhere.
fn validate_org_host(v: &str) -> Result<String> {
    let base = validate_instance_url(v.trim().trim_end_matches('/'))?;
    let host = base.trim_start_matches("https://").to_ascii_lowercase();
    if ![".salesforce.com", ".force.com", ".cloudforce.com", ".salesforce.mil"]
        .iter()
        .any(|s| host.ends_with(s))
    {
        bail!("`instance_url` must be a Salesforce host such as https://acme.my.salesforce.com");
    }
    Ok(base)
}

// ---------------------------------------------------------------- SOQL helpers

/// `escapeSoqlValue`: backslash-escape `\ ' " \n \r \t`.
fn escape_soql_value(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    for c in value.chars() {
        match c {
            '\\' => out.push_str("\\\\"),
            '\'' => out.push_str("\\'"),
            '"' => out.push_str("\\\""),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c => out.push(c),
        }
    }
    out
}

/// `escapeSoqlLike`: value escaping plus `%` and `_` for LIKE patterns.
fn escape_soql_like(value: &str) -> String {
    escape_soql_value(value)
        .replace('%', "\\%")
        .replace('_', "\\_")
}

/// `cleanRecord`: drop the `attributes` node Salesforce adds to every record.
fn clean_record(value: &Value) -> Value {
    match value {
        Value::Array(a) => Value::Array(a.iter().map(clean_record).collect()),
        Value::Object(m) => Value::Object(
            m.iter()
                .filter(|(k, _)| k.as_str() != "attributes")
                .map(|(k, v)| (k.clone(), clean_record(v)))
                .collect(),
        ),
        other => other.clone(),
    }
}

/// `formatQueryResult`: `{records, count, total_size, done, next_records_url}`.
fn format_query_result(v: &Value) -> Value {
    let records: Vec<Value> = v["records"]
        .as_array()
        .cloned()
        .unwrap_or_default()
        .iter()
        .map(clean_record)
        .collect();
    json!({
        "records": records.clone(),
        "count": records.len(),
        "total_size": v["totalSize"],
        "done": v["done"],
        "next_records_url": v["nextRecordsUrl"],
    })
}

/// `{records, count}` for the small dropdown queries.
fn record_list(v: &Value) -> Value {
    let records: Vec<Value> = v["records"]
        .as_array()
        .cloned()
        .unwrap_or_default()
        .iter()
        .map(clean_record)
        .collect();
    json!({"records": records.clone(), "count": records.len()})
}

// ---------------------------------------------------------------- body helpers

/// Copy an optional string argument under its Salesforce API name, dropping
/// blank values the way the actions' `cleanedBody` reducer does.
fn put_field(m: &mut Map<String, Value>, args: &Value, key: &str, field: &str) {
    if let Some(value) = opt_str(args, key) {
        m.insert(field.into(), json!(value));
    }
}

/// `buildBody` from `common/crm.ts`: API-named fields (blank values dropped, like
/// the actions' `cleanedBody` reducer) plus the caller's `other_fields` JSON
/// object, which wins on conflicts.
fn sobject_body<K: Into<String>>(args: &Value, fields: Vec<(K, Value)>) -> Result<Value> {
    let mut m = Map::new();
    for (field, value) in fields {
        match &value {
            Value::Null => continue,
            Value::String(s) if s.is_empty() => continue,
            _ => {
                m.insert(field.into(), value);
            }
        }
    }
    if let Some(Value::Object(extra)) = args.get("other_fields") {
        for (k, v) in extra {
            if !v.is_null() {
                m.insert(k.clone(), v.clone());
            }
        }
    }
    Ok(Value::Object(m))
}

/// Required field of an update tool: at least one field must be set.
fn require_fields(body: &Value) -> Result<()> {
    if body.as_object().is_some_and(Map::is_empty) {
        bail!("nothing to update: pass at least one field to change");
    }
    Ok(())
}

/// `fields` argument of the generic create/update record actions.
fn fields_object(args: &Value) -> Result<Value> {
    match args.get("fields") {
        Some(Value::Object(m)) => Ok(Value::Object(m.clone())),
        Some(_) => bail!("`fields` must be a JSON object of API name to value"),
        None => bail!("missing argument `fields`"),
    }
}

// ---------------------------------------------------------------- report rows

/// `transformReportToRows` from `common/report.ts`: flatten `factMap` into rows.
fn report_rows(report: &Value) -> Value {
    let detail_columns = report["reportMetadata"]["detailColumns"]
        .as_array()
        .cloned()
        .unwrap_or_default();
    let detail_info = &report["reportExtendedMetadata"]["detailColumnInfo"];
    let groupings_down = report["reportMetadata"]["groupingsDown"]
        .as_array()
        .cloned()
        .unwrap_or_default();
    let grouping_info = &report["reportExtendedMetadata"]["groupingColumnInfo"];

    let label = |info: &Value, name: &Value| -> String {
        name.as_str()
            .map(|n| {
                info[n]["label"]
                    .as_str()
                    .map(String::from)
                    .unwrap_or_else(|| n.to_string())
            })
            .unwrap_or_default()
    };
    let column_labels: Vec<String> = detail_columns.iter().map(|c| label(detail_info, c)).collect();
    let grouping_labels: Vec<String> = groupings_down.iter().map(|g| label(grouping_info, &g["name"])).collect();

    // key -> {label, depth} for the whole grouping tree.
    let mut grouping_values = Map::new();
    collect_grouping_values(
        report["groupingsDown"]["groupings"].as_array(),
        0,
        &mut grouping_values,
    );

    let mut rows: Vec<Value> = Vec::new();
    if let Some(fact_map) = report["factMap"].as_object() {
        for (fact_key, entry) in fact_map {
            let Some(entry_rows) = entry["rows"].as_array() else {
                continue;
            };
            let context = grouping_context(fact_key, &grouping_values, &grouping_labels);
            for row in entry_rows {
                let mut m = Map::new();
                for (k, v) in &context {
                    m.insert(k.clone(), json!(v));
                }
                for (i, cell) in row["dataCells"].as_array().into_iter().flatten().enumerate() {
                    let value = if cell["label"].is_string() {
                        cell["label"].as_str()
                    } else {
                        cell["value"].as_str()
                    };
                    let key = column_labels
                        .get(i)
                        .cloned()
                        .unwrap_or_else(|| format!("Column_{i}"));
                    m.insert(
                        key,
                        match value {
                            None => Value::Null,
                            Some("-") | Some("--") => json!(""),
                            Some(v) => json!(v),
                        },
                    );
                }
                rows.push(Value::Object(m));
            }
        }
    }

    let mut columns = grouping_labels.clone();
    columns.extend(column_labels);
    json!({
        "report_name": report["attributes"]["reportName"]
            .as_str()
            .or_else(|| report["reportMetadata"]["name"].as_str())
            .unwrap_or("Unknown Report"),
        "report_id": report["attributes"]["reportId"].as_str().unwrap_or("Unknown Report"),
        "total_rows": rows.len(),
        "columns": columns,
        "rows": rows,
    })
}

fn collect_grouping_values(groupings: Option<&Vec<Value>>, depth: u64, out: &mut Map<String, Value>) {
    let Some(groupings) = groupings else { return };
    for grouping in groupings {
        let Some(key) = grouping["key"].as_str() else {
            continue;
        };
        out.insert(key.to_string(), json!({"label": grouping["label"], "depth": depth}));
        collect_grouping_values(grouping["groupings"].as_array(), depth + 1, out);
    }
}

/// Prefix of the factMap key (`0_1!T…`) mapped to grouping labels.
fn grouping_context(
    fact_key: &str,
    grouping_values: &Map<String, Value>,
    grouping_labels: &[String],
) -> Vec<(String, String)> {
    let Some((row_part, _)) = fact_key.split_once('!') else {
        return Vec::new();
    };
    if row_part.is_empty() || row_part == "T" {
        return Vec::new();
    }
    let keys: Vec<&str> = row_part.split('_').collect();
    let mut out = Vec::new();
    for depth in 0..keys.len() {
        let cumulative = keys[..=depth].join("_");
        let Some(info) = grouping_values.get(&cumulative) else {
            continue;
        };
        let Some(label) = info["label"].as_str() else {
            continue;
        };
        let name = grouping_labels
            .get(depth)
            .cloned()
            .unwrap_or_else(|| format!("Group_{depth}"));
        out.push((name, label.to_string()));
    }
    out
}

// ---------------------------------------------------------------- adapter

pub struct Salesforce {
    spec: ToolkitSpec,
    api_base: String,
}

impl Default for Salesforce {
    fn default() -> Self {
        Self::with_api_base(DEFAULT_LOGIN_HOST)
    }
}

#[allow(clippy::too_many_lines)]
impl Salesforce {
    pub fn with_api_base(api_base: &str) -> Self {
        let t = |slug, title, desc, props: Props<'_>, req: &[&str], scopes: &[&str], m| {
            tool(SLUG, slug, title, desc, props, req, scopes, m)
        };
        let spec = ToolkitSpec {
            slug: SLUG.into(),
            name: "Salesforce".into(),
            description: LocalizedText::new(
                "Salesforce-CRM: Datensätze suchen, lesen, anlegen, ändern und Berichte ausführen.",
                "Salesforce CRM: search, read, create and update records and run reports.",
            ),
            auth_schemes: vec![
                AuthScheme::OAuth2(OAuth2Scheme {
                    // `https://{environment}/services/oauth2/…` in `src/index.ts`,
                    // with the piece's default `environment` = login.salesforce.com.
                    // Sandbox / My Domain orgs substitute their own host here.
                    authorize_url: "https://login.salesforce.com/services/oauth2/authorize".into(),
                    token_url: "https://login.salesforce.com/services/oauth2/token".into(),
                    default_scopes: ["refresh_token", "full", "api"]
                        .iter()
                        .map(|s| (*s).to_string())
                        .collect(),
                    pkce: true,
                    scope_separator: " ".into(),
                    refreshable: true,
                    extra_authorize_params: Vec::new(),
                }),
                // `callSalesforceApi` sends the OAuth access token as a bearer token.
                AuthScheme::Bearer,
            ],
            tools: vec![
                // ---------------------------------------------------------- reads
                t(
                    "SALESFORCE_RUN_SOQL_QUERY",
                    ("SOQL-Abfrage ausführen", "Run SOQL query"),
                    (
                        "Führt eine beliebige SOQL-Abfrage aus (Run SF Query des Stücks).",
                        "Runs an arbitrary SOQL query (the piece's Run SF Query action).",
                    ),
                    &with_instance(vec![(
                        "query",
                        s("SOQL-Abfrage, z. B. SELECT Id, Name FROM Account LIMIT 10"),
                    )]),
                    &["query"],
                    &[API],
                    false,
                ),
                t(
                    "SALESFORCE_FIND_RECORDS_BY_QUERY",
                    ("Datensätze per WHERE-Bedingung suchen", "Find records by query"),
                    (
                        "Sucht Datensätze eines Objekts über eine SOQL-WHERE-Bedingung (bis 200).",
                        "Finds records of one object with a SOQL WHERE clause (up to 200).",
                    ),
                    &with_instance(vec![
                        ("object", s("API-Name des Objekts, z. B. Account oder My_Object__c")),
                        (
                            "where_clause",
                            s("WHERE-Bedingung ohne das Wort WHERE, z. B. Industry = 'Technology'"),
                        ),
                    ]),
                    &["object", "where_clause"],
                    &[API],
                    false,
                ),
                t(
                    "SALESFORCE_FIND_RECORD",
                    ("Datensatz nach Feldwert suchen", "Find record"),
                    (
                        "Findet Datensätze, deren Feld exakt dem Suchwert entspricht (bis 200).",
                        "Finds records whose field equals the search value (up to 200).",
                    ),
                    &with_instance(vec![
                        ("object", s("API-Name des Objekts")),
                        ("field", s("API-Name des Vergleichsfelds, z. B. Email")),
                        ("search_value", s("Exakter Vergleichswert")),
                    ]),
                    &["object", "field", "search_value"],
                    &[API],
                    false,
                ),
                t(
                    "SALESFORCE_FIND_CHILD_RECORDS",
                    ("Kinddatensätze lesen", "Find child records"),
                    (
                        "Liest die untergeordneten Datensätze eines Datensatzes über eine Child-Relationship.",
                        "Reads the child records of a record through a child relationship.",
                    ),
                    &with_instance(vec![
                        ("parent_id", s("Record-ID des Elterndatensatzes")),
                        (
                            "child_relationship",
                            s("Name der Child-Relationship, z. B. Contacts oder Opportunities"),
                        ),
                    ]),
                    &["parent_id", "child_relationship"],
                    &[API],
                    false,
                ),
                t(
                    "SALESFORCE_RUN_REPORT",
                    ("Bericht ausführen", "Run report"),
                    (
                        "Führt einen Analytics-Bericht aus und liefert die Zeilen mit Gruppen- und Detailspalten.",
                        "Runs an analytics report and returns its rows with grouping and detail columns.",
                    ),
                    &with_instance(vec![
                        ("report_id", s("Report-ID (15/18 Zeichen)")),
                        (
                            "filters",
                            json!({"type": "array", "items": {"type": "object"},
                                "description": "Dynamische Filter, z. B. [{\"column\": \"ACCOUNT.NAME\", \"operator\": \"equals\", \"value\": \"Acme\"}]; leer = gespeicherte Filter"}),
                        ),
                    ]),
                    &["report_id"],
                    &[API],
                    false,
                ),
                t(
                    "SALESFORCE_LIST_CLASSIC_ATTACHMENTS",
                    ("Klassische Anhänge auflisten", "List classic attachments"),
                    (
                        "Listet die klassischen Attachment-Datensätze (Name, Größe, Content-Type) eines Datensatzes.",
                        "Lists the classic Attachment records (name, size, content type) of a record.",
                    ),
                    &with_instance(vec![("record_id", s("Record-ID des Elterndatensatzes"))]),
                    &["record_id"],
                    &[API],
                    false,
                ),
                t(
                    "SALESFORCE_LIST_RECORD_FILES",
                    ("Salesforce-Dateien auflisten", "List record files"),
                    (
                        "Listet die ContentDocumentLink-Dateien (Files) eines Datensatzes.",
                        "Lists the ContentDocumentLink files of a record.",
                    ),
                    &with_instance(vec![("record_id", s("Record-ID des Datensatzes"))]),
                    &["record_id"],
                    &[API],
                    false,
                ),
                // ------------------------------------------------- list helpers
                t(
                    "SALESFORCE_LIST_OBJECTS",
                    ("Objekte auflisten", "List objects"),
                    (
                        "Listet alle sObjects der Org (Name und Label) für das Objekt-Argument der Werkzeuge.",
                        "Lists every sobject of the org (name and label) for the object argument of the tools.",
                    ),
                    &with_instance(vec![]),
                    &[],
                    &[API],
                    false,
                ),
                t(
                    "SALESFORCE_DESCRIBE_OBJECT",
                    ("Objekt beschreiben", "Describe object"),
                    (
                        "Liest Felder, Picklists und Child-Relationships eines Objekts (`sobjects/{object}/describe`).",
                        "Reads the fields, picklists and child relationships of an object (`sobjects/{object}/describe`).",
                    ),
                    &with_instance(vec![("object", s("API-Name des Objekts"))]),
                    &["object"],
                    &[API],
                    false,
                ),
                t(
                    "SALESFORCE_LIST_ACCOUNTS",
                    ("Accounts auflisten", "List accounts"),
                    (
                        "Listet bis zu 100 Accounts nach Namen (Account-Dropdown des Stücks).",
                        "Lists up to 100 accounts by name (the piece's account dropdown).",
                    ),
                    &with_instance(vec![]),
                    &[],
                    &[API],
                    false,
                ),
                t(
                    "SALESFORCE_LIST_CONTACTS",
                    ("Kontakte auflisten", "List contacts"),
                    (
                        "Listet bis zu 200 Kontakte mit Name und E-Mail (Kontakt-Dropdown des Stücks).",
                        "Lists up to 200 contacts with name and email (the piece's contact dropdown).",
                    ),
                    &with_instance(vec![]),
                    &[],
                    &[API],
                    false,
                ),
                t(
                    "SALESFORCE_LIST_USERS",
                    ("Nutzer auflisten", "List users"),
                    (
                        "Listet die aktiven Nutzer der Org (Owner-Dropdown des Stücks).",
                        "Lists the active users of the org (the piece's owner dropdown).",
                    ),
                    &with_instance(vec![]),
                    &[],
                    &[API],
                    false,
                ),
                t(
                    "SALESFORCE_LIST_REPORTS",
                    ("Berichte auflisten", "List reports"),
                    (
                        "Listet die Analytics-Berichte, optional nach Namensanfang gefiltert (Report-Dropdown).",
                        "Lists the analytics reports, optionally filtered by name prefix (report dropdown).",
                    ),
                    &with_instance(vec![(
                        "name_prefix",
                        s("Berichte, deren Name mit diesem Text beginnt"),
                    )]),
                    &[],
                    &[API],
                    false,
                ),
                // --------------------------------------------------------- writes
                t(
                    "SALESFORCE_CREATE_CONTACT",
                    ("Kontakt anlegen", "Create contact"),
                    (
                        "Legt einen Contact an (Nachname Pflicht); braucht --confirm.",
                        "Creates a Contact (last name required); requires --confirm.",
                    ),
                    &with_instance(vec![
                        ("last_name", s("LastName (Pflicht)")),
                        ("first_name", s("FirstName")),
                        ("account_id", s("AccountId, verknüpft den Kontakt mit einem Account")),
                        ("email", s("Email")),
                        ("phone", s("Phone")),
                        ("title", s("Title")),
                        ("other_fields", json!({"type": "object", "description": "Weitere Felder als JSON-Objekt von API-Name zu Wert, z. B. {\"My_Field__c\": \"x\"}."})),
                    ]),
                    &["last_name"],
                    &[API],
                    true,
                ),
                t(
                    "SALESFORCE_CREATE_LEAD",
                    ("Lead anlegen", "Create lead"),
                    (
                        "Legt einen Lead an (Nachname und Firma Pflicht); braucht --confirm.",
                        "Creates a Lead (last name and company required); requires --confirm.",
                    ),
                    &with_instance(vec![
                        ("last_name", s("LastName (Pflicht)")),
                        ("company", s("Company (Pflicht)")),
                        ("first_name", s("FirstName")),
                        ("email", s("Email")),
                        ("phone", s("Phone")),
                        ("lead_source", s("LeadSource, Wert der LeadSource-Picklist")),
                        ("other_fields", json!({"type": "object", "description": "Weitere Felder als JSON-Objekt von API-Name zu Wert."})),
                    ]),
                    &["last_name", "company"],
                    &[API],
                    true,
                ),
                t(
                    "SALESFORCE_CREATE_CASE",
                    ("Case anlegen", "Create case"),
                    (
                        "Legt einen Support-Case an (Betreff Pflicht); braucht --confirm.",
                        "Creates a support Case (subject required); requires --confirm.",
                    ),
                    &with_instance(vec![
                        ("subject", s("Subject (Pflicht)")),
                        ("description", s("Description")),
                        ("status", s("Status, Wert der CaseStatus-Picklist")),
                        ("priority", s("Priority, Wert der CasePriority-Picklist")),
                        ("origin", s("Origin, Wert der CaseOrigin-Picklist")),
                        ("account_id", s("AccountId")),
                        ("contact_id", s("ContactId")),
                        ("other_fields", json!({"type": "object", "description": "Weitere Felder als JSON-Objekt von API-Name zu Wert."})),
                    ]),
                    &["subject"],
                    &[API],
                    true,
                ),
                t(
                    "SALESFORCE_CREATE_OPPORTUNITY",
                    ("Opportunity anlegen", "Create opportunity"),
                    (
                        "Legt eine Opportunity an (Name, CloseDate, Stage Pflicht); braucht --confirm.",
                        "Creates an Opportunity (name, close date, stage required); requires --confirm.",
                    ),
                    &with_instance(vec![
                        ("name", s("Name (Pflicht)")),
                        ("close_date", s("CloseDate im Format YYYY-MM-DD (Pflicht)")),
                        ("stage_name", s("StageName, Wert der OpportunityStage-Picklist (Pflicht)")),
                        ("account_id", s("AccountId")),
                        ("amount", json!({"type": "number", "description": "Amount"})),
                        ("other_fields", json!({"type": "object", "description": "Weitere Felder als JSON-Objekt von API-Name zu Wert."})),
                    ]),
                    &["name", "close_date", "stage_name"],
                    &[API],
                    true,
                ),
                t(
                    "SALESFORCE_CREATE_TASK",
                    ("Task anlegen", "Create task"),
                    (
                        "Legt eine Aktivitäts-Task an (Betreff Pflicht); braucht --confirm.",
                        "Creates an activity Task (subject required); requires --confirm.",
                    ),
                    &with_instance(vec![
                        ("subject", s("Subject (Pflicht)")),
                        ("owner_id", s("OwnerId, aus SALESFORCE_LIST_USERS")),
                        ("status", s("Status, Wert der TaskStatus-Picklist")),
                        ("priority", s("Priority, Wert der TaskPriority-Picklist")),
                        ("description", s("Description")),
                        ("who_id", s("WhoId: Contact- oder Lead-ID")),
                        ("what_id", s("WhatId: Account-, Opportunity- oder andere Record-ID")),
                    ]),
                    &["subject"],
                    &[API],
                    true,
                ),
                t(
                    "SALESFORCE_CREATE_NOTE",
                    ("Notiz anlegen", "Create note"),
                    (
                        "Legt eine klassische Notiz an und hängt sie an einen Datensatz; braucht --confirm.",
                        "Creates a classic Note and attaches it to a record; requires --confirm.",
                    ),
                    &with_instance(vec![
                        ("parent_id", s("ParentId: Record-ID, an die die Notiz gehängt wird (Pflicht)")),
                        ("title", s("Title (Pflicht)")),
                        ("body", s("Body: Inhalt der Notiz (Pflicht)")),
                    ]),
                    &["parent_id", "title", "body"],
                    &[API],
                    true,
                ),
                t(
                    "SALESFORCE_ADD_CONTACT_TO_CAMPAIGN",
                    ("Kontakt zur Kampagne hinzufügen", "Add contact to campaign"),
                    (
                        "Legt einen CampaignMember an, der einen Kontakt mit einer Kampagne verknüpft; braucht --confirm.",
                        "Creates a CampaignMember linking a contact to a campaign; requires --confirm.",
                    ),
                    &with_instance(vec![
                        ("campaign_id", s("CampaignId (Pflicht)")),
                        ("contact_id", s("ContactId (Pflicht)")),
                        (
                            "status",
                            s("Status der Kampagnenmitgliedschaft (Pflicht), z. B. Sent oder Responded"),
                        ),
                    ]),
                    &["campaign_id", "contact_id", "status"],
                    &[API],
                    true,
                ),
                t(
                    "SALESFORCE_ADD_LEAD_TO_CAMPAIGN",
                    ("Lead zur Kampagne hinzufügen", "Add lead to campaign"),
                    (
                        "Legt einen CampaignMember an, der einen Lead mit einer Kampagne verknüpft; braucht --confirm.",
                        "Creates a CampaignMember linking a lead to a campaign; requires --confirm.",
                    ),
                    &with_instance(vec![
                        ("campaign_id", s("CampaignId (Pflicht)")),
                        ("lead_id", s("LeadId (Pflicht)")),
                        (
                            "status",
                            s("Status der Kampagnenmitgliedschaft (Pflicht), z. B. Sent oder Responded"),
                        ),
                    ]),
                    &["campaign_id", "lead_id", "status"],
                    &[API],
                    true,
                ),
                t(
                    "SALESFORCE_SEND_EMAIL",
                    ("E-Mail senden", "Send email"),
                    (
                        "Sendet eine E-Mail an einen Kontakt oder Lead, indem ein EmailMessage-Datensatz entsteht; braucht --confirm.",
                        "Sends an email to a Contact or Lead by creating an EmailMessage record; requires --confirm.",
                    ),
                    &with_instance(vec![
                        ("recipient_id", s("ToIds: Record-ID des Kontakts oder Leads (Pflicht)")),
                        ("subject", s("Subject (Pflicht)")),
                        ("body", s("HtmlBody: Inhalt, Klartext oder HTML (Pflicht)")),
                        (
                            "related_to_id",
                            s("RelatedToId: Record-ID, mit der die E-Mail verknüpft wird"),
                        ),
                    ]),
                    &["recipient_id", "subject", "body"],
                    &[API],
                    true,
                ),
                t(
                    "SALESFORCE_CREATE_RECORD",
                    ("Datensatz anlegen", "Create record"),
                    (
                        "Legt einen Datensatz eines beliebigen Objekts an; braucht --confirm.",
                        "Creates a record of any object; requires --confirm.",
                    ),
                    &with_instance(vec![
                        ("object", s("API-Name des Objekts (Pflicht)")),
                        (
                            "fields",
                            json!({"type": "object", "description": "Felder des neuen Datensatzes als JSON-Objekt, z. B. {\"Name\": \"Acme\"}."}),
                        ),
                    ]),
                    &["object", "fields"],
                    &[API],
                    true,
                ),
                t(
                    "SALESFORCE_UPDATE_RECORD",
                    ("Datensatz ändern", "Update record"),
                    (
                        "Ändert die übergebenen Felder eines Datensatzes; braucht --confirm.",
                        "Changes the fields you pass on a record; requires --confirm.",
                    ),
                    &with_instance(vec![
                        ("object", s("API-Name des Objekts (Pflicht)")),
                        ("record_id", s("Record-ID (Pflicht)")),
                        (
                            "fields",
                            json!({"type": "object", "description": "Zu ändernde Felder als JSON-Objekt von API-Name zu Wert."}),
                        ),
                    ]),
                    &["object", "record_id", "fields"],
                    &[API],
                    true,
                ),
                t(
                    "SALESFORCE_UPDATE_CONTACT",
                    ("Kontakt ändern", "Update contact"),
                    (
                        "Ändert nur die übergebenen Felder eines Kontakts; braucht --confirm.",
                        "Changes only the fields you pass on a contact; requires --confirm.",
                    ),
                    &with_instance(vec![
                        ("contact_id", s("Contact-ID (Pflicht)")),
                        ("first_name", s("Neuer FirstName")),
                        ("last_name", s("Neuer LastName")),
                        ("account_id", s("Neue AccountId")),
                        ("email", s("Neue Email")),
                        ("phone", s("Neue Phone")),
                        ("title", s("Neuer Title")),
                        ("other_fields", json!({"type": "object", "description": "Weitere zu ändernde Felder als JSON-Objekt von API-Name zu Wert."})),
                    ]),
                    &["contact_id"],
                    &[API],
                    true,
                ),
                t(
                    "SALESFORCE_UPDATE_LEAD",
                    ("Lead ändern", "Update lead"),
                    (
                        "Ändert nur die übergebenen Felder eines Leads; braucht --confirm.",
                        "Changes only the fields you pass on a lead; requires --confirm.",
                    ),
                    &with_instance(vec![
                        ("lead_id", s("Lead-ID (Pflicht)")),
                        ("first_name", s("Neuer FirstName")),
                        ("last_name", s("Neuer LastName")),
                        ("company", s("Neue Company")),
                        ("email", s("Neue Email")),
                        ("phone", s("Neue Phone")),
                        ("lead_source", s("Neuer LeadSource")),
                        ("other_fields", json!({"type": "object", "description": "Weitere zu ändernde Felder als JSON-Objekt von API-Name zu Wert."})),
                    ]),
                    &["lead_id"],
                    &[API],
                    true,
                ),
                t(
                    "SALESFORCE_DELETE_RECORD",
                    ("Datensatz löschen", "Delete record"),
                    (
                        "Löscht einen Datensatz eines beliebigen Objekts; braucht --confirm.",
                        "Deletes a record of any object; requires --confirm.",
                    ),
                    &with_instance(vec![
                        ("object", s("API-Name des Objekts (Pflicht)")),
                        ("record_id", s("Record-ID (Pflicht)")),
                    ]),
                    &["object", "record_id"],
                    &[API],
                    true,
                ),
                t(
                    "SALESFORCE_DELETE_OPPORTUNITY",
                    ("Opportunity löschen", "Delete opportunity"),
                    (
                        "Löscht eine Opportunity; braucht --confirm.",
                        "Deletes an Opportunity; requires --confirm.",
                    ),
                    &with_instance(vec![("opportunity_id", s("Opportunity-ID (Pflicht)"))]),
                    &["opportunity_id"],
                    &[API],
                    true,
                ),
                t(
                    "SALESFORCE_UPSERT_BY_EXTERNAL_ID",
                    ("Batch-Upsert nach External-ID", "Batch upsert by external id"),
                    (
                        "Legt an oder aktualisiert einen kleinen Stapel über die Composite-API, matched auf einem External-ID-Feld; braucht --confirm.",
                        "Upserts a small batch through the composite API, matching on an external id field; requires --confirm.",
                    ),
                    &with_instance(vec![
                        ("object", s("API-Name des Objekts (Pflicht)")),
                        (
                            "external_field",
                            s("API-Name des External-ID-Felds (Pflicht), z. B. My_External_Id__c"),
                        ),
                        (
                            "records",
                            json!({"type": "array", "items": {"type": "object"},
                                "description": "Datensätze; jeder Eintrag braucht das External-ID-Feld, z. B. [{\"My_External_Id__c\": \"A-1\", \"Name\": \"Acme\"}]."}),
                        ),
                    ]),
                    &["object", "external_field", "records"],
                    &[API],
                    true,
                ),
            ],
        };
        Self {
            spec,
            api_base: api_base.trim_end_matches('/').to_string(),
        }
    }

    /// Instance URL for this call: the `instance_url` argument, else the adapter's.
    fn base(&self, args: &Value) -> Result<String> {
        match opt_str(args, "instance_url") {
            Some(url) => validate_org_host(url),
            None => Ok(self.api_base.clone()),
        }
    }

    /// `GET {instance}/services/data/v56.0/query?q=…` (`querySalesforceApi`).
    fn query_request(&self, args: &Value, soql: &str) -> Result<HttpRequest> {
        Ok(HttpRequest::new(
            "GET",
            format!(
                "{}{V56}/query{}",
                self.base(args)?,
                query(&[("q", Some(soql.to_string()))])
            ),
        ))
    }
}

impl ToolkitAdapter for Salesforce {
    fn connection_fields(&self) -> Vec<ConnectionField> {
        vec![ConnectionField::new(
            "instance_url",
            ("Salesforce-Instanz-URL", "Salesforce instance URL"),
            (
                "Adresse deiner Salesforce-Org, z. B. https://acme.my.salesforce.com. Bei der Anmeldung über Salesforce wird sie automatisch übernommen.",
                "Your Salesforce org address, e.g. https://acme.my.salesforce.com. Filled in automatically when you sign in with Salesforce.",
            ),
            "https://acme.my.salesforce.com",
            true,
        )]
    }
    fn connection_api_base(&self, params: &std::collections::BTreeMap<String, String>) -> Result<Option<String>> {
        params.get("instance_url").map(|v| validate_org_host(v)).transpose()
    }
    fn default_api_base(&self) -> Option<String> {
        Some(self.api_base.clone())
    }
    fn derive_connection_params(
        &self,
        _credential: Option<&str>,
        token_extra: &std::collections::BTreeMap<String, String>,
    ) -> std::collections::BTreeMap<String, String> {
        token_extra
            .get("instance_url")
            .map(|v| [("instance_url".to_string(), v.clone())].into())
            .unwrap_or_default()
    }
    fn spec(&self) -> &ToolkitSpec {
        &self.spec
    }

    #[allow(clippy::too_many_lines)]
    fn build_request(&self, tool: &str, args: &Value) -> Result<HttpRequest> {
        let base = self.base(args)?;
        Ok(match tool {
            // AP: src/lib/action/run-sf-query.ts
            "SALESFORCE_RUN_SOQL_QUERY" => {
                return self.query_request(args, req_str(args, "query")?.trim())
            }
            // AP: src/lib/action/find-records-by-query.ts
            "SALESFORCE_FIND_RECORDS_BY_QUERY" => {
                let object = api_name(args, "object")?;
                let where_clause = req_str(args, "where_clause")?.trim();
                if where_clause.is_empty() {
                    bail!("`where_clause` cannot be empty");
                }
                return self.query_request(
                    args,
                    &format!("SELECT FIELDS(ALL) FROM {object} WHERE {where_clause} LIMIT 200"),
                );
            }
            // AP: src/lib/action/find-record.ts
            "SALESFORCE_FIND_RECORD" => {
                let object = api_name(args, "object")?;
                let field = api_name(args, "field")?;
                let value = escape_soql_value(req_str(args, "search_value")?.trim());
                return self.query_request(
                    args,
                    &format!("SELECT FIELDS(ALL) FROM {object} WHERE {field} = '{value}' LIMIT 200"),
                );
            }
            // AP: src/lib/action/find-child-records.ts
            "SALESFORCE_FIND_CHILD_RECORDS" => HttpRequest::new(
                "GET",
                format!(
                    "{base}{V56}/ui-api/records/{}/child-relationships/{}",
                    record_id(args, "parent_id")?,
                    path_segment(args, "child_relationship")?
                ),
            ),
            // AP: src/lib/action/run-report.ts
            "SALESFORCE_RUN_REPORT" => {
                let report_id = record_id(args, "report_id")?;
                let mut req = HttpRequest::new(
                    "POST",
                    format!("{base}{V56}/analytics/reports/{report_id}?includeDetails=true"),
                );
                if let Some(Value::Array(filters)) = args.get("filters") {
                    if !filters.is_empty() {
                        req = req.json(&json!({"reportMetadata": {"reportFilters": filters}}));
                    }
                }
                req
            }
            // AP: src/lib/action/get-record-attachments.ts (classic Attachment query)
            "SALESFORCE_LIST_CLASSIC_ATTACHMENTS" => {
                let id = record_id(args, "record_id")?;
                return self.query_request(
                    args,
                    &format!(
                        "SELECT Id, Name, BodyLength, ContentType FROM Attachment WHERE ParentId = '{id}'"
                    ),
                );
            }
            // AP: src/lib/action/get-record-attachments.ts (ContentDocumentLink query)
            "SALESFORCE_LIST_RECORD_FILES" => {
                let id = record_id(args, "record_id")?;
                return self.query_request(
                    args,
                    &format!(
                        "SELECT ContentDocument.Id, ContentDocument.Title FROM ContentDocumentLink WHERE LinkedEntityId = '{id}'"
                    ),
                );
            }
            // AP: src/lib/common/index.ts (object dropdown: getSalesforceObjects)
            "SALESFORCE_LIST_OBJECTS" => {
                HttpRequest::new("GET", format!("{base}{V56}/sobjects"))
            }
            // AP: src/lib/common/index.ts (field dropdown: getSalesforceFields)
            "SALESFORCE_DESCRIBE_OBJECT" => HttpRequest::new(
                "GET",
                format!("{base}{V56}/sobjects/{}/describe", api_name(args, "object")?),
            ),
            // AP: src/lib/common/index.ts (account dropdown)
            "SALESFORCE_LIST_ACCOUNTS" => {
                return self.query_request(args, "SELECT Id, Name FROM Account ORDER BY Name LIMIT 100")
            }
            // AP: src/lib/common/index.ts (contact dropdown)
            "SALESFORCE_LIST_CONTACTS" => {
                return self.query_request(
                    args,
                    "SELECT Id, Name, Email FROM Contact ORDER BY Name LIMIT 200",
                )
            }
            // AP: src/lib/common/index.ts (owner dropdown)
            "SALESFORCE_LIST_USERS" => {
                return self.query_request(
                    args,
                    "SELECT Id, Name FROM User WHERE IsActive = true ORDER BY Name",
                )
            }
            // AP: src/lib/common/index.ts (report dropdown, optional name search)
            "SALESFORCE_LIST_REPORTS" => {
                let mut soql = String::from("SELECT Id, Name FROM Report");
                if let Some(prefix) = opt_str(args, "name_prefix") {
                    soql.push_str(&format!(" WHERE Name LIKE '{}%'", escape_soql_like(prefix.trim())));
                }
                soql.push_str(" ORDER BY Name");
                return self.query_request(args, &soql);
            }
            // AP: src/lib/action/create-contact.ts
            "SALESFORCE_CREATE_CONTACT" => {
                let mut fields = Map::new();
                put_field(&mut fields, args, "last_name", "LastName");
                put_field(&mut fields, args, "first_name", "FirstName");
                put_field(&mut fields, args, "account_id", "AccountId");
                put_field(&mut fields, args, "email", "Email");
                put_field(&mut fields, args, "phone", "Phone");
                put_field(&mut fields, args, "title", "Title");
                if fields.get("LastName").is_none() {
                    bail!("missing argument `last_name`");
                }
                let body = sobject_body(args, fields.into_iter().collect())?;
                HttpRequest::new("POST", format!("{base}{V56}/sobjects/Contact")).json(&body)
            }
            // AP: src/lib/action/create-lead.ts
            "SALESFORCE_CREATE_LEAD" => {
                let mut fields = Map::new();
                put_field(&mut fields, args, "last_name", "LastName");
                put_field(&mut fields, args, "company", "Company");
                put_field(&mut fields, args, "first_name", "FirstName");
                put_field(&mut fields, args, "email", "Email");
                put_field(&mut fields, args, "phone", "Phone");
                put_field(&mut fields, args, "lead_source", "LeadSource");
                if fields.get("LastName").is_none() || fields.get("Company").is_none() {
                    bail!("missing argument `last_name` or `company`");
                }
                let body = sobject_body(args, fields.into_iter().collect())?;
                HttpRequest::new("POST", format!("{base}{V56}/sobjects/Lead")).json(&body)
            }
            // AP: src/lib/action/create-case.ts
            "SALESFORCE_CREATE_CASE" => {
                let mut fields = Map::new();
                put_field(&mut fields, args, "subject", "Subject");
                put_field(&mut fields, args, "description", "Description");
                put_field(&mut fields, args, "status", "Status");
                put_field(&mut fields, args, "priority", "Priority");
                put_field(&mut fields, args, "origin", "Origin");
                put_field(&mut fields, args, "account_id", "AccountId");
                put_field(&mut fields, args, "contact_id", "ContactId");
                if fields.get("Subject").is_none() {
                    bail!("missing argument `subject`");
                }
                let body = sobject_body(args, fields.into_iter().collect())?;
                HttpRequest::new("POST", format!("{base}{V56}/sobjects/Case")).json(&body)
            }
            // AP: src/lib/action/create-opportunity.ts
            "SALESFORCE_CREATE_OPPORTUNITY" => {
                let close_date = req_str(args, "close_date")?.trim();
                if !close_date.is_empty() && !looks_like_date(close_date) {
                    bail!("`close_date` must be a date in YYYY-MM-DD format");
                }
                let mut fields = Map::new();
                put_field(&mut fields, args, "name", "Name");
                put_field(&mut fields, args, "close_date", "CloseDate");
                put_field(&mut fields, args, "stage_name", "StageName");
                put_field(&mut fields, args, "account_id", "AccountId");
                if fields.get("Name").is_none()
                    || fields.get("CloseDate").is_none()
                    || fields.get("StageName").is_none()
                {
                    bail!("missing argument `name`, `close_date` or `stage_name`");
                }
                if let Some(amount) = args.get("amount").filter(|v| v.is_number()) {
                    fields.insert("Amount".into(), amount.clone());
                }
                let body = sobject_body(args, fields.into_iter().collect())?;
                HttpRequest::new("POST", format!("{base}{V56}/sobjects/Opportunity")).json(&body)
            }
            // AP: src/lib/action/create-task.ts
            "SALESFORCE_CREATE_TASK" => {
                let mut fields = Map::new();
                put_field(&mut fields, args, "subject", "Subject");
                put_field(&mut fields, args, "owner_id", "OwnerId");
                put_field(&mut fields, args, "status", "Status");
                put_field(&mut fields, args, "priority", "Priority");
                put_field(&mut fields, args, "description", "Description");
                put_field(&mut fields, args, "who_id", "WhoId");
                put_field(&mut fields, args, "what_id", "WhatId");
                if fields.get("Subject").is_none() {
                    bail!("missing argument `subject`");
                }
                let body = sobject_body(args, fields.into_iter().collect())?;
                HttpRequest::new("POST", format!("{base}{V56}/sobjects/Task")).json(&body)
            }
            // AP: src/lib/action/create-note.ts
            "SALESFORCE_CREATE_NOTE" => {
                let body = sobject_body(
                    args,
                    vec![
                        ("ParentId", json!(record_id(args, "parent_id")?)),
                        ("Title", json!(req_str(args, "title")?.trim())),
                        ("Body", json!(req_str(args, "body")?)),
                    ],
                )?;
                HttpRequest::new("POST", format!("{base}{V56}/sobjects/Note")).json(&body)
            }
            // AP: src/lib/action/add-contact-to-campaign.ts
            "SALESFORCE_ADD_CONTACT_TO_CAMPAIGN" => {
                let body = sobject_body(
                    args,
                    vec![
                        ("CampaignId", json!(record_id(args, "campaign_id")?)),
                        ("ContactId", json!(record_id(args, "contact_id")?)),
                        ("Status", json!(req_str(args, "status")?.trim())),
                    ],
                )?;
                HttpRequest::new("POST", format!("{base}{V56}/sobjects/CampaignMember")).json(&body)
            }
            // AP: src/lib/action/add-lead-to-campaign.ts
            "SALESFORCE_ADD_LEAD_TO_CAMPAIGN" => {
                let body = sobject_body(
                    args,
                    vec![
                        ("CampaignId", json!(record_id(args, "campaign_id")?)),
                        ("LeadId", json!(record_id(args, "lead_id")?)),
                        ("Status", json!(req_str(args, "status")?.trim())),
                    ],
                )?;
                HttpRequest::new("POST", format!("{base}{V56}/sobjects/CampaignMember")).json(&body)
            }
            // AP: src/lib/action/send-email.ts
            "SALESFORCE_SEND_EMAIL" => {
                let mut fields = Map::new();
                fields.insert("ToIds".into(), json!([record_id(args, "recipient_id")?]));
                fields.insert("Subject".into(), json!(req_str(args, "subject")?.trim()));
                fields.insert("HtmlBody".into(), json!(req_str(args, "body")?));
                fields.insert("Status".into(), json!("3"));
                put_field(&mut fields, args, "related_to_id", "RelatedToId");
                HttpRequest::new("POST", format!("{base}{V56}/sobjects/EmailMessage"))
                    .json(&Value::Object(fields))
            }
            // AP: src/lib/action/create-record.ts
            "SALESFORCE_CREATE_RECORD" => {
                let object = api_name(args, "object")?;
                let body = fields_object(args)?;
                HttpRequest::new("POST", format!("{base}{V56}/sobjects/{object}")).json(&body)
            }
            // AP: src/lib/action/update-record.ts
            "SALESFORCE_UPDATE_RECORD" => {
                let object = api_name(args, "object")?;
                let id = record_id(args, "record_id")?;
                let body = fields_object(args)?;
                require_fields(&body)?;
                HttpRequest::new("PATCH", format!("{base}{V56}/sobjects/{object}/{id}")).json(&body)
            }
            // AP: src/lib/action/update-contact.ts
            "SALESFORCE_UPDATE_CONTACT" => {
                let id = record_id(args, "contact_id")?;
                let mut fields = Map::new();
                put_field(&mut fields, args, "first_name", "FirstName");
                put_field(&mut fields, args, "last_name", "LastName");
                put_field(&mut fields, args, "account_id", "AccountId");
                put_field(&mut fields, args, "email", "Email");
                put_field(&mut fields, args, "phone", "Phone");
                put_field(&mut fields, args, "title", "Title");
                let body = sobject_body(args, fields.into_iter().collect())?;
                require_fields(&body)?;
                HttpRequest::new("PATCH", format!("{base}{V56}/sobjects/Contact/{id}")).json(&body)
            }
            // AP: src/lib/action/update-lead.ts
            "SALESFORCE_UPDATE_LEAD" => {
                let id = record_id(args, "lead_id")?;
                let mut fields = Map::new();
                put_field(&mut fields, args, "first_name", "FirstName");
                put_field(&mut fields, args, "last_name", "LastName");
                put_field(&mut fields, args, "company", "Company");
                put_field(&mut fields, args, "email", "Email");
                put_field(&mut fields, args, "phone", "Phone");
                put_field(&mut fields, args, "lead_source", "LeadSource");
                let body = sobject_body(args, fields.into_iter().collect())?;
                require_fields(&body)?;
                HttpRequest::new("PATCH", format!("{base}{V56}/sobjects/Lead/{id}")).json(&body)
            }
            // AP: src/lib/action/delete-record.ts
            "SALESFORCE_DELETE_RECORD" => HttpRequest::new(
                "DELETE",
                format!(
                    "{base}{V56}/sobjects/{}/{}",
                    api_name(args, "object")?,
                    record_id(args, "record_id")?
                ),
            ),
            // AP: src/lib/action/delete-opportunity.ts
            "SALESFORCE_DELETE_OPPORTUNITY" => HttpRequest::new(
                "DELETE",
                format!(
                    "{base}{V56}/sobjects/Opportunity/{}",
                    record_id(args, "opportunity_id")?
                ),
            ),
            // AP: src/lib/action/upsert-by-external-id.ts
            "SALESFORCE_UPSERT_BY_EXTERNAL_ID" => {
                let object = api_name(args, "object")?;
                let external = api_name(args, "external_field")?;
                let records = match args.get("records") {
                    Some(Value::Array(a)) if !a.is_empty() => a.clone(),
                    Some(Value::Array(_)) | None => {
                        bail!("`records` must contain at least one record")
                    }
                    Some(_) => bail!("`records` must be a JSON array"),
                };
                if records.len() > 200 {
                    bail!("at most 200 `records` per call");
                }
                if records
                    .iter()
                    .any(|r| !r.is_object() || r.get(&external).is_none())
                {
                    bail!("every record needs the external id field `{external}`");
                }
                HttpRequest::new(
                    "PATCH",
                    format!("{base}{V55}/composite/sobjects/{object}/{external}"),
                )
                .json(&json!({"allOrNone": false, "records": records}))
            }
            other => bail!("unknown Salesforce tool `{other}`"),
        })
    }

    fn parse_response(&self, tool: &str, response: &HttpResponse) -> Result<Value> {
        let v = response.json()?;
        Ok(match tool {
            // AP: src/lib/common/utils.ts (formatQueryResult)
            "SALESFORCE_RUN_SOQL_QUERY"
            | "SALESFORCE_FIND_RECORDS_BY_QUERY"
            | "SALESFORCE_FIND_RECORD"
            | "SALESFORCE_LIST_CLASSIC_ATTACHMENTS" => format_query_result(&v),
            // AP: src/lib/action/get-record-attachments.ts (files branch)
            "SALESFORCE_LIST_RECORD_FILES" => {
                let files: Vec<Value> = v["records"]
                    .as_array()
                    .cloned()
                    .unwrap_or_default()
                    .iter()
                    .map(|f| {
                        json!({
                            "Id": f["ContentDocument"]["Id"],
                            "Title": f["ContentDocument"]["Title"],
                        })
                    })
                    .collect();
                json!({"files": files.clone(), "count": files.len()})
            }
            "SALESFORCE_FIND_CHILD_RECORDS" => {
                let records = v["records"].as_array().cloned().unwrap_or_default();
                json!({"records": records.clone(), "count": records.len()})
            }
            "SALESFORCE_LIST_OBJECTS" => {
                let sobjects = v["sobjects"].as_array().cloned().unwrap_or_default();
                json!({"sobjects": sobjects.clone(), "count": sobjects.len()})
            }
            "SALESFORCE_DESCRIBE_OBJECT" => json!({
                "name": v["name"],
                "label": v["label"],
                "fields": v["fields"].as_array().cloned().unwrap_or_default(),
                "child_relationships": v["childRelationships"].as_array().cloned().unwrap_or_default(),
            }),
            "SALESFORCE_LIST_ACCOUNTS"
            | "SALESFORCE_LIST_CONTACTS"
            | "SALESFORCE_LIST_USERS"
            | "SALESFORCE_LIST_REPORTS" => record_list(&v),
            // AP: src/lib/common/report.ts (transformReportToRows)
            "SALESFORCE_RUN_REPORT" => report_rows(&v),
            // The delete actions return `{success: true}` after a 204.
            "SALESFORCE_DELETE_RECORD" | "SALESFORCE_DELETE_OPPORTUNITY" => {
                json!({"success": true, "id": v["id"]})
            }
            // sobject create/update and the composite upsert return the API envelope.
            _ => v,
        })
    }
}

fn looks_like_date(v: &str) -> bool {
    v.len() == 10
        && v.bytes().enumerate().all(|(i, b)| match i {
            4 | 7 => b == b'-',
            _ => b.is_ascii_digit(),
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sf() -> Salesforce {
        Salesforce::with_api_base("https://acme.my.salesforce.test")
    }

    #[test]
    fn queries_escape_and_encode() {
        let s = sf();
        let r = s
            .build_request(
                "SALESFORCE_RUN_SOQL_QUERY",
                &json!({"query": "SELECT Id FROM Account WHERE Name = 'O'Brien & Co'"}),
            )
            .unwrap();
        assert_eq!(r.method, "GET");
        assert!(r.url.starts_with("https://acme.my.salesforce.test/services/data/v56.0/query?q="));
        assert!(r.url.contains("O%27Brien+%26+Co"), "{}", r.url);

        let f = s
            .build_request(
                "SALESFORCE_FIND_RECORD",
                &json!({"object": "Contact", "field": "Email", "search_value": "a@b.de"}),
            )
            .unwrap();
        assert!(f.url.contains("SELECT+FIELDS%28ALL%29+FROM+Contact+WHERE+Email+%3D+%27a%40b.de%27+LIMIT+200"), "{}", f.url);

        assert!(s
            .build_request("SALESFORCE_FIND_RECORD", &json!({"object": "Contact/x", "field": "Email", "search_value": "a"}))
            .is_err());
        assert!(s
            .build_request("SALESFORCE_LIST_USERS", &json!({"instance_url": "http://evil.example"}))
            .is_err());
        assert!(s
            .build_request("SALESFORCE_LIST_USERS", &json!({"instance_url": "https://evil.example"}))
            .is_err());
        let per_call = s
            .build_request("SALESFORCE_LIST_USERS", &json!({"instance_url": "https://other.my.salesforce.com"}))
            .unwrap();
        assert!(per_call.url.starts_with("https://other.my.salesforce.com/"));
    }

    #[test]
    fn writes_use_ids_and_field_names() {
        let s = sf();
        let c = s
            .build_request(
                "SALESFORCE_CREATE_CONTACT",
                &json!({"last_name": "Lovelace", "email": "ada@acme.com",
                        "other_fields": {"Title": "CTO"}}),
            )
            .unwrap();
        assert_eq!(c.url, "https://acme.my.salesforce.test/services/data/v56.0/sobjects/Contact");
        let body: Value = serde_json::from_slice(c.body.as_deref().unwrap()).unwrap();
        assert_eq!(body, json!({"LastName": "Lovelace", "Email": "ada@acme.com", "Title": "CTO"}));
        assert!(s.build_request("SALESFORCE_CREATE_CONTACT", &json!({})).is_err());

        let u = s
            .build_request("SALESFORCE_UPDATE_RECORD", &json!({"object": "Account", "record_id": "0013000000abcdeAAA", "fields": {"Name": "Acme"}}))
            .unwrap();
        assert_eq!(u.method, "PATCH");
        assert_eq!(u.url, "https://acme.my.salesforce.test/services/data/v56.0/sobjects/Account/0013000000abcdeAAA");
        assert!(s
            .build_request("SALESFORCE_UPDATE_RECORD", &json!({"object": "Account", "record_id": "0013000000abcdeAAA", "fields": {}}))
            .is_err());
        assert!(s
            .build_request("SALESFORCE_DELETE_RECORD", &json!({"object": "Account", "record_id": "nope"}))
            .is_err());

        let up = s
            .build_request(
                "SALESFORCE_UPSERT_BY_EXTERNAL_ID",
                &json!({"object": "Account", "external_field": "Ext__c",
                        "records": [{"Ext__c": "A-1", "Name": "Acme"}]}),
            )
            .unwrap();
        assert_eq!(up.url, "https://acme.my.salesforce.test/services/data/v55.0/composite/sobjects/Account/Ext__c");
        assert!(s
            .build_request(
                "SALESFORCE_UPSERT_BY_EXTERNAL_ID",
                &json!({"object": "Account", "external_field": "Ext__c", "records": [{"Name": "Acme"}]})
            )
            .is_err());
    }

    #[test]
    fn report_rows_are_flattened() {
        let report = json!({
            "attributes": {"reportId": "00O3000000abcdeAAA", "reportName": "Pipeline"},
            "reportMetadata": {"name": "Pipeline", "detailColumns": ["AMOUNT", "STAGE"],
                               "groupingsDown": [{"name": "ACCOUNT_NAME"}, {"name": "REGION_NAME"}]},
            "reportExtendedMetadata": {"detailColumnInfo": {"AMOUNT": {"label": "Amount"},
                                                            "STAGE": {"label": "Stage"}},
                                       "groupingColumnInfo": {"ACCOUNT_NAME": {"label": "Account"},
                                                              "REGION_NAME": {"label": "Region"}}},
            "groupingsDown": {"groupings": [{"key": "0", "label": "Acme",
                "groupings": [{"key": "0_1", "label": "EMEA"}]}]},
            "factMap": {"0_1!T": {"rows": [{"dataCells": [{"label": "1200"}, {"label": "--"}]}]}}
        });
        let out = report_rows(&report);
        assert_eq!(out["report_name"], "Pipeline");
        assert_eq!(out["total_rows"], 1);
        assert_eq!(out["rows"][0]["Account"], "Acme");
        assert_eq!(out["rows"][0]["Region"], "EMEA");
        assert_eq!(out["rows"][0]["Amount"], "1200");
        assert_eq!(out["rows"][0]["Stage"], "");
        assert_eq!(out["columns"], json!(["Account", "Region", "Amount", "Stage"]));
    }
}
