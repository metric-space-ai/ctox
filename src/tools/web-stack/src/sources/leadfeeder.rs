//! `leadfeeder.com` — Tier C, DACH.
//!
//! Maintains the legacy API: <https://docs.leadfeeder.com/api/>.
//! Existing `LEADFEEDER_API_KEY` tokens use `Authorization: Token token=...`.
//! `LEADFEEDER_ACCOUNT_ID` must explicitly identify an account from `/accounts`;
//! there is no documented `me` alias. New legacy tokens are no longer issued.
//!
//! This is a bounded visitor-lead lookup, not general prospect/contact search:
//! one page (up to 100 leads) over the last 30 UTC calendar days, filtered
//! locally by exact company name (case-insensitive). A miss is not evidence
//! that the company is absent from all pages or historical data.
//! The legacy contract has neither a contacts endpoint nor company/person
//! email fields. Only documented lead fields are extracted; unknown resource
//! types and email-like administrative fields are not contact evidence.
//!
//! The separate v1 API uses `X-Api-Key` and different resources:
//! <https://docs.leadfeeder.com/api/public/authentication-354547m0>.
//! This adapter does not migrate tokens or claim v1 contact capabilities.

use std::time::Duration;

use anyhow::anyhow;
use serde_json::Value;

use super::{
    BrowserSourceRecipe, Confidence, Country, FieldEvidence, FieldKey, ShapedQuery, SourceCtx,
    SourceError, SourceHit, SourceModule, SourceReadResult, Tier,
};
use crate::runtime_config;

const API_BASE: &str = "https://api.leadfeeder.com";
const SECRET_NAME: &str = "LEADFEEDER_API_KEY";
const BROWSER_SECRET_NAME: &str = "LEADFEEDER_BROWSER_LOGIN";
const LOGIN_URL: &str = "https://app.leadfeeder.com/login";
const VERIFY_SELECTOR: &str =
    "[data-testid=\"account-menu\"], [data-testid*=\"account-switcher\"], input[placeholder*=\"Firma suchen\" i]";
const CREDENTIAL_SELECTOR: &str =
    "input[name=\"password\"], input#password, input[type=\"password\"]";
const CAPTURE_SCRIPT: &str = "leadfeeder.lead_capture.v1";
const ACCOUNT_CONFIG: &str = "LEADFEEDER_ACCOUNT_ID";
const TIMEOUT_MS: u64 = 12_000;
const MAX_HITS: usize = 8;
const USER_AGENT: &str = "ctox-web-stack/0.1 (+https://ctox.local)";

struct Leadfeeder;

