//! Freshdesk toolkit (REST API v2). Action set modelled on the Activepieces
//! piece `freshdesk` (MIT), files under `src/lib/actions/`.
//!
//! Auth (replicated from `src/index.ts`): the piece uses a CustomAuth with the
//! props `base_url` + `access_token` and every action sends the API token
//! verbatim as the `Authorization` header value — no `Bearer`/`Token token=`
//! prefix. The token is created under profile settings → "View API key".
//! Freshdesk API keys carry no scopes, so the `scopes` of every tool stay empty.
//!
//! Base URL: the account URL (`https://{subdomain}.freshdesk.com`) is a
//! required auth prop of the piece, so it is injected with
//! [`Freshdesk::with_subdomain`] / [`Freshdesk::with_api_base`]; the piece
//! itself strips a trailing slash (`base_url.replace(/\/$/, '')`). The
//! `Default` instance only carries a documented placeholder; `builtin()` has no
//! per-account wiring, so any caller that knows the subdomain must register its
//! own adapter.
//!
//! The upstream piece ships exactly the five actions below (plus
//! `createCustomApiCallAction`, which is not ported). Every request arm carries
//! the `// AP:` comment naming the Activepieces file the method, path and query
//! fields were taken from.
use anyhow::{bail, Result};
use serde_json::{json, Value};

use super::common::*;
use super::{path_segment, ToolkitAdapter};
use crate::http::{HttpRequest, HttpResponse};
use crate::model::*;

pub const SLUG: &str = "freshdesk";

/// Placeholder account of [`Freshdesk::default`] — not a real account.
const DEFAULT_SUBDOMAIN: &str = "example";

/// `per_page` cap documented on the piece's contacts action ("if specified
/// maximum is 100").
const MAX_PER_PAGE: u64 = 100;

/// Filter types and contact states of `get_contacts`.
const FILTER_TYPES: &[&str] = &["email", "mobile", "phone", "company_id", "updated_since"];
const FILTER_STATES: &[&str] = &["blocked", "deleted", "unverified", "verified"];

// ---------------------------------------------------------------- schema helpers

fn choice(desc: &str, options: &[&str]) -> Value {
    json!({"type": "string", "description": desc, "enum": options})
}

/// Numeric Freshdesk record id (`ticketid` / `contactid` travel in the path).
fn numeric_id<'a>(args: &'a Value, key: &str) -> Result<&'a str> {
    let id = path_segment(args, key)?;
    if !id.bytes().all(|b| b.is_ascii_digit()) {
        bail!("`{key}` must be a numeric Freshdesk ID, got \"{id}\".");
    }
    Ok(id)
}

/// One status token of the ticket search: `status:2` (or a bare `2`) .. `status:5`.
fn status_code(token: &str) -> Result<u64> {
    let raw = token.trim().strip_prefix("status:").unwrap_or(token.trim());
    let code = raw.parse::<u64>().map_err(|_| {
        anyhow::anyhow!(
            "`status_filter` must contain status:2 (open), status:3 (pending), \
             status:4 (resolved) or status:5 (closed), got \"{token}\"."
        )
    })?;
    if !(2..=5).contains(&code) {
        bail!(
            "`status_filter` must contain status:2 (open), status:3 (pending), \
             status:4 (resolved) or status:5 (closed), got \"{token}\"."
        );
    }
    Ok(code)
}

/// `query` value of the ticket search: `"status:2 OR status:3"`, quoted and
/// joined like the piece does (commas inside a token also become ` OR `).
fn search_query(args: &Value) -> Result<String> {
    let mut codes: Vec<String> = Vec::new();
    for token in str_vec(args, "status_filter") {
        for part in token.split(',') {
            if part.trim().is_empty() {
                continue;
            }
            codes.push(format!("status:{}", status_code(part)?));
        }
    }
    if codes.is_empty() {
        bail!("`status_filter` needs at least one status");
    }
    Ok(format!("\"{}\"", codes.join(" OR ")))
}

/// Status number -> human string, as the piece maps it for search results.
fn ticket_status_word(status: &Value) -> Value {
    match status.as_i64() {
        Some(2) => json!("Open"),
        Some(3) => json!("Pending"),
        Some(4) => json!("Resolved"),
        Some(5) => json!("Closed"),
        _ => json!("Unknown"),
    }
}