impl SourceModule for Leadfeeder {
    fn id(&self) -> &'static str {
        "leadfeeder.com"
    }

    fn aliases(&self) -> &'static [&'static str] {
        &["leadfeeder", "lf"]
    }

    fn host_suffixes(&self) -> &'static [&'static str] {
        &["api.leadfeeder.com"]
    }

    fn tier(&self) -> Tier {
        Tier::C
    }

    fn countries(&self) -> &'static [Country] {
        &[Country::De, Country::At, Country::Ch]
    }

    fn authoritative_for(&self) -> &'static [FieldKey] {
        &[
            FieldKey::FirmaName,
            FieldKey::FirmaDomain,
            FieldKey::FirmaGeschaeftstaetigkeit,
            FieldKey::Mitarbeiter,
        ]
    }

    fn requires_credential(&self) -> Option<&'static str> {
        Some(SECRET_NAME)
    }

    fn browser_recipe(&self) -> Option<BrowserSourceRecipe> {
        Some(BrowserSourceRecipe {
            source_id: self.id(),
            login_url: LOGIN_URL.to_string(),
            allowed_domains: vec![
                "leadfeeder.com".to_string(),
                "app.leadfeeder.com".to_string(),
                "api.leadfeeder.com".to_string(),
            ],
            required_secret_name: Some(BROWSER_SECRET_NAME),
            verify_selector: Some(VERIFY_SELECTOR),
            credential_selector: Some(CREDENTIAL_SELECTOR),
            capture_script: Some(CAPTURE_SCRIPT),
        })
    }

    fn shape_query(&self, _query: &str, _ctx: &SourceCtx<'_>) -> Option<ShapedQuery> {
        // API-Quelle: keine Search-Engine-Variante.
        None
    }

    fn fetch_direct(
        &self,
        ctx: &SourceCtx<'_>,
        company: &str,
    ) -> Option<Result<Vec<SourceHit>, SourceError>> {
        // DACH-only. Andere Länder still überspringen.
        if matches!(ctx.country, Some(country) if !matches!(country, Country::De | Country::At | Country::Ch))
        {
            return None;
        }

        let trimmed = company.trim();
        if trimmed.is_empty() {
            return Some(Err(SourceError::NoMatch));
        }

        let token = match runtime_config::get(ctx.root, SECRET_NAME) {
            Some(t) => t,
            None => {
                return Some(Err(SourceError::CredentialMissing {
                    secret_name: SECRET_NAME,
                }));
            }
        };
        let account_id = runtime_config::get(ctx.root, ACCOUNT_CONFIG);
        let account_id = match validated_account_id(account_id.as_deref()) {
            Ok(id) => id,
            Err(err) => return Some(Err(err)),
        };

        let agent = build_agent();
        Some(perform_search(
            &agent, &token, account_id, trimmed, API_BASE,
        ))
    }

    fn extract_fields(&self, page: &SourceReadResult) -> Vec<(FieldKey, FieldEvidence)> {
        let value: Value = match serde_json::from_str(page.text.trim_start()) {
            Ok(v) => v,
            Err(_) => return Vec::new(),
        };
        extract_from_json(&value, &page.url)
    }
}

// ---------------------------------------------------------------------------
// HTTP
// ---------------------------------------------------------------------------

fn build_agent() -> ureq::Agent {
    ureq::AgentBuilder::new()
        .user_agent(USER_AGENT)
        .timeout(Duration::from_millis(TIMEOUT_MS))
        // SSRF guard: a resolved or redirected host must never reach an
        // internal/loopback/metadata address.
        .resolver(crate::egress::SsrfResolver::new(Vec::new()))
        .build()
}

fn auth_header(token: &str) -> String {
    format!("Token token={token}")
}

fn validated_account_id(value: Option<&str>) -> Result<&str, SourceError> {
    match value.map(str::trim) {
        Some(id) if !id.is_empty() && id.bytes().all(|b| b.is_ascii_digit()) => Ok(id),
        _ => Err(SourceError::Other(anyhow!(
            "configure LEADFEEDER_ACCOUNT_ID with an explicit numeric account ID from /accounts"
        ))),
    }
}

fn perform_search(
    agent: &ureq::Agent,
    token: &str,
    account_id: &str,
    company: &str,
    api_base: &str,
) -> Result<Vec<SourceHit>, SourceError> {
    let leads = fetch_leads(agent, token, account_id, api_base)?;
    let mut hits = leads_to_hits(&leads, account_id, company);

    if hits.is_empty() {
        return Err(SourceError::NoMatch);
    }
    hits.truncate(MAX_HITS);
    Ok(hits)
}

fn fetch_leads(
    agent: &ureq::Agent,
    token: &str,
    account_id: &str,
    api_base: &str,
) -> Result<Value, SourceError> {
    let url = format!("{api_base}/accounts/{account_id}/leads");
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_err(|err| SourceError::Other(anyhow!(err)))?;
    let end = chrono::DateTime::from_timestamp(now.as_secs() as i64, 0)
        .ok_or_else(|| SourceError::Other(anyhow!("invalid current UTC date")))?
        .date_naive();
    let start = end - chrono::Duration::days(29);
    let response = agent
        .get(&url)
        .set("Authorization", &auth_header(token))
        .set("accept", "application/json")
        .query("start_date", &start.to_string())
        .query("end_date", &end.to_string())
        .query("page[number]", "1")
        .query("page[size]", "100")
        .call();
    decode_json_response(response)
}

fn decode_json_response(
    response: Result<ureq::Response, ureq::Error>,
) -> Result<Value, SourceError> {
    let response = match response {
        Ok(r) => r,
        Err(ureq::Error::Status(status, resp)) => {
            return Err(classify_status(status, resp));
        }
        Err(err) => return Err(SourceError::Network(anyhow!(err))),
    };
    let text = response
        .into_string()
        .map_err(|err| SourceError::Network(anyhow!(err)))?;
    serde_json::from_str::<Value>(&text).map_err(|err| SourceError::ParseFailed {
        detail: err.to_string(),
    })
}

fn classify_status(status: u16, resp: ureq::Response) -> SourceError {
    match status {
        429 => {
            let retry = resp
                .header("retry-after")
                .and_then(|v| v.parse::<u64>().ok())
                .map(|secs| secs.saturating_mul(1_000));
            SourceError::RateLimited {
                retry_after_ms: retry,
            }
        }
        401 => SourceError::CredentialMissing {
            secret_name: SECRET_NAME,
        },
        403 => SourceError::Blocked {
            reason: format!("http {status}"),
        },
        404 => SourceError::NoMatch,
        _ => {
            let detail = resp
                .into_string()
                .unwrap_or_else(|_| format!("http {status}"));
            SourceError::Other(anyhow!("leadfeeder http {status}: {detail}"))
        }
    }
}

// ---------------------------------------------------------------------------
// Hit construction
// ---------------------------------------------------------------------------

fn lead_records(value: &Value) -> &[Value] {
    value
        .get("data")
        .and_then(Value::as_array)
        .map(|v| v.as_slice())
        .unwrap_or(&[])
}

fn leads_to_hits(value: &Value, account_id: &str, company: &str) -> Vec<SourceHit> {
    let mut hits = Vec::new();
    for record in lead_records(value) {
        if record.get("type").and_then(Value::as_str) == Some("leads") {
            if let Some(hit) = lead_to_hit(record, account_id) {
                if hit.title.to_lowercase() == company.trim().to_lowercase() {
                    hits.push(hit);
                }
            }
        }
    }
    hits
}

fn lead_to_hit(record: &Value, account_id: &str) -> Option<SourceHit> {
    let id = record.get("id").and_then(Value::as_str).unwrap_or("");
    if id.is_empty() {
        return None;
    }
    let attrs = record.get("attributes")?;
    let name = attrs
        .get("name")
        .and_then(Value::as_str)
        .unwrap_or("")
        .trim();
    if name.is_empty() {
        return None;
    }
    let domain = attrs
        .get("website_url")
        .and_then(Value::as_str)
        .map(domain_from_url)
        .unwrap_or_default();
    let industry = attrs.get("industry").and_then(Value::as_str).unwrap_or("");
    let snippet = [domain.as_str(), industry]
        .iter()
        .filter(|s| !s.is_empty())
        .copied()
        .collect::<Vec<_>>()
        .join(" · ");
    Some(SourceHit {
        title: name.to_string(),
        url: format!("{API_BASE}/accounts/{account_id}/leads/{id}"),
        snippet,
    })
}

// ---------------------------------------------------------------------------
// Field extraction
// ---------------------------------------------------------------------------

fn extract_from_json(value: &Value, source_url: &str) -> Vec<(FieldKey, FieldEvidence)> {
    // Beide Antwortformen — Listenantwort (`{"data":[...]}`) und
    // Einzelantwort (`{"data":{...}}`) — auf eine gemeinsame Schleife
    // reduzieren.
    let records: Vec<&Value> = match value.get("data") {
        Some(Value::Array(arr)) => arr.iter().collect(),
        Some(obj @ Value::Object(_)) => vec![obj],
        _ => Vec::new(),
    };

    let mut out = Vec::new();
    let url = source_url.to_string();
    for record in records {
        let record_type = record.get("type").and_then(Value::as_str).unwrap_or("");
        let attrs = match record.get("attributes") {
            Some(a) => a,
            None => continue,
        };
        if record_type == "leads" {
            extract_lead_fields(attrs, &url, &mut out);
        }
    }
    out
}