// ---------------------------------------------------------------- adapter

pub struct Freshdesk {
    spec: ToolkitSpec,
    api_base: String,
}

impl Default for Freshdesk {
    fn default() -> Self {
        Self::with_subdomain(DEFAULT_SUBDOMAIN)
    }
}

impl Freshdesk {
    /// `Freshdesk::with_subdomain("acme")` -> `https://acme.freshdesk.com`.
    pub fn with_subdomain(subdomain: &str) -> Self {
        let sub = subdomain.trim().trim_end_matches('/');
        Self::with_api_base(&format!("https://{sub}.freshdesk.com"))
    }

    /// Account base URL as the piece's `base_url` auth prop (trailing slash dropped).
    pub fn with_api_base(api_base: &str) -> Self {
        let t = |slug, title, desc, props: Props<'_>, req: &[&str], scopes: &[&str], m| {
            tool(SLUG, slug, title, desc, props, req, scopes, m)
        };
        let spec = ToolkitSpec {
            slug: SLUG.into(),
            name: "Freshdesk".into(),
            description: LocalizedText::new(
                "Freshdesk-Support: Tickets und Kontakte lesen und nach Status filtern.",
                "Freshdesk support: read tickets and contacts and filter them by status.",
            ),
            auth_schemes: vec![AuthScheme::ApiKey {
                // The piece sends the raw token: `Authorization: <access_token>`.
                header_name: "authorization".into(),
                prefix: String::new(),
            }],
            tools: vec![
                t(
                    "FRESHDESK_LIST_TICKETS",
                    ("Tickets auflisten", "List tickets"),
                    (
                        "Liest die letzten Tickets des Freshdesk-Accounts (ungefilterter Standard-Endpunkt).",
                        "Reads the Freshdesk account's recent tickets (unfiltered default endpoint).",
                    ),
                    &[],
                    &[],
                    &[],
                    false,
                ),
                t(
                    "FRESHDESK_GET_TICKET_STATUS",
                    ("Ticket-Status lesen", "Get ticket status"),
                    (
                        "Liest Workflow-Status und Zuweisung eines Tickets: ticket_status, assigned_status, assigned_id.",
                        "Reads a ticket's workflow status and assignment: ticket_status, assigned_status, assigned_id.",
                    ),
                    &[("ticketid", s("Ticket-ID, z. B. 1234"))],
                    &["ticketid"],
                    &[],
                    false,
                ),
                t(
                    "FRESHDESK_GET_CONTACT",
                    ("Kontakt lesen", "Get contact"),
                    (
                        "Liest die vollständigen Kontaktdaten zu einer Kontakt-ID.",
                        "Reads the full contact details for a contact ID.",
                    ),
                    &[("contactid", s("Kontakt-ID, z. B. 1234"))],
                    &["contactid"],
                    &[],
                    false,
                ),
                t(
                    "FRESHDESK_LIST_CONTACTS",
                    ("Kontakte auflisten", "List contacts"),
                    (
                        "Listet Kontakte auf, optional gefiltert nach E-Mail, Mobil, Telefon, Firmen-ID oder Aktualisierungszeit sowie nach Kontaktstatus.",
                        "Lists contacts, optionally filtered by email, mobile, phone, company ID or update time and by contact state.",
                    ),
                    &[
                        (
                            "filter_type",
                            choice(
                                "Optionaler Filter: nur zusammen mit `filter_value`",
                                FILTER_TYPES,
                            ),
                        ),
                        ("filter_value", s("Wert zum Filter; ignoriert ohne `filter_type`")),
                        (
                            "filter_status",
                            choice("Optional nach Kontaktstatus filtern", FILTER_STATES),
                        ),
                        (
                            "per_page",
                            int(
                                "Ergebnisse pro Seite; 0 = Serverstandard, sonst maximal 100",
                                0,
                                MAX_PER_PAGE,
                                0,
                            ),
                        ),
                    ],
                    &["per_page"],
                    &[],
                    false,
                ),
                t(
                    "FRESHDESK_LIST_TICKETS_BY_STATUS",
                    (
                        "Tickets nach Status auflisten",
                        "List tickets by status",
                    ),
                    (
                        "Durchsucht Tickets, die einem oder mehreren Status entsprechen (ODER-verknüpft), sortiert nach Antragsteller-ID.",
                        "Searches tickets matching one or more statuses (OR-combined), sorted by requester ID.",
                    ),
                    &[(
                        "status_filter",
                        json!({"type": "array",
                            "items": {"type": "string",
                                "enum": ["status:2", "status:3", "status:4", "status:5"]},
                            "description": "Ein oder mehrere Status: 2 offen, 3 ausstehend, 4 gelöst, 5 geschlossen / \
                                One or more statuses: 2 open, 3 pending, 4 resolved, 5 closed"}),
                    )],
                    &["status_filter"],
                    &[],
                    false,
                ),
            ],
        };
        Self {
            spec,
            api_base: api_base.trim_end_matches('/').to_string(),
        }
    }

    fn url(&self, path: &str) -> String {
        format!("{}{}", self.api_base, path)
    }
}