fn extract_lead_fields(attrs: &Value, url: &str, out: &mut Vec<(FieldKey, FieldEvidence)>) {
    for (key, field) in [
        ("name", FieldKey::FirmaName),
        ("industry", FieldKey::FirmaGeschaeftstaetigkeit),
    ] {
        if let Some(value) = attrs.get(key).and_then(Value::as_str) {
            push(out, field, value, url, Confidence::High);
        }
    }
    if let Some(count) = attrs.get("employee_count").and_then(Value::as_u64) {
        push(
            out,
            FieldKey::Mitarbeiter,
            &count.to_string(),
            url,
            Confidence::High,
        );
    }
    if let Some(website) = attrs.get("website_url").and_then(Value::as_str) {
        let domain = domain_from_url(website);
        if !domain.is_empty() {
            push(out, FieldKey::FirmaDomain, &domain, url, Confidence::High);
        }
    }
}

fn domain_from_url(raw: &str) -> String {
    // Leadfeeder liefert `website_url` mal mit Schema, mal ohne. Wir
    // ziehen die nackte registrable Domain heraus, ohne `www.`-Prefix und
    // ohne Trailing-Slash/Path. URL-Crate ist nicht überall verlässlich,
    // wenn das Schema fehlt — kleine, robuste Heuristik tut es hier.
    let s = raw.trim();
    if s.is_empty() {
        return String::new();
    }
    let after_scheme = match s.find("://") {
        Some(idx) => &s[idx + 3..],
        None => s,
    };
    let host_and_path = after_scheme.trim_start_matches('/');
    let host = host_and_path
        .split(|c: char| c == '/' || c == '?' || c == '#')
        .next()
        .unwrap_or("");
    let host = host.trim_start_matches("www.");
    host.trim().to_ascii_lowercase()
}

fn push(
    out: &mut Vec<(FieldKey, FieldEvidence)>,
    key: FieldKey,
    value: &str,
    url: &str,
    confidence: Confidence,
) {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return;
    }
    out.push((
        key,
        FieldEvidence {
            value: trimmed.to_string(),
            confidence,
            source_url: url.to_string(),
            note: None,
        },
    ));
}

// ---------------------------------------------------------------------------
// Registry hook
// ---------------------------------------------------------------------------

static MODULE: Leadfeeder = Leadfeeder;