impl ToolkitAdapter for Freshdesk {
    fn connection_fields(&self) -> Vec<ConnectionField> {
        vec![ConnectionField::new(
            "subdomain",
            ("Freshdesk-Subdomain", "Freshdesk subdomain"),
            (
                "Der Teil vor .freshdesk.com, z. B. „acme“ für https://acme.freshdesk.com.",
                "The part before .freshdesk.com, e.g. \"acme\" for https://acme.freshdesk.com.",
            ),
            "acme",
            true,
        )]
    }
    fn connection_api_base(&self, params: &std::collections::BTreeMap<String, String>) -> Result<Option<String>> {
        params
            .get("subdomain")
            .map(|v| Ok(format!("https://{}.freshdesk.com", host_label(v, "freshdesk.com", "subdomain")?)))
            .transpose()
    }
    fn default_api_base(&self) -> Option<String> {
        Some(self.api_base.clone())
    }
    fn spec(&self) -> &ToolkitSpec {
        &self.spec
    }

    fn build_request(&self, tool: &str, args: &Value) -> Result<HttpRequest> {
        Ok(match tool {
            // AP: src/lib/actions/get-tickets.ts
            "FRESHDESK_LIST_TICKETS" => HttpRequest::new("GET", self.url("/api/v2/tickets/")),
            // AP: src/lib/actions/get-ticket-status.ts
            "FRESHDESK_GET_TICKET_STATUS" => {
                let id = numeric_id(args, "ticketid")?;
                HttpRequest::new("GET", self.url(&format!("/api/v2/tickets/{id}")))
            }
            // AP: src/lib/actions/get-contact-from-id.ts
            "FRESHDESK_GET_CONTACT" => {
                let id = numeric_id(args, "contactid")?;
                HttpRequest::new("GET", self.url(&format!("/api/v2/contacts/{id}")))
            }
            // AP: src/lib/actions/get-contacts.ts
            "FRESHDESK_LIST_CONTACTS" => {
                let mut pairs: Vec<(&str, Option<String>)> = Vec::new();
                match opt_str(args, "filter_type") {
                    // "Provide value if previous option selected!"
                    Some(kind) => {
                        if !FILTER_TYPES.contains(&kind) {
                            bail!("`filter_type` must be one of {}", FILTER_TYPES.join(", "));
                        }
                        pairs.push((kind, Some(req_str(args, "filter_value")?.to_string())));
                    }
                    None if opt_str(args, "filter_value").is_some() => {
                        bail!("`filter_value` needs a `filter_type`");
                    }
                    None => {}
                }
                if let Some(state) = opt_str(args, "filter_status") {
                    if !FILTER_STATES.contains(&state) {
                        bail!("`filter_status` must be one of {}", FILTER_STATES.join(", "));
                    }
                    pairs.push(("state", Some(state.to_string())));
                }
                // `per_page != 0` is the piece's switch for a non-default page.
                let per_page = opt_u64(args, "per_page", 0, 0, MAX_PER_PAGE);
                if per_page != 0 {
                    pairs.push(("per_page", Some(per_page.to_string())));
                }
                HttpRequest::new(
                    "GET",
                    self.url(&format!("/api/v2/contacts/{}", query(&pairs))),
                )
            }
            // AP: src/lib/actions/get-all-tickets-by-status.ts
            "FRESHDESK_LIST_TICKETS_BY_STATUS" => HttpRequest::new(
                "GET",
                self.url(&format!(
                    "/api/v2/search/tickets/{}",
                    query(&[("query", Some(search_query(args)?))])
                )),
            ),
            other => bail!("unknown Freshdesk tool `{other}`"),
        })
    }