pub fn module() -> &'static dyn SourceModule {
    &MODULE
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sources::{ResearchMode, SourceCtx};
    use std::path::Path;

    const LEADS_FIXTURE: &str =
        include_str!("../../fixtures/sources/leadfeeder/leads_wittenstein.json");
    const CONTACTS_FIXTURE: &str =
        include_str!("../../fixtures/sources/leadfeeder/contacts_wittenstein.json");

    fn dummy_page(text: &str, url: &str) -> SourceReadResult {
        SourceReadResult {
            url: url.to_string(),
            title: String::new(),
            summary: String::new(),
            text: text.to_string(),
            is_pdf: false,
            excerpts: Vec::new(),
            find_results: Vec::new(),
            raw_html: None,
        }
    }

    #[test]
    fn module_metadata() {
        let m = module();
        assert_eq!(m.id(), "leadfeeder.com");
        assert_eq!(m.aliases(), &["leadfeeder", "lf"]);
        assert!(matches!(m.tier(), Tier::C));
        assert_eq!(m.countries(), &[Country::De, Country::At, Country::Ch]);
        assert_eq!(m.requires_credential(), Some("LEADFEEDER_API_KEY"));
        let auth = m.authoritative_for();
        assert!(!auth.contains(&FieldKey::FirmaEmail));
        assert!(auth.contains(&FieldKey::FirmaDomain));
        assert!(auth.contains(&FieldKey::Mitarbeiter));
        assert!(!auth.contains(&FieldKey::PersonEmail));
    }

    #[test]
    fn shape_query_is_none_for_api_source() {
        let ctx = SourceCtx {
            root: Path::new("/tmp/ctox-test"),
            country: Some(Country::De),
            mode: ResearchMode::NewRecord,
        };
        assert!(module().shape_query("Wittenstein", &ctx).is_none());
    }

    #[test]
    fn fetch_direct_engages_for_each_dach_country() {
        // The Country enum currently only covers DACH, so there is no
        // negative variant we could feed in. We instead assert the
        // happy-path: for every DACH country, the module engages
        // (does not return None). With no credential, that engagement
        // surfaces as CredentialMissing — which is the correct contract
        // for the orchestrator to act on.
        for country in [Country::De, Country::At, Country::Ch] {
            let ctx = SourceCtx {
                root: Path::new("/tmp/ctox-nonexistent-leadfeeder"),
                country: Some(country),
                mode: ResearchMode::NewRecord,
            };
            let r = module().fetch_direct(&ctx, "Wittenstein SE");
            assert!(r.is_some(), "{country:?} must engage");
            assert!(matches!(
                r.unwrap(),
                Err(SourceError::CredentialMissing { .. })
            ));
        }
    }

    #[test]
    fn fetch_direct_missing_credential_returns_credential_missing() {
        // Point root at a directory that definitely has no runtime config
        // SQLite — runtime_config::get returns None, fetch_direct must
        // map this to CredentialMissing for the orchestrator.
        let ctx = SourceCtx {
            root: Path::new("/tmp/ctox-nonexistent-leadfeeder"),
            country: Some(Country::De),
            mode: ResearchMode::NewRecord,
        };
        let result = module()
            .fetch_direct(&ctx, "Wittenstein SE")
            .expect("DACH engages");
        match result {
            Err(SourceError::CredentialMissing { secret_name }) => {
                assert_eq!(secret_name, "LEADFEEDER_API_KEY");
            }
            other => panic!("expected CredentialMissing, got: {other:?}"),
        }
    }

    #[test]
    fn fetch_direct_empty_company_is_no_match() {
        let ctx = SourceCtx {
            root: Path::new("/tmp/ctox-nonexistent-leadfeeder"),
            country: Some(Country::De),
            mode: ResearchMode::NewRecord,
        };
        let result = module().fetch_direct(&ctx, "   ").expect("DACH engages");
        assert!(matches!(result, Err(SourceError::NoMatch)));
    }

    #[test]
    fn lead_fixture_yields_domain_but_not_undocumented_email() {
        let page = dummy_page(
            LEADS_FIXTURE,
            "https://api.leadfeeder.com/accounts/6002/leads",
        );
        let fields = module().extract_fields(&page);

        assert!(!fields
            .iter()
            .any(|(k, _)| matches!(k, FieldKey::FirmaEmail | FieldKey::PersonEmail)));

        let firma_domain = fields
            .iter()
            .find(|(k, _)| matches!(k, FieldKey::FirmaDomain))
            .expect("firma_domain present");
        assert_eq!(firma_domain.1.value, "wittenstein.de");
        assert!(matches!(firma_domain.1.confidence, Confidence::High));
    }

    #[test]
    fn unsupported_contact_fixture_is_not_person_evidence() {
        let page = dummy_page(
            CONTACTS_FIXTURE,
            "https://api.leadfeeder.com/accounts/6002/contacts",
        );
        let fields = module().extract_fields(&page);

        assert!(fields.is_empty());
    }

    #[test]
    fn extract_fields_returns_empty_when_text_is_not_json() {
        let page = dummy_page("<html>not json</html>", "https://example.invalid");
        assert!(module().extract_fields(&page).is_empty());
    }

    #[test]
    fn extract_fields_filters_garbage_emails() {
        let body = r#"{
            "data": [{
                "id": "1",
                "type": "leads",
                "attributes": {
                    "name": "Acme",
                    "email": "unknown",
                    "website_url": "https://example.com/path"
                }
            }]
        }"#;
        let page = dummy_page(body, "https://api.leadfeeder.com/accounts/6002/leads");
        let fields = module().extract_fields(&page);
        assert!(
            !fields
                .iter()
                .any(|(k, _)| matches!(k, FieldKey::FirmaEmail)),
            "non-email string must be rejected"
        );
        let domain = fields
            .iter()
            .find(|(k, _)| matches!(k, FieldKey::FirmaDomain))
            .expect("domain stripped from URL");
        assert_eq!(domain.1.value, "example.com");
    }

    #[test]
    fn domain_from_url_strips_scheme_and_www() {
        assert_eq!(
            domain_from_url("https://www.Wittenstein.de/de"),
            "wittenstein.de"
        );
        assert_eq!(domain_from_url("http://example.com"), "example.com");
        assert_eq!(domain_from_url("example.com/foo"), "example.com");
        assert_eq!(domain_from_url(""), "");
    }

    #[test]
    fn account_selection_requires_explicit_numeric_id() {
        for value in [
            None,
            Some(""),
            Some("  "),
            Some("me"),
            Some("1/contacts"),
            Some("1?x=y"),
        ] {
            assert!(matches!(
                validated_account_id(value),
                Err(SourceError::Other(_))
            ));
        }
        assert_eq!(validated_account_id(Some(" 6002 ")).unwrap(), "6002");
    }

    #[test]
    fn undocumented_resource_types_never_become_company_or_person_evidence() {
        for kind in ["contacts", "people", "companies", "unknown", ""] {
            let value = serde_json::json!({"data": {
                "type": kind,
                "attributes": {"name": "Example", "email": "person@example.invalid",
                               "website_url": "https://example.invalid"}
            }});
            assert!(extract_from_json(&value, "https://example.invalid").is_empty());
        }
    }

    #[test]
    fn documented_lead_fields_exclude_administrative_emails() {
        let value = serde_json::json!({"data": {"type": "leads", "attributes": {
            "name": "Example", "industry": "Software", "employee_count": 25,
            "website_url": "https://www.example.invalid",
            "assignee": "owner@example.invalid", "emailed_to": "recipient@example.invalid"
        }}});
        let fields = extract_from_json(&value, "https://example.invalid");
        assert_eq!(fields.len(), 4);
        for (key, expected) in [
            (FieldKey::FirmaName, "Example"),
            (FieldKey::FirmaGeschaeftstaetigkeit, "Software"),
            (FieldKey::Mitarbeiter, "25"),
            (FieldKey::FirmaDomain, "example.invalid"),
        ] {
            assert!(fields
                .iter()
                .any(|(k, ev)| *k == key && ev.value == expected));
        }
        assert!(fields
            .iter()
            .all(|(k, _)| module().authoritative_for().contains(k)));
    }

    #[test]
    fn local_company_filter_rejects_unrelated_leads_and_contact_records() {
        let value = serde_json::json!({"data": [
            {"id": "a", "type": "leads", "attributes": {"name": "Example AG"}},
            {"id": "b", "type": "leads", "attributes": {"name": "Other AG"}},
            {"id": "c", "type": "contacts", "attributes": {"name": "Example AG"}},
            {"type": "leads", "attributes": {"name": "Example AG"}}
        ]});
        let hits = leads_to_hits(&value, "6002", " example ag ");
        assert_eq!(hits.len(), 1);
        assert!(hits[0].url.ends_with("/leads/a"));
        assert!(leads_to_hits(&value, "6002", "Missing AG").is_empty());
    }

    #[test]
    fn successful_leads_survive_missing_contacts_endpoint() {
        use std::io::{Read, Write};
        use std::net::TcpListener;
        use std::time::Instant;

        // Loopback fixture only. Any unsupported follow-up returns 404, which
        // previously discarded a valid lead response. No provider access.
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        listener.set_nonblocking(true).unwrap();
        let base = format!("http://{}", listener.local_addr().unwrap());
        let server = std::thread::spawn(move || {
            let deadline = Instant::now() + Duration::from_secs(2);
            let mut requests = Vec::new();
            while Instant::now() < deadline {
                let (mut stream, _) = match listener.accept() {
                    Ok(pair) => pair,
                    Err(err) if err.kind() == std::io::ErrorKind::WouldBlock => {
                        std::thread::sleep(Duration::from_millis(5));
                        continue;
                    }
                    Err(err) => panic!("fixture accept: {err}"),
                };
                stream
                    .set_read_timeout(Some(Duration::from_secs(1)))
                    .unwrap();
                stream
                    .set_write_timeout(Some(Duration::from_secs(1)))
                    .unwrap();
                let mut bytes = Vec::new();
                let mut byte = [0];
                while !bytes.ends_with(b"\r\n\r\n") && bytes.len() < 8192 {
                    if stream.read(&mut byte).unwrap() == 0 {
                        break;
                    }
                    bytes.push(byte[0]);
                }
                let request = String::from_utf8(bytes).unwrap();
                let (status, body) = if request.starts_with("GET /accounts/6002/leads?") {
                    ("200 OK", LEADS_FIXTURE)
                } else {
                    ("404 Not Found", r#"{"errors":[]}"#)
                };
                write!(stream, "HTTP/1.1 {status}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len()).unwrap();
                requests.push(request);
            }
            requests
        });
        let agent = ureq::AgentBuilder::new()
            .timeout(Duration::from_secs(1))
            .build();
        let result = perform_search(&agent, "fixture-only", "6002", "Wittenstein SE", &base);
        let requests = server.join().unwrap();
        let hits = result.expect("valid leads must survive an absent contacts endpoint");
        assert!(!hits.is_empty());
        assert_eq!(requests.len(), 1, "legacy lookup must not request contacts");
        let request = &requests[0];
        assert!(request
            .to_ascii_lowercase()
            .contains("authorization: token token=fixture-only"));
        assert!(!request.to_ascii_lowercase().contains("x-api-key"));
        let target = request.split_whitespace().nth(1).unwrap();
        let url = url::Url::parse(&format!("{base}{target}")).unwrap();
        let query: std::collections::HashMap<_, _> = url.query_pairs().into_owned().collect();
        assert_eq!(query.len(), 4);
        assert_eq!(query["page[number]"], "1");
        assert_eq!(query["page[size]"], "100");
        let start = chrono::NaiveDate::parse_from_str(&query["start_date"], "%Y-%m-%d").unwrap();
        let end = chrono::NaiveDate::parse_from_str(&query["end_date"], "%Y-%m-%d").unwrap();
        assert_eq!((end - start).num_days(), 29);
    }

    #[test]
    fn failed_leads_still_report_provider_errors() {
        assert!(matches!(
            classify_status(401, ureq::Response::new(401, "Unauthorized", "").unwrap()),
            SourceError::CredentialMissing { .. }
        ));
        assert!(matches!(
            classify_status(403, ureq::Response::new(403, "Forbidden", "").unwrap()),
            SourceError::Blocked { .. }
        ));
        assert!(matches!(
            classify_status(404, ureq::Response::new(404, "Not Found", "").unwrap()),
            SourceError::NoMatch
        ));
        assert!(matches!(
            classify_status(
                429,
                ureq::Response::new(429, "Too Many Requests", "").unwrap()
            ),
            SourceError::RateLimited { .. }
        ));
    }

    #[test]
    #[ignore = "live network; run with: cargo test -p ctox-web-stack -- --ignored sources::leadfeeder"]
    fn live_credential_missing_or_smoke() {
        // The repo has no Leadfeeder token by default; the live test
        // therefore *documents* the credential-missing path. If an
        // operator wires LEADFEEDER_API_KEY into runtime_env_kv, the test
        // becomes a real smoke check against the API.
        let ctx = SourceCtx {
            root: Path::new("/tmp/ctox-leadfeeder-live"),
            country: Some(Country::De),
            mode: ResearchMode::NewRecord,
        };
        let result = module()
            .fetch_direct(&ctx, "Wittenstein SE")
            .expect("DACH context engages");
        match result {
            Err(SourceError::CredentialMissing { secret_name }) => {
                assert_eq!(secret_name, "LEADFEEDER_API_KEY");
            }
            Ok(hits) => {
                assert!(
                    !hits.is_empty(),
                    "live response must contain at least one hit"
                );
            }
            Err(other) => panic!("unexpected live error: {other:?}"),
        }
    }
}