    fn parse_response(&self, tool: &str, response: &HttpResponse) -> Result<Value> {
        let v = response.json()?;
        Ok(match tool {
            // Both list endpoints return the body array as-is (`response.body`).
            "FRESHDESK_LIST_TICKETS" | "FRESHDESK_LIST_CONTACTS" => {
                let items = v.as_array().cloned().unwrap_or_default();
                json!({"items": items, "count": items.len()})
            }
            // Reduces to the three keys the piece returns.
            "FRESHDESK_GET_TICKET_STATUS" => {
                let assigned_id = v["responder_id"].clone();
                json!({
                    "ticket_status": match v["status"].as_i64() {
                        Some(2) => json!("OPEN"),
                        Some(3) => json!("PENDING"),
                        Some(4) => json!("RESOLVED"),
                        Some(5) => json!("CLOSED"),
                        // "it shouldn't happen" — the piece passes the raw value through.
                        _ => v["status"].clone(),
                    },
                    "assigned_status": if assigned_id.is_null() {
                        json!("NOTASSIGNED")
                    } else {
                        json!("ASSIGNED")
                    },
                    "assigned_id": assigned_id,
                })
            }
            // Returns response.body untouched (`get_contact_from_id`).
            "FRESHDESK_GET_CONTACT" => v,
            // Search results, sorted by requester_id, status number mapped to a word.
            "FRESHDESK_LIST_TICKETS_BY_STATUS" => {
                let mut results = v["results"].as_array().cloned().unwrap_or_default();
                results.sort_by_key(|t| t["requester_id"].as_i64().unwrap_or(0));
                let items: Vec<Value> = results
                    .iter()
                    .map(|t| {
                        json!({
                            "id": t["id"],
                            "requester_id": t["requester_id"],
                            "responder_id": t["responder_id"],
                            "company_id": t["company_id"],
                            "status": ticket_status_word(&t["status"]),
                            "subject": t["subject"],
                            "created_at": t["created_at"],
                            "updated_at": t["updated_at"],
                            "description_text": t["description_text"],
                            "description": t["description"],
                        })
                    })
                    .collect();
                json!({"items": items, "count": items.len(), "total": v["total"]})
            }
            other => bail!("unknown Freshdesk tool `{other}`"),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fd() -> Freshdesk {
        Freshdesk::with_api_base("https://acme.freshdesk.com")
    }

    #[test]
    fn paths_follow_the_piece() {
        let f = fd();
        assert_eq!(
            f.build_request("FRESHDESK_LIST_TICKETS", &json!({})).unwrap().url,
            "https://acme.freshdesk.com/api/v2/tickets/"
        );
        assert_eq!(
            f.build_request("FRESHDESK_GET_TICKET_STATUS", &json!({"ticketid": "12"}))
                .unwrap()
                .url,
            "https://acme.freshdesk.com/api/v2/tickets/12"
        );
        assert_eq!(
            f.build_request("FRESHDESK_GET_CONTACT", &json!({"contactid": "7"}))
                .unwrap()
                .url,
            "https://acme.freshdesk.com/api/v2/contacts/7"
        );
        // A trailing slash on the base URL is dropped like the piece does.
        assert_eq!(
            Freshdesk::with_api_base("https://acme.freshdesk.com/")
                .build_request("FRESHDESK_LIST_TICKETS", &json!({}))
                .unwrap()
                .url,
            "https://acme.freshdesk.com/api/v2/tickets/"
        );
    }

    #[test]
    fn query_values_are_encoded() {
        let f = fd();
        let r = f
            .build_request(
                "FRESHDESK_LIST_CONTACTS",
                &json!({"filter_type": "email", "filter_value": "a b@x.de",
                        "filter_status": "verified", "per_page": 30}),
            )
            .unwrap();
        assert_eq!(
            r.url,
            "https://acme.freshdesk.com/api/v2/contacts/?email=a+b%40x.de&state=verified&per_page=30"
        );

        // No filter -> no `?` at all; per_page 0 stays unset (piece default).
        let r = f
            .build_request("FRESHDESK_LIST_CONTACTS", &json!({"per_page": 0}))
            .unwrap();
        assert_eq!(r.url, "https://acme.freshdesk.com/api/v2/contacts/");

        let r = f
            .build_request(
                "FRESHDESK_LIST_TICKETS_BY_STATUS",
                &json!({"status_filter": ["status:2", "status:5"]}),
            )
            .unwrap();
        assert_eq!(
            r.url,
            "https://acme.freshdesk.com/api/v2/search/tickets/?query=%22status%3A2+OR+status%3A5%22"
        );
    }

    #[test]
    fn validation_rejects_bad_input() {
        let f = fd();
        assert!(f
            .build_request("FRESHDESK_GET_TICKET_STATUS", &json!({"ticketid": "../x"}))
            .is_err());
        assert!(f
            .build_request("FRESHDESK_GET_CONTACT", &json!({"contactid": "12a"}))
            .is_err());
        assert!(f
            .build_request(
                "FRESHDESK_LIST_CONTACTS",
                &json!({"filter_type": "company", "filter_value": "1", "per_page": 0})
            )
            .is_err());
        assert!(f
            .build_request(
                "FRESHDESK_LIST_CONTACTS",
                &json!({"filter_type": "email", "per_page": 0})
            )
            .is_err());
        assert!(f
            .build_request(
                "FRESHDESK_LIST_CONTACTS",
                &json!({"filter_value": "x", "per_page": 0})
            )
            .is_err());
        assert!(f
            .build_request("FRESHDESK_LIST_TICKETS_BY_STATUS", &json!({"status_filter": []}))
            .is_err());
        assert!(f
            .build_request(
                "FRESHDESK_LIST_TICKETS_BY_STATUS",
                &json!({"status_filter": ["status:6"]})
            )
            .is_err());
        // Bare numbers and comma-joined tokens are accepted like the piece's replace.
        let r = f
            .build_request(
                "FRESHDESK_LIST_TICKETS_BY_STATUS",
                &json!({"status_filter": ["3", "status:2,status:4"]}),
            )
            .unwrap();
        assert!(r.url.ends_with("query=%22status%3A3+OR+status%3A2+OR+status%3A4%22"), "{}", r.url);
    }

    #[test]
    fn parse_follows_the_run_code() {
        let f = fd();
        let resp = |body: Value| HttpResponse {
            status: 200,
            body: serde_json::to_vec(&body).unwrap(),
            session_id: None,
        };

        let out = f
            .parse_response(
                "FRESHDESK_GET_TICKET_STATUS",
                &resp(json!({"id": 12, "status": 3, "responder_id": 450012345})),
            )
            .unwrap();
        assert_eq!(out["ticket_status"], json!("PENDING"));
        assert_eq!(out["assigned_status"], json!("ASSIGNED"));
        assert_eq!(out["assigned_id"], json!(450012345));

        let out = f
            .parse_response(
                "FRESHDESK_GET_TICKET_STATUS",
                &resp(json!({"id": 13, "status": 7, "responder_id": null})),
            )
            .unwrap();
        assert_eq!(out["ticket_status"], json!(7));
        assert_eq!(out["assigned_status"], json!("NOTASSIGNED"));
        assert_eq!(out["assigned_id"], Value::Null);

        let out = f
            .parse_response(
                "FRESHDESK_LIST_TICKETS_BY_STATUS",
                &resp(json!({"total": 2, "results": [
                    {"id": 2, "requester_id": 20, "status": 5, "subject": "Closed one"},
                    {"id": 1, "requester_id": 10, "status": 2, "subject": "Open one"}
                ]})),
            )
            .unwrap();
        assert_eq!(out["count"], json!(2));
        assert_eq!(out.pointer("/items/0/id"), Some(&json!(1)), "sorted by requester_id");
        assert_eq!(out.pointer("/items/0/status"), Some(&json!("Open")));
        assert_eq!(out.pointer("/items/1/status"), Some(&json!("Closed")));
    }
}
