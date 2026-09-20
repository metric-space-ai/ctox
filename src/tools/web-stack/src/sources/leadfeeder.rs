//! `leadfeeder.com` — Tier C, DACH.
//!
//! Leadfeeder (Dealfront Leadfeeder) maps B2B website visitors to companies
//! and contacts. In the CTOX web stack it is the DACH matrix source for
//! `person_email` and a secondary source for `firma_email` / `firma_domain`.
//!
//! ## Supported current API
//!
//! Official current docs (not the legacy Token API):
//!
//! * Authentication: `https://docs.leadfeeder.com/api/public/authentication-354547m0`
//!   — send `X-Api-Key` on every request. Do not send a current key as
//!   `Authorization: Token token=...`.
//! * Getting started / account selection:
//!   `https://docs.leadfeeder.com/api/public/getting-started-363633m0`
//!   — most endpoints require an explicit `account_id`. List accounts with
//!   `GET /v1/accounts` (`https://docs.leadfeeder.com/api/public/list-accounts-4008950e0`).
//! * Identity (no credits): `POST /v1/companies/match`
//!   (`https://docs.leadfeeder.com/api/public/match-companies-4008956e0`)
//!   and `POST /v1/companies/search`
//!   (`https://docs.leadfeeder.com/api/public/search-companies-4008955e0`).
//!
//! This module never calls credit-consuming retrieve/detail endpoints
//! (`GET /v1/companies/{id}`, `GET /v1/contacts?ids=`), never creates
//! enrichment or find-contact jobs, and never writes CRM/list/tag state.
//! `person_email` therefore remains a remaining live dependency: contact
//! summaries from search/match do not include deep emails unless a later
//! credit-consuming retrieve is explicitly authorized elsewhere.
//!
//! ## Credentials
//!
//! Values come from the runtime config/secret store (`runtime_env_kv`), not
//! process environment:
//!
//! * `LEADFEEDER_API_KEY` — current API key, sent only as `X-Api-Key`.
//! * `LEADFEEDER_LEGACY_API_TOKEN` — legacy Token API only.
//! * `LEADFEEDER_AUTH_SCHEME` — `api_key` or `legacy`. Required when both
//!   secrets are present. Never inferred from key shape. Invalid values are
//!   rejected without echoing the raw config string.
//! * `LEADFEEDER_ACCOUNT_ID` — explicit account. The unsupported alias `me`
//!   is rejected. If unset, `GET /v1/accounts` may use a single authorized
//!   account; multiple accounts fail with `account_selection_required`.
//!
//! Legacy compatibility (`https://docs.leadfeeder.com/api/`): existing Token
//! integrations keep working only when the legacy scheme and token are
//! selected explicitly. New keys are not sent through Token auth.
//!
//! Browser capture remains available via `LEADFEEDER_BROWSER_LOGIN` when the
//! scrape adapter deliberately selects authenticated browser mode.

use std::time::Duration;

use anyhow::anyhow;
use serde_json::json;
use serde_json::Value;

use super::{
    BrowserSourceRecipe, Confidence, Country, FieldEvidence, FieldKey, ShapedQuery, SourceCtx,
    SourceError, SourceHit, SourceModule, SourceReadResult, Tier,
};
use crate::credentials::{resolve_credential, CredentialReference, CredentialResolver};
use crate::runtime_config;

const API_BASE: &str = "https://api.leadfeeder.com";
const SECRET_NAME: &str = "LEADFEEDER_API_KEY";
const LEGACY_SECRET_NAME: &str = "LEADFEEDER_LEGACY_API_TOKEN";
const AUTH_SCHEME_KEY: &str = "LEADFEEDER_AUTH_SCHEME";
const ACCOUNT_ID_KEY: &str = "LEADFEEDER_ACCOUNT_ID";
const BROWSER_SECRET_NAME: &str = "LEADFEEDER_BROWSER_LOGIN";
const LOGIN_URL: &str = "https://app.leadfeeder.com/login";
const VERIFY_SELECTOR: &str =
    "[data-testid=\"account-menu\"], [data-testid*=\"account-switcher\"], input[placeholder*=\"Firma suchen\" i]";
const CREDENTIAL_SELECTOR: &str =
    "input[name=\"password\"], input#password, input[type=\"password\"]";
const CAPTURE_SCRIPT: &str = "leadfeeder.lead_capture.v1";
const TIMEOUT_MS: u64 = 12_000;
const MAX_HITS: usize = 8;
const MIN_MATCH_SCORE: f64 = 0.75;
const USER_AGENT: &str = "ctox-web-stack/0.1 (+https://ctox.local)";
const HIT_FIELDS_PREFIX: &str = "leadfeeder_fields:";
const HIT_FIELDS_MAX_SNIPPET: usize = 2048;
const HIT_FIELDS_MAX_KEYS: usize = 8;
const HIT_FIELDS_MAX_VALUE: usize = 256;
const HIT_FIELD_KEYS: &[&str] = &["id", "country", "domain", "employees", "industry"];
const SINGLE_LEGAL_SUFFIXES: &[&str] = &[
    "ag", "gmbh", "se", "kg", "kgaa", "ohg", "ug", "ltd", "inc", "sa", "sarl", "nv", "bv",
];

struct Leadfeeder;

#[derive(Clone)]
enum AuthScheme {
    ApiKey(String),
    LegacyToken(String),
}

impl std::fmt::Debug for AuthScheme {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            AuthScheme::ApiKey(_) => f.debug_tuple("ApiKey").field(&"<redacted>").finish(),
            AuthScheme::LegacyToken(_) => {
                f.debug_tuple("LegacyToken").field(&"<redacted>").finish()
            }
        }
    }
}

impl AuthScheme {
    fn is_legacy(&self) -> bool {
        matches!(self, AuthScheme::LegacyToken(_))
    }
}

struct Transport {
    base: String,
    allow_hosts: Vec<String>,
}

impl Transport {
    fn production() -> Self {
        Self {
            base: API_BASE.to_string(),
            allow_hosts: Vec::new(),
        }
    }

    fn agent(&self) -> ureq::Agent {
        ureq::AgentBuilder::new()
            .user_agent(USER_AGENT)
            .timeout(Duration::from_millis(TIMEOUT_MS))
            .resolver(crate::egress::SsrfResolver::new(self.allow_hosts.clone()))
            .build()
    }
}

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
            FieldKey::FirmaEmail,
            FieldKey::FirmaDomain,
            FieldKey::FirmaGeschaeftstaetigkeit,
            FieldKey::FirmaHomepageFactSheet,
            FieldKey::Mitarbeiter,
            FieldKey::PersonEmail,
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
        None
    }

    fn fetch_direct(
        &self,
        ctx: &SourceCtx<'_>,
        company: &str,
    ) -> Option<Result<Vec<SourceHit>, SourceError>> {
        self.fetch_direct_with_resolver(ctx, company, None)
    }

    fn fetch_direct_with_resolver(
        &self,
        ctx: &SourceCtx<'_>,
        company: &str,
        resolver: Option<&dyn CredentialResolver>,
    ) -> Option<Result<Vec<SourceHit>, SourceError>> {
        fetch_direct_with(ctx, company, &Transport::production(), resolver)
    }

    fn extract_fields(&self, page: &SourceReadResult) -> Vec<(FieldKey, FieldEvidence)> {
        let value: Value = match serde_json::from_str(page.text.trim_start()) {
            Ok(v) => v,
            Err(_) => return Vec::new(),
        };
        extract_from_json(&value, &page.url)
    }

    fn extract_from_hits(
        &self,
        ctx: &SourceCtx<'_>,
        company: &str,
        hits: &[SourceHit],
    ) -> Vec<(FieldKey, FieldEvidence)> {
        let requested_country = ctx.country.map(Country::as_iso);
        let eligible: Vec<&SourceHit> = hits
            .iter()
            .filter(|hit| company_identity_matches(company, &hit.title, None))
            .filter(|hit| hit_country_allowed(&hit.snippet, requested_country))
            .filter(|hit| hit_company_id(hit).is_some())
            .collect();
        if distinct_hit_ids(&eligible).len() > 1 {
            return Vec::new();
        }
        let mut out = Vec::new();
        for hit in eligible {
            push(
                &mut out,
                FieldKey::FirmaName,
                &hit.title,
                &hit.url,
                Confidence::High,
            );
            extract_snippet_fields(&hit.snippet, &hit.url, &mut out);
        }
        out
    }
}

fn fetch_direct_with(
    ctx: &SourceCtx<'_>,
    company: &str,
    transport: &Transport,
    resolver: Option<&dyn CredentialResolver>,
) -> Option<Result<Vec<SourceHit>, SourceError>> {
    if matches!(ctx.country, Some(country) if !matches!(country, Country::De | Country::At | Country::Ch))
    {
        return None;
    }

    let trimmed = company.trim();
    if trimmed.is_empty() {
        return Some(Err(SourceError::NoMatch));
    }

    let auth = match resolve_auth(ctx, resolver) {
        Ok(auth) => auth,
        Err(err) => return Some(Err(err)),
    };
    let account_id = match resolve_account_id(ctx, transport, &auth) {
        Ok(id) => id,
        Err(err) => return Some(Err(err)),
    };
    let iso = ctx.country.map(|c| c.as_iso());
    Some(if auth.is_legacy() {
        perform_legacy_search(transport, &auth, &account_id, trimmed)
    } else {
        perform_current_search(transport, &auth, &account_id, trimmed, iso)
    })
}

fn credential_reference(name: &'static str) -> CredentialReference {
    CredentialReference {
        scope: "credentials".to_string(),
        name: name.to_string(),
    }
}

fn resolve_secret(
    resolver: Option<&dyn CredentialResolver>,
    name: &'static str,
) -> Result<Option<String>, SourceError> {
    match resolve_credential(resolver, &credential_reference(name)) {
        Ok(Some(value)) => {
            let trimmed = value.expose_secret().trim();
            if trimmed.is_empty() {
                Err(SourceError::Other(anyhow!(
                    "credential_invalid: empty stored value"
                )))
            } else {
                Ok(Some(trimmed.to_string()))
            }
        }
        Ok(None) => Ok(None),
        Err(error) => Err(SourceError::Other(error.into())),
    }
}

fn resolve_auth(
    ctx: &SourceCtx<'_>,
    resolver: Option<&dyn CredentialResolver>,
) -> Result<AuthScheme, SourceError> {
    let scheme = nonempty_config(ctx.root, AUTH_SCHEME_KEY).map(|value| value.to_ascii_lowercase());
    match scheme.as_deref() {
        Some("api_key") | Some("x-api-key") | Some("current") => {
            resolve_secret(resolver, SECRET_NAME)?
                .map(AuthScheme::ApiKey)
                .ok_or(SourceError::CredentialMissing {
                    secret_name: SECRET_NAME,
                })
        }
        Some("legacy") | Some("legacy_token") | Some("token") => {
            resolve_secret(resolver, LEGACY_SECRET_NAME)?
                .map(AuthScheme::LegacyToken)
                .ok_or(SourceError::CredentialMissing {
                    secret_name: LEGACY_SECRET_NAME,
                })
        }
        Some(_) => Err(SourceError::Other(anyhow!(
            "invalid {AUTH_SCHEME_KEY}; expected api_key or legacy"
        ))),
        None => {
            let api_key = resolve_secret(resolver, SECRET_NAME)?;
            let legacy_token = resolve_secret(resolver, LEGACY_SECRET_NAME)?;
            match (api_key, legacy_token) {
                (Some(key), None) => Ok(AuthScheme::ApiKey(key)),
                (None, Some(token)) => Ok(AuthScheme::LegacyToken(token)),
                (Some(_), Some(_)) => Err(SourceError::Other(anyhow!(
                    "ambiguous Leadfeeder credentials; set {AUTH_SCHEME_KEY} to api_key or legacy"
                ))),
                (None, None) => Err(SourceError::CredentialMissing {
                    secret_name: SECRET_NAME,
                }),
            }
        }
    }
}

fn resolve_account_id(
    ctx: &SourceCtx<'_>,
    transport: &Transport,
    auth: &AuthScheme,
) -> Result<String, SourceError> {
    if let Some(configured) = nonempty_config(ctx.root, ACCOUNT_ID_KEY) {
        return validate_account_id(&configured);
    }
    let ids = list_account_ids(transport, auth)?;
    match ids.as_slice() {
        [] => Err(SourceError::Other(anyhow!(
            "entitlement: no authorized Leadfeeder account for this API key"
        ))),
        [one] => Ok(one.clone()),
        _ => Err(SourceError::Other(anyhow!(
            "account_selection_required: {} authorized accounts; set {ACCOUNT_ID_KEY}",
            ids.len()
        ))),
    }
}

fn validate_account_id(raw: &str) -> Result<String, SourceError> {
    let trimmed = raw.trim();
    if trimmed.is_empty() || trimmed.eq_ignore_ascii_case("me") {
        return Err(SourceError::Other(anyhow!(
            "{ACCOUNT_ID_KEY} must be an explicit authorized account id, not empty or 'me'"
        )));
    }
    Ok(trimmed.to_string())
}

fn nonempty_config(root: &std::path::Path, key: &str) -> Option<String> {
    runtime_config::get(root, key).and_then(|value| {
        let trimmed = value.trim();
        if trimmed.is_empty() {
            None
        } else {
            Some(trimmed.to_string())
        }
    })
}

fn apply_auth<'a>(request: ureq::Request, auth: &'a AuthScheme) -> ureq::Request {
    let request = request
        .set("accept", "application/json")
        .set("user-agent", USER_AGENT);
    match auth {
        AuthScheme::ApiKey(key) => request.set("X-Api-Key", key),
        AuthScheme::LegacyToken(token) => {
            request.set("Authorization", &format!("Token token={token}"))
        }
    }
}

fn list_account_ids(transport: &Transport, auth: &AuthScheme) -> Result<Vec<String>, SourceError> {
    let path = if auth.is_legacy() {
        format!("{}/accounts", transport.base)
    } else {
        format!("{}/v1/accounts", transport.base)
    };
    let agent = transport.agent();
    let response = apply_auth(agent.get(&path), auth).call();
    let value = decode_json_response(response, auth)?;
    Ok(flatten_records(&value)
        .into_iter()
        .filter_map(|record| record.get("id").and_then(Value::as_str).map(str::to_string))
        .filter(|id| validate_account_id(id).is_ok())
        .collect())
}

fn perform_current_search(
    transport: &Transport,
    auth: &AuthScheme,
    account_id: &str,
    company: &str,
    country_iso: Option<&str>,
) -> Result<Vec<SourceHit>, SourceError> {
    let agent = transport.agent();
    let matches = current_match(&agent, transport, auth, account_id, company, country_iso)?;
    let mut hits = current_records_to_hits(&matches, account_id, company, true, country_iso);
    if hits.is_empty() {
        let search = current_search(&agent, transport, auth, account_id, company, country_iso)?;
        hits = current_records_to_hits(&search, account_id, company, false, country_iso);
    }
    if hits.is_empty() {
        return Err(SourceError::NoMatch);
    }
    hits = unique_identity_hits(hits)?;
    hits.truncate(MAX_HITS);
    Ok(hits)
}

fn current_match(
    agent: &ureq::Agent,
    transport: &Transport,
    auth: &AuthScheme,
    account_id: &str,
    company: &str,
    country_iso: Option<&str>,
) -> Result<Value, SourceError> {
    let url = format!("{}/v1/companies/match", transport.base);
    let mut company_obj = json!({ "company_name": company });
    if let Some(code) = country_iso {
        company_obj["country_code"] = json!(code);
    }
    let body = json!({ "companies": [company_obj] }).to_string();
    let response = apply_auth(agent.post(&url), auth)
        .set("content-type", "application/json")
        .query("account_id", account_id)
        .query("max_results_per_company", "5")
        .send_string(&body);
    decode_json_response(response, auth)
}

fn current_search(
    agent: &ureq::Agent,
    transport: &Transport,
    auth: &AuthScheme,
    account_id: &str,
    company: &str,
    country_iso: Option<&str>,
) -> Result<Value, SourceError> {
    let url = format!("{}/v1/companies/search", transport.base);
    let mut body = json!({
        "search_terms": [company],
    });
    if let Some(code) = country_iso {
        body["locations"] = json!([{ "country_code": code }]);
    }
    let response = apply_auth(agent.post(&url), auth)
        .set("content-type", "application/json")
        .query("account_id", account_id)
        .query("page[size]", &MAX_HITS.to_string())
        .send_string(&body.to_string());
    decode_json_response(response, auth)
}

fn perform_legacy_search(
    transport: &Transport,
    auth: &AuthScheme,
    account_id: &str,
    company: &str,
) -> Result<Vec<SourceHit>, SourceError> {
    let agent = transport.agent();
    let leads = fetch_legacy(
        &agent,
        transport,
        auth,
        &format!("/accounts/{account_id}/leads"),
        &[("company_name", company)],
    )?;
    let contacts = match fetch_legacy(
        &agent,
        transport,
        auth,
        &format!("/accounts/{account_id}/contacts"),
        &[("search", company)],
    ) {
        Ok(value) => value,
        Err(SourceError::Blocked { .. }) => Value::Null,
        Err(other) => return Err(other),
    };
    let mut hits = leads_to_hits(&leads, account_id, company);
    hits.extend(contacts_to_hits(&contacts, account_id));
    if hits.is_empty() {
        return Err(SourceError::NoMatch);
    }
    hits.truncate(MAX_HITS);
    Ok(hits)
}

fn fetch_legacy(
    agent: &ureq::Agent,
    transport: &Transport,
    auth: &AuthScheme,
    path: &str,
    query: &[(&str, &str)],
) -> Result<Value, SourceError> {
    let url = format!("{}{path}", transport.base);
    let mut request = apply_auth(agent.get(&url), auth);
    for (key, value) in query {
        request = request.query(key, value);
    }
    decode_json_response(request.call(), auth)
}

fn decode_json_response(
    response: Result<ureq::Response, ureq::Error>,
    auth: &AuthScheme,
) -> Result<Value, SourceError> {
    let response = match response {
        Ok(r) => r,
        Err(ureq::Error::Status(status, resp)) => {
            return Err(classify_status(status, resp, auth));
        }
        Err(err) => return Err(SourceError::Network(anyhow!(err))),
    };
    let text = response
        .into_string()
        .map_err(|err| SourceError::Network(anyhow!(err)))?;
    serde_json::from_str::<Value>(&text).map_err(|_| SourceError::ParseFailed {
        detail: "invalid json".to_string(),
    })
}

fn classify_status(status: u16, resp: ureq::Response, auth: &AuthScheme) -> SourceError {
    let retry = resp
        .header("retry-after")
        .and_then(|v| v.parse::<u64>().ok())
        .map(|secs| secs.saturating_mul(1_000));
    let body = resp.into_string().unwrap_or_default();
    let code = allowlisted_error_code(&body);
    match (status, code) {
        (429, _) => SourceError::RateLimited {
            retry_after_ms: retry,
        },
        (401, _) | (403, Some("missing_token" | "invalid_api_key" | "invalid_token")) => {
            SourceError::Other(anyhow!("authentication_rejected: http {status}"))
        }
        (403, Some(code @ ("insufficient_scope" | "forbidden"))) => {
            SourceError::Other(anyhow!("entitlement: {code}"))
        }
        (403, _) if auth.is_legacy() => SourceError::Blocked {
            reason: format!("http {status}"),
        },
        (403, _) => SourceError::Other(anyhow!("entitlement: http {status}")),
        (404, _) => SourceError::NoMatch,
        _ => SourceError::Other(anyhow!("leadfeeder http {status}")),
    }
}

const ALLOWED_ERROR_CODES: &[&str] = &[
    "missing_token",
    "invalid_api_key",
    "invalid_token",
    "insufficient_scope",
    "forbidden",
];

fn allowlisted_error_code(body: &str) -> Option<&'static str> {
    let parsed = error_code(body)?;
    ALLOWED_ERROR_CODES
        .iter()
        .copied()
        .find(|code| *code == parsed)
}

fn error_code(body: &str) -> Option<String> {
    let value: Value = serde_json::from_str(body).ok()?;
    if let Some(code) = value.get("code").and_then(Value::as_str) {
        return Some(code.to_string());
    }
    value
        .get("errors")
        .and_then(Value::as_array)
        .and_then(|arr| arr.first())
        .and_then(|err| err.get("code").or_else(|| err.get("title")))
        .and_then(Value::as_str)
        .map(str::to_string)
}

fn flatten_records(value: &Value) -> Vec<&Value> {
    match value.get("data") {
        Some(Value::Array(arr)) => {
            let mut out = Vec::new();
            for item in arr {
                match item {
                    Value::Array(inner) => out.extend(inner.iter()),
                    other => out.push(other),
                }
            }
            out
        }
        Some(obj @ Value::Object(_)) => vec![obj],
        _ => Vec::new(),
    }
}

fn current_records_to_hits(
    value: &Value,
    account_id: &str,
    company: &str,
    require_score: bool,
    requested_country: Option<&str>,
) -> Vec<SourceHit> {
    let mut hits = Vec::new();
    for record in flatten_records(value) {
        let score = record
            .pointer("/attributes/match_score")
            .and_then(Value::as_f64);
        if require_score && score.unwrap_or(0.0) < MIN_MATCH_SCORE {
            continue;
        }
        let Some(summary) = company_summary(record) else {
            continue;
        };
        let Some(hit) = summary_to_hit(summary, account_id, company, score, requested_country)
        else {
            continue;
        };
        hits.push(hit);
    }
    hits
}

fn company_summary(record: &Value) -> Option<&Value> {
    let record_type = record.get("type").and_then(Value::as_str).unwrap_or("");
    if matches!(
        record_type,
        "company_summary" | "companies" | "company" | "leads" | "lead"
    ) {
        return Some(record);
    }
    match record.pointer("/relationships/company_summary") {
        Some(summary) if summary.get("attributes").is_some() => Some(summary),
        _ => None,
    }
}

fn summary_to_hit(
    summary: &Value,
    account_id: &str,
    company: &str,
    score: Option<f64>,
    requested_country: Option<&str>,
) -> Option<SourceHit> {
    let id = nonempty_provider_id(summary.get("id").and_then(Value::as_str))?;
    let attrs = summary.get("attributes")?;
    let name = attrs
        .get("name")
        .and_then(Value::as_str)
        .unwrap_or("")
        .trim();
    if name.is_empty() || !company_identity_matches(company, name, score) {
        return None;
    }
    let response_country = attrs
        .pointer("/address/country_code")
        .and_then(Value::as_str);
    if !country_code_matches(requested_country, response_country) {
        return None;
    }
    let domain = attrs
        .get("url")
        .or_else(|| attrs.get("website_url"))
        .and_then(Value::as_str)
        .map(domain_from_url)
        .unwrap_or_default();
    let employees = employee_display_value(attrs).unwrap_or_default();
    let industry = first_industry_name(attrs).unwrap_or("");
    let snippet = encode_hit_fields(id, response_country, &domain, &employees, industry);
    Some(SourceHit {
        title: name.to_string(),
        url: format!("{API_BASE}/v1/companies/{id}?account_id={account_id}"),
        snippet,
    })
}

fn leads_to_hits(value: &Value, account_id: &str, company: &str) -> Vec<SourceHit> {
    flatten_records(value)
        .into_iter()
        .filter_map(|record| lead_to_hit(record, account_id, company))
        .collect()
}

fn contacts_to_hits(value: &Value, account_id: &str) -> Vec<SourceHit> {
    flatten_records(value)
        .into_iter()
        .filter_map(|record| contact_to_hit(record, account_id))
        .collect()
}

fn lead_to_hit(record: &Value, account_id: &str, company: &str) -> Option<SourceHit> {
    let id = nonempty_provider_id(record.get("id").and_then(Value::as_str))?;
    let attrs = record.get("attributes")?;
    let name = attrs
        .get("name")
        .and_then(Value::as_str)
        .unwrap_or("")
        .trim();
    if name.is_empty() || !company_identity_matches(company, name, None) {
        return None;
    }
    let domain = attrs
        .get("website_url")
        .and_then(Value::as_str)
        .map(domain_from_url)
        .unwrap_or_default();
    let industry = attrs.get("industry").and_then(Value::as_str).unwrap_or("");
    let snippet = encode_hit_fields(id, None, &domain, "", industry);
    Some(SourceHit {
        title: name.to_string(),
        url: format!("{API_BASE}/accounts/{account_id}/leads/{id}"),
        snippet,
    })
}

fn contact_to_hit(record: &Value, account_id: &str) -> Option<SourceHit> {
    let id = record.get("id").and_then(Value::as_str).unwrap_or("");
    let attrs = record.get("attributes")?;
    let name = attrs
        .get("name")
        .and_then(Value::as_str)
        .unwrap_or("")
        .trim();
    if name.is_empty() {
        return None;
    }
    let title = attrs
        .get("title")
        .and_then(Value::as_str)
        .unwrap_or("")
        .trim();
    let email = attrs
        .get("email")
        .and_then(Value::as_str)
        .unwrap_or("")
        .trim();
    let snippet_parts: Vec<&str> = [title, email]
        .into_iter()
        .filter(|part| !part.is_empty())
        .collect();
    Some(SourceHit {
        title: name.to_string(),
        url: format!("{API_BASE}/accounts/{account_id}/contacts/{id}"),
        snippet: snippet_parts.join(" · "),
    })
}

fn extract_from_json(value: &Value, source_url: &str) -> Vec<(FieldKey, FieldEvidence)> {
    let mut out = Vec::new();
    let url = source_url.to_string();
    for record in flatten_records(value) {
        let record_type = record.get("type").and_then(Value::as_str).unwrap_or("");
        if let Some(summary) = company_summary(record) {
            if let Some(attrs) = summary.get("attributes") {
                extract_company_fields(attrs, &url, &mut out);
            }
        }
        let attrs = match record.get("attributes") {
            Some(attrs) => attrs,
            None => continue,
        };
        match record_type {
            "leads" | "lead" => extract_lead_fields(attrs, &url, &mut out),
            "contacts" | "contact" | "people" | "person" => {
                extract_contact_fields(attrs, &url, &mut out)
            }
            "company_summary" | "companies" | "company" | "company_match" => {}
            _ => {
                extract_lead_fields(attrs, &url, &mut out);
                extract_contact_fields(attrs, &url, &mut out);
            }
        }
    }
    out
}

fn extract_company_fields(attrs: &Value, url: &str, out: &mut Vec<(FieldKey, FieldEvidence)>) {
    if let Some(name) = attrs.get("name").and_then(Value::as_str) {
        push(out, FieldKey::FirmaName, name, url, Confidence::High);
    }
    if let Some(website) = attrs
        .get("url")
        .or_else(|| attrs.get("website_url"))
        .and_then(Value::as_str)
    {
        let domain = domain_from_url(website);
        if !domain.is_empty() {
            push(out, FieldKey::FirmaDomain, &domain, url, Confidence::High);
            push(
                out,
                FieldKey::FirmaHomepageFactSheet,
                website,
                url,
                Confidence::High,
            );
        }
    } else if let Some(domain) = attrs.get("domain").and_then(Value::as_str) {
        let clean = domain.trim().trim_start_matches("www.");
        if !clean.is_empty() {
            push(out, FieldKey::FirmaDomain, clean, url, Confidence::High);
        }
    }
    if let Some(email) = attrs.get("email").and_then(Value::as_str) {
        let clean = email.trim();
        if looks_like_email(clean) {
            push(out, FieldKey::FirmaEmail, clean, url, Confidence::High);
        }
    }
    if let Some(employees) = employee_display_value(attrs) {
        push(
            out,
            FieldKey::Mitarbeiter,
            &employees,
            url,
            Confidence::High,
        );
    }
    if let Some(industry) = first_industry_name(attrs) {
        push(
            out,
            FieldKey::FirmaGeschaeftstaetigkeit,
            industry,
            url,
            Confidence::High,
        );
    }
    if let Some(code) = first_industry_code(attrs) {
        push(out, FieldKey::WzCode, code, url, Confidence::Medium);
    }
    if let Some(address) = attrs.get("address") {
        if let Some(street) = address.get("street_address").and_then(Value::as_str) {
            push(out, FieldKey::FirmaAnschrift, street, url, Confidence::High);
        }
        if let Some(plz) = address.get("postal_code").and_then(Value::as_str) {
            push(out, FieldKey::FirmaPlz, plz, url, Confidence::High);
        }
        if let Some(city) = address.get("city").and_then(Value::as_str) {
            push(out, FieldKey::FirmaOrt, city, url, Confidence::High);
        }
    }
    if let Some(revenue) = attrs.get("revenue") {
        if let Some(amount) = revenue.get("value").and_then(Value::as_f64) {
            let currency = revenue
                .get("currency")
                .and_then(Value::as_str)
                .unwrap_or("");
            let rendered = if currency.is_empty() {
                amount.to_string()
            } else {
                format!("{amount} {currency}")
            };
            push(out, FieldKey::Umsatz, &rendered, url, Confidence::Medium);
        }
    }
}

fn extract_lead_fields(attrs: &Value, url: &str, out: &mut Vec<(FieldKey, FieldEvidence)>) {
    extract_company_fields(attrs, url, out);
    if let Some(industry) = attrs.get("industry").and_then(Value::as_str) {
        push(
            out,
            FieldKey::FirmaGeschaeftstaetigkeit,
            industry,
            url,
            Confidence::High,
        );
    }
}

fn extract_contact_fields(attrs: &Value, url: &str, out: &mut Vec<(FieldKey, FieldEvidence)>) {
    if let Some(email) = attrs.get("email").and_then(Value::as_str) {
        let clean = email.trim();
        if looks_like_email(clean) {
            push(out, FieldKey::PersonEmail, clean, url, Confidence::Medium);
        }
    }
}

fn extract_snippet_fields(snippet: &str, url: &str, out: &mut Vec<(FieldKey, FieldEvidence)>) {
    let Some(fields) = parse_hit_fields(snippet) else {
        return;
    };
    if let Some(domain) = hit_field_str(&fields, "domain") {
        push(out, FieldKey::FirmaDomain, domain, url, Confidence::High);
    }
    if let Some(employees) = hit_field_str(&fields, "employees") {
        push(out, FieldKey::Mitarbeiter, employees, url, Confidence::High);
    }
    if let Some(industry) = hit_field_str(&fields, "industry") {
        push(
            out,
            FieldKey::FirmaGeschaeftstaetigkeit,
            industry,
            url,
            Confidence::High,
        );
    }
}

fn first_industry_name(attrs: &Value) -> Option<&str> {
    attrs
        .pointer("/industries/industry/0/name")
        .and_then(Value::as_str)
        .or_else(|| attrs.get("industry").and_then(Value::as_str))
}

fn first_industry_code(attrs: &Value) -> Option<&str> {
    attrs
        .pointer("/industries/industry/0/code")
        .and_then(Value::as_str)
}

fn company_identity_matches(query: &str, candidate: &str, score: Option<f64>) -> bool {
    if let Some(score) = score {
        if score < MIN_MATCH_SCORE {
            return false;
        }
    }
    let (query_body, query_suffix) = split_legal_identity(query);
    let (candidate_body, candidate_suffix) = split_legal_identity(candidate);
    if query_body.is_empty() || query_body != candidate_body {
        return false;
    }
    match (query_suffix, candidate_suffix) {
        (Some(query_suffix), Some(candidate_suffix)) => query_suffix == candidate_suffix,
        _ => true,
    }
}

fn split_legal_identity(value: &str) -> (Vec<String>, Option<String>) {
    let mut tokens = normalize_identity(value)
        .split_whitespace()
        .filter(|token| !token.is_empty())
        .map(str::to_string)
        .collect::<Vec<_>>();
    let suffix = peel_legal_suffix(&mut tokens);
    (tokens, suffix)
}

fn peel_legal_suffix(tokens: &mut Vec<String>) -> Option<String> {
    if tokens.len() >= 4 {
        let n = tokens.len();
        if tokens[n - 4] == "gmbh"
            && matches!(tokens[n - 3].as_str(), "und" | "and")
            && tokens[n - 2] == "co"
            && tokens[n - 1] == "kg"
        {
            tokens.truncate(n - 4);
            return Some("gmbh_co_kg".to_string());
        }
    }
    if tokens.len() >= 3 {
        let n = tokens.len();
        if tokens[n - 3] == "gmbh" && tokens[n - 2] == "co" && tokens[n - 1] == "kg" {
            tokens.truncate(n - 3);
            return Some("gmbh_co_kg".to_string());
        }
    }
    let last = tokens.last()?;
    if SINGLE_LEGAL_SUFFIXES.contains(&last.as_str()) {
        tokens.pop()
    } else {
        None
    }
}

fn normalize_identity(value: &str) -> String {
    let mut out = String::new();
    for ch in value.chars() {
        if ch.is_alphanumeric() {
            for lower in ch.to_lowercase() {
                out.push(lower);
            }
        } else {
            out.push(' ');
        }
    }
    out
}

fn canonical_country_code(code: &str) -> &str {
    match Country::from_iso(code) {
        Some(country) => country.as_iso(),
        None => code,
    }
}

fn country_code_matches(requested: Option<&str>, response: Option<&str>) -> bool {
    let Some(requested) = requested.filter(|code| !code.is_empty()) else {
        return true;
    };
    let Some(response) = response.filter(|code| !code.is_empty()) else {
        return true;
    };
    canonical_country_code(requested).eq_ignore_ascii_case(canonical_country_code(response))
}

fn employee_display_value(attrs: &Value) -> Option<String> {
    display_scalar(attrs.get("employee_range"))
        .or_else(|| display_scalar(attrs.get("employee_count")))
}

fn display_scalar(value: Option<&Value>) -> Option<String> {
    match value {
        Some(Value::String(text)) => {
            let trimmed = text.trim();
            if trimmed.is_empty() {
                None
            } else {
                Some(trimmed.to_string())
            }
        }
        Some(Value::Number(number)) => Some(number.to_string()),
        _ => None,
    }
}

fn encode_hit_fields(
    id: &str,
    country: Option<&str>,
    domain: &str,
    employees: &str,
    industry: &str,
) -> String {
    let mut obj = serde_json::Map::new();
    if !id.is_empty() {
        obj.insert("id".to_string(), Value::String(id.to_string()));
    }
    if let Some(code) = country.map(str::trim).filter(|code| !code.is_empty()) {
        obj.insert("country".to_string(), Value::String(code.to_string()));
    }
    if !domain.is_empty() {
        obj.insert("domain".to_string(), Value::String(domain.to_string()));
    }
    if !employees.is_empty() {
        obj.insert(
            "employees".to_string(),
            Value::String(employees.to_string()),
        );
    }
    if !industry.is_empty() {
        obj.insert("industry".to_string(), Value::String(industry.to_string()));
    }
    format!("{HIT_FIELDS_PREFIX}{}", Value::Object(obj))
}

fn parse_hit_fields(snippet: &str) -> Option<Value> {
    let snippet = snippet.trim();
    if snippet.is_empty() || snippet.len() > HIT_FIELDS_MAX_SNIPPET {
        return None;
    }
    let raw = snippet.strip_prefix(HIT_FIELDS_PREFIX)?;
    if raw.is_empty() || raw.len() > HIT_FIELDS_MAX_SNIPPET {
        return None;
    }
    let value: Value = serde_json::from_str(raw).ok()?;
    let object = value.as_object()?;
    if object.is_empty() || object.len() > HIT_FIELDS_MAX_KEYS {
        return None;
    }
    for (key, field) in object {
        if !HIT_FIELD_KEYS.contains(&key.as_str()) {
            return None;
        }
        let text = field.as_str()?;
        if text.len() > HIT_FIELDS_MAX_VALUE {
            return None;
        }
        if key == "id" && nonempty_provider_id(Some(text)).is_none() {
            return None;
        }
    }
    Some(value)
}

fn nonempty_provider_id(value: Option<&str>) -> Option<&str> {
    value.map(str::trim).filter(|id| !id.is_empty())
}

fn hit_field_str<'a>(fields: &'a Value, key: &str) -> Option<&'a str> {
    fields
        .get(key)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
}

fn hit_country_allowed(snippet: &str, requested: Option<&str>) -> bool {
    let fields = parse_hit_fields(snippet);
    let response = fields
        .as_ref()
        .and_then(|fields| hit_field_str(fields, "country"));
    country_code_matches(requested, response)
}

fn hit_company_id(hit: &SourceHit) -> Option<String> {
    if let Some(id) = parse_hit_fields(&hit.snippet)
        .as_ref()
        .and_then(|fields| hit_field_str(fields, "id"))
        .map(str::to_string)
    {
        return Some(id);
    }
    let path = hit.url.split('?').next().unwrap_or("");
    let id = path.rsplit('/').next().unwrap_or("");
    if id.is_empty() || id == "companies" || id == "leads" || id == "contacts" {
        None
    } else {
        Some(id.to_string())
    }
}

fn distinct_hit_ids(hits: &[&SourceHit]) -> Vec<String> {
    let mut ids = Vec::new();
    for hit in hits {
        let Some(id) = hit_company_id(hit) else {
            continue;
        };
        if !ids.iter().any(|existing| existing == &id) {
            ids.push(id);
        }
    }
    ids
}

fn unique_identity_hits(mut hits: Vec<SourceHit>) -> Result<Vec<SourceHit>, SourceError> {
    if hits.is_empty() {
        return Ok(hits);
    }
    let mut ids = Vec::new();
    for hit in &hits {
        let Some(id) = hit_company_id(hit) else {
            return Err(SourceError::ParseFailed {
                detail: "missing company id".to_string(),
            });
        };
        if !ids.iter().any(|existing| existing == &id) {
            ids.push(id);
        }
    }
    if ids.len() > 1 {
        return Err(SourceError::Other(anyhow!("ambiguous_company_identity")));
    }
    hits.truncate(1);
    Ok(hits)
}

fn domain_from_url(raw: &str) -> String {
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
    host.trim_start_matches("www.").trim().to_ascii_lowercase()
}

fn looks_like_email(value: &str) -> bool {
    let trimmed = value.trim();
    if trimmed.is_empty() || trimmed.contains(' ') {
        return false;
    }
    let mut parts = trimmed.splitn(2, '@');
    let local = parts.next().unwrap_or("");
    let domain = parts.next().unwrap_or("");
    !local.is_empty() && domain.contains('.') && !domain.ends_with('.')
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
    if out
        .iter()
        .any(|(existing, evidence)| existing == &key && evidence.value == trimmed)
    {
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

static MODULE: Leadfeeder = Leadfeeder;

pub fn module() -> &'static dyn SourceModule {
    &MODULE
}

#[cfg(test)]
pub(crate) fn fixture_current_match_hits(
    response: &serde_json::Value,
    account_id: &str,
    company: &str,
    country_iso: Option<&str>,
) -> Result<Vec<SourceHit>, SourceError> {
    let hits = current_records_to_hits(response, account_id, company, true, country_iso);
    if hits.is_empty() {
        return Err(SourceError::NoMatch);
    }
    unique_identity_hits(hits)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::credentials::{
        CredentialReference, CredentialResolveError, CredentialResolver, SecretValue,
    };
    use crate::sources::{ResearchMode, SourceCtx};
    use std::io::Read;
    use std::io::Write;
    use std::net::TcpListener;
    use std::path::Path;
    use std::path::PathBuf;
    use std::sync::atomic::AtomicBool;
    use std::sync::atomic::AtomicU64;
    use std::sync::atomic::Ordering;
    use std::sync::Arc;
    use std::sync::Mutex;
    use std::thread;
    use std::time::Duration as StdDuration;

    const LEADS_FIXTURE: &str =
        include_str!("../../fixtures/sources/leadfeeder/leads_wittenstein.json");
    const CONTACTS_FIXTURE: &str =
        include_str!("../../fixtures/sources/leadfeeder/contacts_wittenstein.json");
    const ACCOUNTS_SINGLE: &str =
        include_str!("../../fixtures/sources/leadfeeder/accounts_single_fixture.json");
    const ACCOUNTS_MULTIPLE: &str =
        include_str!("../../fixtures/sources/leadfeeder/accounts_multiple_fixture.json");
    const MATCH_FIXTURE: &str =
        include_str!("../../fixtures/sources/leadfeeder/match_example_manufacturing_fixture.json");
    const SEARCH_FIXTURE: &str =
        include_str!("../../fixtures/sources/leadfeeder/search_example_manufacturing_fixture.json");
    const MATCH_UNRELATED: &str =
        include_str!("../../fixtures/sources/leadfeeder/match_unrelated_fixture.json");
    const MATCH_COUNTRY_MISMATCH: &str =
        include_str!("../../fixtures/sources/leadfeeder/match_country_mismatch_at_fixture.json");
    const MATCH_AMBIGUOUS_IDS: &str =
        include_str!("../../fixtures/sources/leadfeeder/match_ambiguous_ids_fixture.json");
    const MATCH_FIELD_FIDELITY: &str =
        include_str!("../../fixtures/sources/leadfeeder/match_field_fidelity_fixture.json");
    const MATCH_MISSING_DOMAIN: &str = r#"{
  "data": [[{
    "id": "co-fixture-nodomain",
    "type": "company_match",
    "attributes": { "match_score": 0.94 },
    "relationships": {
      "company_summary": {
        "id": "co-fixture-nodomain",
        "type": "company_summary",
        "attributes": {
          "name": "Example Manufacturing AG",
          "employee_range": "51-200",
          "industries": { "industry": [{ "name": "Industrial machinery" }] },
          "address": { "country_code": "DE" }
        }
      }
    }
  }]]
}"#;

    static TEST_ROOT_SEQ: AtomicU64 = AtomicU64::new(1);

    struct TestRoot {
        path: PathBuf,
    }

    impl TestRoot {
        fn new() -> Self {
            let path = std::env::temp_dir().join(format!(
                "ctox-leadfeeder-test-root-{}-{}",
                std::process::id(),
                TEST_ROOT_SEQ.fetch_add(1, Ordering::Relaxed)
            ));
            std::fs::create_dir_all(path.join("runtime")).expect("test root");
            Self { path }
        }

        fn write_env(&self, pairs: &[(&str, &str)]) {
            let db = self.path.join("runtime/ctox.sqlite3");
            let conn = rusqlite::Connection::open(&db).expect("open runtime sqlite");
            conn.execute_batch(
                "CREATE TABLE IF NOT EXISTS runtime_env_kv (
                    env_key TEXT PRIMARY KEY,
                    env_value TEXT NOT NULL
                );",
            )
            .expect("create runtime_env_kv");
            for (key, value) in pairs {
                conn.execute(
                    "INSERT OR REPLACE INTO runtime_env_kv(env_key, env_value) VALUES (?1, ?2)",
                    [*key, *value],
                )
                .expect("insert runtime env");
            }
        }

        fn ctx(&self) -> SourceCtx<'_> {
            SourceCtx {
                root: &self.path,
                country: Some(Country::De),
                mode: ResearchMode::NewRecord,
            }
        }
    }

    impl Drop for TestRoot {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.path);
        }
    }

    struct InjectedResolver {
        outcomes: Vec<(
            &'static str,
            Result<Option<&'static str>, CredentialResolveError>,
        )>,
    }

    impl InjectedResolver {
        fn present(pairs: &[(&'static str, &'static str)]) -> Self {
            Self {
                outcomes: pairs
                    .iter()
                    .map(|(name, value)| (*name, Ok(Some(*value))))
                    .collect(),
            }
        }

        fn current_key() -> Self {
            Self::present(&[(SECRET_NAME, "fixture-current-key")])
        }

        fn outcome(
            name: &'static str,
            outcome: Result<Option<&'static str>, CredentialResolveError>,
        ) -> Self {
            Self {
                outcomes: vec![(name, outcome)],
            }
        }
    }

    impl CredentialResolver for InjectedResolver {
        fn resolve(
            &self,
            reference: &CredentialReference,
        ) -> Result<Option<SecretValue>, CredentialResolveError> {
            assert_eq!(reference.scope, "credentials");
            for (name, outcome) in &self.outcomes {
                if reference.name == *name {
                    return match outcome {
                        Ok(Some(value)) => Ok(Some(SecretValue::new((*value).to_string()))),
                        Ok(None) => Ok(None),
                        Err(error) => Err(*error),
                    };
                }
            }
            Ok(None)
        }
    }

    fn fetch_current(
        ctx: &SourceCtx<'_>,
        company: &str,
        transport: &Transport,
    ) -> Option<Result<Vec<SourceHit>, SourceError>> {
        fetch_direct_with(
            ctx,
            company,
            transport,
            Some(&InjectedResolver::current_key()),
        )
    }

    #[derive(Clone, Debug)]
    struct RecordedRequest {
        method: String,
        path: String,
        headers: Vec<(String, String)>,
        body: String,
    }

    impl RecordedRequest {
        fn header(&self, name: &str) -> Option<&str> {
            self.headers.iter().find_map(|(key, value)| {
                if key.eq_ignore_ascii_case(name) {
                    Some(value.as_str())
                } else {
                    None
                }
            })
        }
    }

    struct MockApi {
        base: String,
        requests: Arc<Mutex<Vec<RecordedRequest>>>,
        stop: Arc<AtomicBool>,
        join: Option<thread::JoinHandle<()>>,
    }

    impl MockApi {
        fn spawn(handler: impl Fn(&RecordedRequest) -> (u16, String) + Send + 'static) -> Self {
            let listener = TcpListener::bind("127.0.0.1:0").expect("bind mock");
            let addr = listener.local_addr().expect("local addr");
            let requests = Arc::new(Mutex::new(Vec::new()));
            let stop = Arc::new(AtomicBool::new(false));
            let reqs = Arc::clone(&requests);
            let stop_flag = Arc::clone(&stop);
            listener.set_nonblocking(true).expect("nonblocking");
            let join = thread::spawn(move || {
                while !stop_flag.load(Ordering::Relaxed) {
                    match listener.accept() {
                        Ok((mut stream, _)) => {
                            stream
                                .set_read_timeout(Some(StdDuration::from_millis(500)))
                                .ok();
                            if let Some(recorded) = read_http_request(&mut stream) {
                                reqs.lock().expect("lock requests").push(recorded.clone());
                                let (status, body) = handler(&recorded);
                                let reason = match status {
                                    200 => "OK",
                                    401 => "Unauthorized",
                                    403 => "Forbidden",
                                    404 => "Not Found",
                                    429 => "Too Many Requests",
                                    _ => "Error",
                                };
                                let response = format!(
                                    "HTTP/1.1 {status} {reason}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                                    body.len()
                                );
                                let _ = stream.write_all(response.as_bytes());
                            }
                        }
                        Err(err) if err.kind() == std::io::ErrorKind::WouldBlock => {
                            thread::sleep(StdDuration::from_millis(10));
                        }
                        Err(_) => break,
                    }
                }
            });
            Self {
                base: format!("http://{addr}"),
                requests,
                stop,
                join: Some(join),
            }
        }

        fn transport(&self) -> Transport {
            Transport {
                base: self.base.clone(),
                allow_hosts: vec!["127.0.0.1".to_string()],
            }
        }

        fn recorded(&self) -> Vec<RecordedRequest> {
            self.requests.lock().expect("lock").clone()
        }
    }

    impl Drop for MockApi {
        fn drop(&mut self) {
            self.stop.store(true, Ordering::Relaxed);
            if let Some(join) = self.join.take() {
                let _ = join.join();
            }
        }
    }

    fn read_http_request(stream: &mut std::net::TcpStream) -> Option<RecordedRequest> {
        let mut buf = Vec::new();
        let mut tmp = [0u8; 1024];
        loop {
            match stream.read(&mut tmp) {
                Ok(0) => break,
                Ok(n) => {
                    buf.extend_from_slice(&tmp[..n]);
                    if buf.windows(4).any(|w| w == b"\r\n\r\n") {
                        break;
                    }
                    if buf.len() > 64 * 1024 {
                        break;
                    }
                }
                Err(_) => break,
            }
        }
        let text = String::from_utf8_lossy(&buf);
        let (head, rest) = text.split_once("\r\n\r\n")?;
        let mut lines = head.split("\r\n");
        let request_line = lines.next()?;
        let mut parts = request_line.split_whitespace();
        let method = parts.next()?.to_string();
        let path = parts.next()?.to_string();
        let mut headers = Vec::new();
        let mut content_length = 0usize;
        for line in lines {
            if let Some((name, value)) = line.split_once(':') {
                let name = name.trim().to_string();
                let value = value.trim().to_string();
                if name.eq_ignore_ascii_case("content-length") {
                    content_length = value.parse().unwrap_or(0);
                }
                headers.push((name, value));
            }
        }
        let mut body = rest.as_bytes().to_vec();
        while body.len() < content_length {
            let n = stream.read(&mut tmp).ok()?;
            if n == 0 {
                break;
            }
            body.extend_from_slice(&tmp[..n]);
        }
        body.truncate(content_length);
        Some(RecordedRequest {
            method,
            path,
            headers,
            body: String::from_utf8_lossy(&body).into_owned(),
        })
    }

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

    fn assert_read_only(requests: &[RecordedRequest]) {
        for request in requests {
            assert!(
                !request.path.contains("/enrichment"),
                "must not create enrichment jobs: {}",
                request.path
            );
            assert!(
                !request.path.contains("/find"),
                "must not create find-contact jobs: {}",
                request.path
            );
            if request.method == "GET" {
                assert!(
                    !request.path.starts_with("/v1/companies/")
                        || request.path.starts_with("/v1/companies/match")
                        || request.path.starts_with("/v1/companies/search"),
                    "must not retrieve company deep data: {} {}",
                    request.method,
                    request.path
                );
                assert!(
                    !request.path.starts_with("/v1/contacts"),
                    "must not retrieve contact deep data: {}",
                    request.path
                );
            }
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
        assert!(auth.contains(&FieldKey::FirmaEmail));
        assert!(auth.contains(&FieldKey::FirmaDomain));
        assert!(auth.contains(&FieldKey::Mitarbeiter));
        assert!(auth.contains(&FieldKey::PersonEmail));
    }

    #[test]
    fn shape_query_is_none_for_api_source() {
        let ctx = SourceCtx {
            root: Path::new("/tmp/ctox-test"),
            country: Some(Country::De),
            mode: ResearchMode::NewRecord,
        };
        assert!(module()
            .shape_query("Example Manufacturing AG", &ctx)
            .is_none());
    }

    #[test]
    fn fetch_direct_engages_for_each_dach_country() {
        for country in [Country::De, Country::At, Country::Ch] {
            let ctx = SourceCtx {
                root: Path::new("/tmp/ctox-nonexistent-leadfeeder"),
                country: Some(country),
                mode: ResearchMode::NewRecord,
            };
            let r = module().fetch_direct(&ctx, "Example Manufacturing AG");
            assert!(r.is_some(), "{country:?} must engage");
            match r.unwrap() {
                Err(SourceError::Other(inner)) => {
                    assert_eq!(
                        inner.to_string(),
                        CredentialResolveError::Unavailable.to_string()
                    );
                }
                other => panic!("expected unavailable resolver, got: {other:?}"),
            }
        }
    }

    #[test]
    fn fetch_direct_without_resolver_is_unavailable() {
        let ctx = SourceCtx {
            root: Path::new("/tmp/ctox-nonexistent-leadfeeder"),
            country: Some(Country::De),
            mode: ResearchMode::NewRecord,
        };
        let result = module()
            .fetch_direct(&ctx, "Example Manufacturing AG")
            .expect("DACH engages");
        match result {
            Err(SourceError::Other(inner)) => {
                assert_eq!(
                    inner.to_string(),
                    CredentialResolveError::Unavailable.to_string()
                );
            }
            other => panic!("expected unavailable resolver, got: {other:?}"),
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
    fn lead_fixture_yields_firma_email_and_domain_with_high_confidence() {
        let page = dummy_page(
            LEADS_FIXTURE,
            "https://api.leadfeeder.com/accounts/me/leads",
        );
        let fields = module().extract_fields(&page);
        let firma_email = fields
            .iter()
            .find(|(k, _)| matches!(k, FieldKey::FirmaEmail))
            .expect("firma_email present");
        assert_eq!(firma_email.1.value, "info@wittenstein.de");
        assert!(matches!(firma_email.1.confidence, Confidence::High));
        let firma_domain = fields
            .iter()
            .find(|(k, _)| matches!(k, FieldKey::FirmaDomain))
            .expect("firma_domain present");
        assert_eq!(firma_domain.1.value, "wittenstein.de");
        assert!(matches!(firma_domain.1.confidence, Confidence::High));
    }

    #[test]
    fn contact_fixture_yields_person_email_with_medium_confidence() {
        let page = dummy_page(
            CONTACTS_FIXTURE,
            "https://api.leadfeeder.com/accounts/me/contacts",
        );
        let fields = module().extract_fields(&page);
        let person_emails: Vec<_> = fields
            .iter()
            .filter(|(k, _)| matches!(k, FieldKey::PersonEmail))
            .map(|(_, ev)| (ev.value.clone(), ev.confidence))
            .collect();
        assert!(
            person_emails
                .iter()
                .any(|(v, _)| v == "manfred.weber@wittenstein.de"),
            "expected Weber email, got: {person_emails:?}"
        );
        for (_, conf) in &person_emails {
            assert!(matches!(conf, Confidence::Medium));
        }
    }

    #[test]
    fn current_match_fixture_yields_identity_and_provenance() {
        let page = dummy_page(
            MATCH_FIXTURE,
            "https://api.leadfeeder.com/v1/companies/match?account_id=acct-fixture-1",
        );
        let fields = module().extract_fields(&page);
        let name = fields
            .iter()
            .find(|(k, _)| matches!(k, FieldKey::FirmaName))
            .expect("firma_name");
        assert_eq!(name.1.value, "Example Manufacturing AG");
        assert_eq!(
            name.1.source_url,
            "https://api.leadfeeder.com/v1/companies/match?account_id=acct-fixture-1"
        );
        let domain = fields
            .iter()
            .find(|(k, _)| matches!(k, FieldKey::FirmaDomain))
            .expect("firma_domain");
        assert_eq!(domain.1.value, "example-manufacturing.test");
        assert!(fields
            .iter()
            .any(|(k, ev)| matches!(k, FieldKey::Mitarbeiter) && ev.value == "101-500"));
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
        let page = dummy_page(body, "https://api.leadfeeder.com/accounts/me/leads");
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
            domain_from_url("https://www.example-manufacturing.test/de"),
            "example-manufacturing.test"
        );
        assert_eq!(domain_from_url("http://example.com"), "example.com");
        assert_eq!(domain_from_url("example.com/foo"), "example.com");
        assert_eq!(domain_from_url(""), "");
    }

    #[test]
    fn looks_like_email_accepts_real_addresses_and_rejects_others() {
        assert!(looks_like_email("a@b.de"));
        assert!(looks_like_email("foo.bar@example.co.uk"));
        assert!(!looks_like_email(""));
        assert!(!looks_like_email("unknown"));
        assert!(!looks_like_email("a@b"));
        assert!(!looks_like_email("a@b."));
        assert!(!looks_like_email("a b@c.de"));
    }

    #[test]
    fn current_api_match_uses_x_api_key_and_explicit_account() {
        let mock = MockApi::spawn(|req| {
            if req.method == "GET" && req.path.starts_with("/v1/accounts") {
                panic!("explicit account must not list accounts");
            }
            assert_eq!(req.method, "POST");
            assert!(req
                .path
                .starts_with("/v1/companies/match?account_id=acct-fixture-1"));
            assert_eq!(req.header("X-Api-Key"), Some("fixture-current-key"));
            assert!(req.header("Authorization").is_none());
            (200, MATCH_FIXTURE.to_string())
        });
        let root = TestRoot::new();
        root.write_env(&[(ACCOUNT_ID_KEY, "acct-fixture-1")]);
        let hits = fetch_current(&root.ctx(), "Example Manufacturing AG", &mock.transport())
            .expect("engages")
            .expect("match hits");
        assert_eq!(hits[0].title, "Example Manufacturing AG");
        assert!(hits[0]
            .url
            .contains("/v1/companies/co-fixture-1?account_id=acct-fixture-1"));
        assert!(hits[0].snippet.contains("example-manufacturing.test"));
        let recorded = mock.recorded();
        assert_eq!(recorded.len(), 1);
        assert_read_only(&recorded);
        assert!(recorded[0].body.contains("Example Manufacturing AG"));
        assert!(recorded[0].body.contains("\"country_code\":\"DE\""));
    }

    #[test]
    fn current_api_resolves_single_authorized_account() {
        let mock = MockApi::spawn(|req| {
            if req.method == "GET" && req.path == "/v1/accounts" {
                assert_eq!(req.header("X-Api-Key"), Some("fixture-current-key"));
                return (200, ACCOUNTS_SINGLE.to_string());
            }
            assert!(req.path.contains("account_id=acct-fixture-1"));
            (200, MATCH_FIXTURE.to_string())
        });
        let root = TestRoot::new();
        root.write_env(&[]);
        let hits = fetch_current(&root.ctx(), "Example Manufacturing AG", &mock.transport())
            .expect("engages")
            .expect("hits");
        assert_eq!(hits[0].title, "Example Manufacturing AG");
        assert_read_only(&mock.recorded());
    }

    #[test]
    fn current_api_requires_explicit_account_when_multiple() {
        let mock = MockApi::spawn(|req| {
            assert_eq!(req.path, "/v1/accounts");
            (200, ACCOUNTS_MULTIPLE.to_string())
        });
        let root = TestRoot::new();
        root.write_env(&[]);
        let err = fetch_current(&root.ctx(), "Example Manufacturing AG", &mock.transport())
            .expect("engages")
            .expect_err("multiple accounts");
        match err {
            SourceError::Other(inner) => {
                let text = inner.to_string();
                assert!(text.contains("account_selection_required"), "{text}");
            }
            other => panic!("expected account selection error, got {other:?}"),
        }
        assert_eq!(mock.recorded().len(), 1);
        assert_read_only(&mock.recorded());
    }

    #[test]
    fn rejects_me_as_account_id() {
        let mock = MockApi::spawn(|_req| {
            panic!("must not call the API with account alias me");
        });
        let root = TestRoot::new();
        root.write_env(&[(ACCOUNT_ID_KEY, "me")]);
        let err = fetch_current(&root.ctx(), "Example Manufacturing AG", &mock.transport())
            .expect("engages")
            .expect_err("rejected me");
        match err {
            SourceError::Other(inner) => {
                assert!(inner.to_string().contains("explicit authorized account"));
            }
            other => panic!("unexpected {other:?}"),
        }
        assert!(mock.recorded().is_empty());
    }

    #[test]
    fn current_api_search_fallback_and_identity_reject_unrelated() {
        let mock = MockApi::spawn(|req| {
            if req.path.starts_with("/v1/companies/match") {
                return (200, MATCH_UNRELATED.to_string());
            }
            if req.path.starts_with("/v1/companies/search") {
                return (200, SEARCH_FIXTURE.to_string());
            }
            panic!("unexpected {}", req.path);
        });
        let root = TestRoot::new();
        root.write_env(&[(ACCOUNT_ID_KEY, "acct-fixture-1")]);
        let hits = fetch_current(&root.ctx(), "Example Manufacturing AG", &mock.transport())
            .expect("engages")
            .expect("search fallback");
        assert_eq!(hits[0].title, "Example Manufacturing AG");
        let unrelated = fetch_current(&root.ctx(), "Completely Different GmbH", &mock.transport())
            .expect("engages");
        assert!(matches!(unrelated, Err(SourceError::NoMatch)));
        assert_read_only(&mock.recorded());
    }

    #[test]
    fn distinguishable_error_classes() {
        let cases: Vec<(u16, &str, fn(&SourceError) -> bool)> = vec![
            (401, "{}", |err| match err {
                SourceError::Other(inner) => {
                    inner.to_string() == "authentication_rejected: http 401"
                }
                _ => false,
            }),
            (429, "{}", |err| {
                matches!(err, SourceError::RateLimited { .. })
            }),
            (
                403,
                r#"{"code":"invalid_api_key","message":"fixture invalid key"}"#,
                |err| match err {
                    SourceError::Other(inner) => {
                        inner.to_string() == "authentication_rejected: http 403"
                    }
                    _ => false,
                },
            ),
            (
                403,
                r#"{"code":"forbidden","message":"fixture entitlement"}"#,
                |err| match err {
                    SourceError::Other(inner) => inner.to_string().contains("entitlement"),
                    _ => false,
                },
            ),
            (200, "{not-json", |err| {
                matches!(err, SourceError::ParseFailed { .. })
            }),
            (200, r#"{"data":[]}"#, |err| {
                matches!(err, SourceError::NoMatch)
            }),
        ];
        for (status, body, predicate) in cases {
            let body = body.to_string();
            let mock = MockApi::spawn(move |_req| (status, body.clone()));
            let root = TestRoot::new();
            root.write_env(&[(ACCOUNT_ID_KEY, "acct-fixture-1")]);
            let resolver = InjectedResolver::present(&[(SECRET_NAME, SECRET_CANARY)]);
            let err = fetch_direct_with(
                &root.ctx(),
                "Example Manufacturing AG",
                &mock.transport(),
                Some(&resolver),
            )
            .expect("engages")
            .expect_err("classified error");
            assert!(predicate(&err), "status {status} classified as {err:?}");
        }
    }

    const SECRET_CANARY: &str = "FIXTURE_LEADFEEDER_KEY_DO_NOT_LEAK";

    fn serialized_source_failure(err: &SourceError) -> String {
        serde_json::json!({
            "kind": err.as_str(),
            "error": err.to_string(),
            "secret_name": match err {
                SourceError::CredentialMissing { secret_name } => Some(*secret_name),
                _ => None,
            },
            "secret_value_in_payload": false,
        })
        .to_string()
    }

    fn assert_no_secret_canary(haystack: &str) {
        assert!(
            !haystack.contains(SECRET_CANARY),
            "secret canary leaked: {haystack}"
        );
    }

    #[test]
    fn test_root_uses_process_temp_dir() {
        let first = TestRoot::new();
        let second = TestRoot::new();
        let temp = std::env::temp_dir();
        assert_eq!(first.path.parent(), Some(temp.as_path()));
        assert_eq!(second.path.parent(), Some(temp.as_path()));
        let first_name = first
            .path
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("");
        let second_name = second
            .path
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("");
        assert!(
            first_name.starts_with("ctox-leadfeeder-test-root-"),
            "{first_name}"
        );
        assert!(
            second_name.starts_with("ctox-leadfeeder-test-root-"),
            "{second_name}"
        );
        assert_ne!(first.path, second.path);
    }

    #[test]
    fn auth_scheme_debug_redacts_secret_material() {
        let api = AuthScheme::ApiKey(SECRET_CANARY.to_string());
        let legacy = AuthScheme::LegacyToken(SECRET_CANARY.to_string());
        assert_no_secret_canary(&format!("{api:?}"));
        assert_no_secret_canary(&format!("{legacy:?}"));
        assert!(format!("{api:?}").contains("<redacted>"));
        assert!(format!("{legacy:?}").contains("<redacted>"));
    }

    #[test]
    fn provider_and_config_errors_do_not_echo_secret_material() {
        let bodies = [
            (
                403,
                format!(r#"{{"code":"forbidden","message":"supplied key {SECRET_CANARY}"}}"#),
            ),
            (
                403,
                format!(r#"{{"code":"{SECRET_CANARY}","message":"echo"}}"#),
            ),
            (
                500,
                format!(r#"{{"error":"upstream","key":"{SECRET_CANARY}"}}"#),
            ),
        ];
        for (status, body) in bodies {
            let mock = MockApi::spawn(move |_req| (status, body.clone()));
            let root = TestRoot::new();
            root.write_env(&[(ACCOUNT_ID_KEY, "acct-fixture-1")]);
            let err = fetch_current(&root.ctx(), "Example Manufacturing AG", &mock.transport())
                .expect("engages")
                .expect_err("classified error");
            let display = err.to_string();
            let debug = format!("{err:?}");
            let serialized = serialized_source_failure(&err);
            assert_no_secret_canary(&display);
            assert_no_secret_canary(&debug);
            assert_no_secret_canary(&serialized);
            match status {
                403 => assert!(display.contains("entitlement"), "{display}"),
                500 => assert!(display.contains("leadfeeder http 500"), "{display}"),
                _ => {}
            }
        }
    }

    #[test]
    fn invalid_auth_scheme_does_not_echo_raw_config() {
        let mock = MockApi::spawn(|_req| panic!("must not call API with invalid scheme"));
        let root = TestRoot::new();
        root.write_env(&[
            (AUTH_SCHEME_KEY, SECRET_CANARY),
            (ACCOUNT_ID_KEY, "acct-fixture-1"),
        ]);
        let err = fetch_current(&root.ctx(), "Example Manufacturing AG", &mock.transport())
            .expect("engages")
            .expect_err("invalid scheme");
        let display = err.to_string();
        assert!(
            display.contains("invalid LEADFEEDER_AUTH_SCHEME"),
            "{display}"
        );
        assert!(!display.contains(SECRET_CANARY), "{display}");
        assert_no_secret_canary(&serialized_source_failure(&err));
        assert!(mock.recorded().is_empty());
    }

    #[test]
    fn never_sends_current_key_as_legacy_token() {
        let mock = MockApi::spawn(|req| {
            assert_eq!(req.header("X-Api-Key"), Some("fixture-current-key"));
            assert!(req.header("Authorization").is_none());
            (200, MATCH_FIXTURE.to_string())
        });
        let root = TestRoot::new();
        root.write_env(&[
            (AUTH_SCHEME_KEY, "api_key"),
            (ACCOUNT_ID_KEY, "acct-fixture-1"),
        ]);
        let _ = fetch_current(&root.ctx(), "Example Manufacturing AG", &mock.transport());
        assert!(mock.recorded()[0].path.starts_with("/v1/companies/match"));
    }

    #[test]
    fn legacy_scheme_uses_token_auth_and_legacy_paths() {
        let mock = MockApi::spawn(|req| {
            assert_eq!(
                req.header("Authorization"),
                Some("Token token=fixture-legacy-token")
            );
            assert!(req.header("X-Api-Key").is_none());
            if req.path.starts_with("/accounts/acct-fixture-1/leads") {
                return (200, LEADS_FIXTURE.to_string());
            }
            if req.path.starts_with("/accounts/acct-fixture-1/contacts") {
                return (200, CONTACTS_FIXTURE.to_string());
            }
            panic!("unexpected legacy path {}", req.path);
        });
        let root = TestRoot::new();
        root.write_env(&[
            (AUTH_SCHEME_KEY, "legacy"),
            (ACCOUNT_ID_KEY, "acct-fixture-1"),
        ]);
        let resolver = InjectedResolver::present(&[(LEGACY_SECRET_NAME, "fixture-legacy-token")]);
        let hits = fetch_direct_with(
            &root.ctx(),
            "WITTENSTEIN SE",
            &mock.transport(),
            Some(&resolver),
        )
        .expect("engages")
        .expect("legacy hits");
        assert!(hits.iter().any(|hit| hit.title.contains("WITTENSTEIN")));
        assert!(mock
            .recorded()
            .iter()
            .all(|req| req.path.starts_with("/accounts/")));
    }

    #[test]
    fn ambiguous_secrets_without_scheme_are_not_guessed() {
        let mock = MockApi::spawn(|_req| panic!("must not guess auth scheme"));
        let root = TestRoot::new();
        root.write_env(&[]);
        let resolver = InjectedResolver::present(&[
            (SECRET_NAME, "fixture-current-key"),
            (LEGACY_SECRET_NAME, "fixture-legacy-token"),
        ]);
        let err = fetch_direct_with(
            &root.ctx(),
            "Example Manufacturing AG",
            &mock.transport(),
            Some(&resolver),
        )
        .expect("engages")
        .expect_err("ambiguous");
        match err {
            SourceError::Other(inner) => {
                assert!(inner.to_string().contains("ambiguous"));
            }
            other => panic!("unexpected {other:?}"),
        }
        assert!(mock.recorded().is_empty());
    }

    #[test]
    fn resolver_absent_key_is_credential_missing() {
        let mock = MockApi::spawn(|_req| panic!("absent key must not call API"));
        let root = TestRoot::new();
        root.write_env(&[(ACCOUNT_ID_KEY, "acct-fixture-1")]);
        let resolver = InjectedResolver::outcome(SECRET_NAME, Ok(None));
        let err = fetch_direct_with(
            &root.ctx(),
            "Example Manufacturing AG",
            &mock.transport(),
            Some(&resolver),
        )
        .expect("engages")
        .expect_err("missing");
        match err {
            SourceError::CredentialMissing { secret_name } => {
                assert_eq!(secret_name, SECRET_NAME);
            }
            other => panic!("unexpected {other:?}"),
        }
        assert!(mock.recorded().is_empty());
    }

    #[test]
    fn resolver_empty_stored_value_is_invalid_not_absent() {
        for stored in ["", "   "] {
            let mock = MockApi::spawn(|_req| panic!("invalid stored key must not call API"));
            let root = TestRoot::new();
            root.write_env(&[(AUTH_SCHEME_KEY, "api_key")]);
            let resolver = InjectedResolver::present(&[(SECRET_NAME, stored)]);
            let error = fetch_direct_with(
                &root.ctx(),
                "Example Manufacturing AG",
                &mock.transport(),
                Some(&resolver),
            )
            .expect("engages")
            .expect_err("invalid stored key");
            assert!(matches!(&error, SourceError::Other(_)));
            assert_eq!(error.to_string(), "credential_invalid: empty stored value");
            assert!(mock.recorded().is_empty());
        }
    }

    #[test]
    fn resolver_denied_unavailable_decrypt_and_encoding_are_sanitized() {
        for outcome in [
            Err(CredentialResolveError::Denied),
            Err(CredentialResolveError::Unavailable),
            Err(CredentialResolveError::DecryptionFailed),
            Err(CredentialResolveError::InvalidEncoding),
        ] {
            let mock = MockApi::spawn(|_req| panic!("resolver error must not call API"));
            let root = TestRoot::new();
            root.write_env(&[(ACCOUNT_ID_KEY, "acct-fixture-1")]);
            let resolver = InjectedResolver::outcome(SECRET_NAME, outcome);
            let err = fetch_direct_with(
                &root.ctx(),
                "Example Manufacturing AG",
                &mock.transport(),
                Some(&resolver),
            )
            .expect("engages")
            .expect_err("resolver error");
            let display = err.to_string();
            let debug = format!("{err:?}");
            assert_eq!(display, outcome.unwrap_err().to_string());
            assert_no_secret_canary(&display);
            assert_no_secret_canary(&debug);
            assert_no_secret_canary(&serialized_source_failure(&err));
            assert!(mock.recorded().is_empty());
        }
    }

    #[test]
    fn runtime_config_secret_is_not_a_credential_fallback() {
        let mock = MockApi::spawn(|_req| panic!("must not use runtime_env secret"));
        let root = TestRoot::new();
        root.write_env(&[
            (SECRET_NAME, "fixture-current-key"),
            (ACCOUNT_ID_KEY, "acct-fixture-1"),
        ]);
        let err = fetch_direct_with(
            &root.ctx(),
            "Example Manufacturing AG",
            &mock.transport(),
            None,
        )
        .expect("engages")
        .expect_err("no resolver");
        match err {
            SourceError::Other(inner) => {
                assert_eq!(
                    inner.to_string(),
                    CredentialResolveError::Unavailable.to_string()
                );
            }
            other => panic!("unexpected {other:?}"),
        }
        assert!(mock.recorded().is_empty());
    }

    #[test]
    fn auth_header_uses_resolver_secret() {
        let mock = MockApi::spawn(|req| {
            assert_eq!(req.header("X-Api-Key"), Some("fixture-current-key"));
            assert!(req.header("Authorization").is_none());
            (200, MATCH_FIXTURE.to_string())
        });
        let root = TestRoot::new();
        root.write_env(&[
            (SECRET_NAME, "runtime-env-must-not-win"),
            (ACCOUNT_ID_KEY, "acct-fixture-1"),
        ]);
        let _hits = fetch_current(&root.ctx(), "Example Manufacturing AG", &mock.transport())
            .expect("engages")
            .expect("hits");
        assert_eq!(
            mock.recorded()[0].header("X-Api-Key"),
            Some("fixture-current-key")
        );
    }

    #[test]
    fn fixture_current_match_hits_uses_production_identity_path() {
        let value: Value = serde_json::from_str(MATCH_FIXTURE).unwrap();
        let hits = fixture_current_match_hits(
            &value,
            "acct-fixture-1",
            "Example Manufacturing AG",
            Some("DE"),
        )
        .expect("unique identity");
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].title, "Example Manufacturing AG");
        assert!(hits[0].url.contains("co-fixture-1"));
    }

    #[test]
    fn extract_from_hits_requires_identity_and_keeps_source_url() {
        let ctx = SourceCtx {
            root: Path::new("/tmp/ctox-nonexistent-leadfeeder"),
            country: Some(Country::De),
            mode: ResearchMode::NewRecord,
        };
        let hits = vec![SourceHit {
            title: "Example Manufacturing AG".to_string(),
            url: "https://api.leadfeeder.com/v1/companies/co-fixture-1?account_id=acct-fixture-1"
                .to_string(),
            snippet: encode_hit_fields(
                "co-fixture-1",
                Some("DE"),
                "example-manufacturing.test",
                "101-500",
                "Manufacture of bearings",
            ),
        }];
        let fields = module().extract_from_hits(&ctx, "Example Manufacturing AG", &hits);
        assert!(fields
            .iter()
            .any(|(k, ev)| matches!(k, FieldKey::FirmaName)
                && ev.value == "Example Manufacturing AG"));
        assert!(fields
            .iter()
            .any(|(k, ev)| matches!(k, FieldKey::FirmaDomain)
                && ev.value == "example-manufacturing.test"
                && ev.source_url.contains("/v1/companies/co-fixture-1")));
        assert!(fields
            .iter()
            .any(|(k, ev)| matches!(k, FieldKey::Mitarbeiter)
                && ev.value == "101-500"
                && ev.source_url.contains("/v1/companies/co-fixture-1")));
        assert!(fields
            .iter()
            .any(|(k, ev)| matches!(k, FieldKey::FirmaGeschaeftstaetigkeit)
                && ev.value == "Manufacture of bearings"));
        let rejected = module().extract_from_hits(&ctx, "Completely Different GmbH", &hits);
        assert!(rejected.is_empty());
    }

    #[test]
    fn identity_requires_exact_significant_tokens() {
        assert!(company_identity_matches(
            "Example Manufacturing AG",
            "Example Manufacturing",
            None
        ));
        assert!(company_identity_matches(
            "Example-Manufacturing ag",
            "example manufacturing",
            None
        ));
        assert!(company_identity_matches(
            "Müller Technik AG",
            "Müller Technik",
            None
        ));
        assert!(company_identity_matches("AB AG", "AB", None));
        assert!(company_identity_matches("3M", "3M AG", None));
        assert!(!company_identity_matches(
            "Example Manufacturing AG",
            "Example Manufacturing Holdings AG",
            Some(0.99)
        ));
        assert!(!company_identity_matches(
            "Example Manufacturing AG",
            "Example Manufacturing Automotive GmbH",
            Some(0.99)
        ));
        assert!(!company_identity_matches(
            "Example Manufacturing AG",
            "Example",
            Some(0.99)
        ));
        assert!(!company_identity_matches(
            "Example Manufacturing AG",
            "Completely Different GmbH",
            Some(0.99)
        ));
        assert!(!company_identity_matches(
            "Müller Technik AG",
            "Muller Technik AG",
            None
        ));
        assert!(!company_identity_matches(
            "AG",
            "Example Manufacturing AG",
            None
        ));
        assert!(company_identity_matches(
            "Example Company AG",
            "Example Company",
            None
        ));
        assert!(company_identity_matches(
            "Example GmbH & Co. KG",
            "Example GmbH & Co KG",
            None
        ));
        assert!(company_identity_matches(
            "Example GmbH und Co. KG",
            "Example GmbH & Co. KG",
            None
        ));
        assert!(!company_identity_matches(
            "Example AG",
            "Example GmbH",
            None
        ));
        assert!(!company_identity_matches(
            "Example Manufacturing AG",
            "Example Manufacturing GmbH",
            None
        ));
        assert!(!company_identity_matches(
            "Example GmbH & Co. KG",
            "Example GmbH",
            None
        ));
        assert!(!company_identity_matches(
            "Example Company",
            "Example AG",
            None
        ));
    }

    #[test]
    fn summary_to_hit_rejects_blank_provider_id() {
        let blank = serde_json::json!({
            "id": "  ",
            "type": "company_summary",
            "attributes": {
                "name": "Example Manufacturing AG",
                "address": { "country_code": "DE" }
            }
        });
        assert!(summary_to_hit(
            &blank,
            "acct-fixture-1",
            "Example Manufacturing AG",
            Some(0.96),
            Some("DE"),
        )
        .is_none());
        let missing = serde_json::json!({
            "type": "company_summary",
            "attributes": {
                "name": "Example Manufacturing AG",
                "address": { "country_code": "DE" }
            }
        });
        assert!(summary_to_hit(
            &missing,
            "acct-fixture-1",
            "Example Manufacturing AG",
            Some(0.96),
            Some("DE"),
        )
        .is_none());
    }

    #[test]
    fn unique_identity_hits_rejects_missing_ids_instead_of_title_fallback() {
        let hits = vec![
            SourceHit {
                title: "Example Manufacturing AG".to_string(),
                url: "https://api.leadfeeder.com/v1/companies/?account_id=acct-fixture-1"
                    .to_string(),
                snippet: String::new(),
            },
            SourceHit {
                title: "Example Manufacturing AG".to_string(),
                url: "https://api.leadfeeder.com/v1/companies/".to_string(),
                snippet: String::new(),
            },
        ];
        let err = unique_identity_hits(hits).expect_err("missing ids");
        assert!(matches!(err, SourceError::ParseFailed { .. }));
    }

    #[test]
    fn parse_hit_fields_rejects_invalid_or_oversized_payloads() {
        assert!(parse_hit_fields(r#"leadfeeder_fields:[1]"#).is_none());
        assert!(parse_hit_fields(r#"leadfeeder_fields:{"id":""}"#).is_none());
        assert!(parse_hit_fields(r#"leadfeeder_fields:{"id":"co-1","extra":"nope"}"#).is_none());
        let oversized = format!(
            r#"leadfeeder_fields:{{"id":"{}"}}"#,
            "x".repeat(HIT_FIELDS_MAX_VALUE + 1)
        );
        assert!(parse_hit_fields(&oversized).is_none());
        assert!(parse_hit_fields(
            r#"leadfeeder_fields:{"id":"co-fixture-1","domain":"factory-24.test"}"#
        )
        .is_some());
    }

    #[test]
    fn unlabeled_snippet_does_not_guess_fields_from_domain() {
        let ctx = SourceCtx {
            root: Path::new("/tmp/ctox-nonexistent-leadfeeder"),
            country: Some(Country::De),
            mode: ResearchMode::NewRecord,
        };
        let hits = vec![SourceHit {
            title: "Example Manufacturing AG".to_string(),
            url: "https://api.leadfeeder.com/v1/companies/co-fixture-1?account_id=acct-fixture-1"
                .to_string(),
            snippet: "factory-24.test · 101-500 · Manufacture of bearings".to_string(),
        }];
        let fields = module().extract_from_hits(&ctx, "Example Manufacturing AG", &hits);
        assert!(fields
            .iter()
            .any(|(k, ev)| matches!(k, FieldKey::FirmaName)
                && ev.value == "Example Manufacturing AG"));
        assert!(!fields
            .iter()
            .any(|(k, _)| matches!(k, FieldKey::FirmaDomain)));
        assert!(!fields
            .iter()
            .any(|(k, ev)| matches!(k, FieldKey::Mitarbeiter)
                && (ev.value.contains("factory-24.test") || ev.value.contains("101-500"))));
        assert!(!fields
            .iter()
            .any(|(k, _)| matches!(k, FieldKey::FirmaGeschaeftstaetigkeit)));
    }

    #[test]
    fn structured_fields_roundtrip_numeric_hyphen_domain_and_industry() {
        let ctx = SourceCtx {
            root: Path::new("/tmp/ctox-nonexistent-leadfeeder"),
            country: Some(Country::De),
            mode: ResearchMode::NewRecord,
        };
        let value: Value = serde_json::from_str(MATCH_FIELD_FIDELITY).unwrap();
        let hits = current_records_to_hits(
            &value,
            "acct-fixture-1",
            "Example Manufacturing AG",
            true,
            Some("DE"),
        );
        assert_eq!(hits.len(), 1);
        assert!(hits[0].snippet.starts_with(HIT_FIELDS_PREFIX));
        let fields = module().extract_from_hits(&ctx, "Example Manufacturing AG", &hits);
        assert!(fields
            .iter()
            .any(|(k, ev)| matches!(k, FieldKey::FirmaDomain)
                && ev.value == "factory-24.test"
                && ev.source_url.contains("co-fixture-fields-1")));
        assert!(fields
            .iter()
            .any(|(k, ev)| matches!(k, FieldKey::Mitarbeiter)
                && ev.value == "120"
                && ev.source_url.contains("co-fixture-fields-1")));
        assert!(fields
            .iter()
            .any(|(k, ev)| matches!(k, FieldKey::FirmaGeschaeftstaetigkeit)
                && ev.value == "Manufacture of bearings"));
        assert!(!fields
            .iter()
            .any(|(k, ev)| matches!(k, FieldKey::Mitarbeiter) && ev.value.contains("factory-24")));
    }

    #[test]
    fn structured_fields_omit_missing_domain_and_keep_range_employees() {
        let ctx = SourceCtx {
            root: Path::new("/tmp/ctox-nonexistent-leadfeeder"),
            country: Some(Country::De),
            mode: ResearchMode::NewRecord,
        };
        let value: Value = serde_json::from_str(MATCH_MISSING_DOMAIN).unwrap();
        let hits = current_records_to_hits(
            &value,
            "acct-fixture-1",
            "Example Manufacturing AG",
            true,
            Some("DE"),
        );
        assert_eq!(hits.len(), 1);
        let parsed = parse_hit_fields(&hits[0].snippet).expect("structured snippet");
        assert!(hit_field_str(&parsed, "domain").is_none());
        let fields = module().extract_from_hits(&ctx, "Example Manufacturing AG", &hits);
        assert!(!fields
            .iter()
            .any(|(k, _)| matches!(k, FieldKey::FirmaDomain)));
        assert!(fields
            .iter()
            .any(|(k, ev)| matches!(k, FieldKey::Mitarbeiter) && ev.value == "51-200"));
        assert!(fields
            .iter()
            .any(|(k, ev)| matches!(k, FieldKey::FirmaGeschaeftstaetigkeit)
                && ev.value == "Industrial machinery"));
    }

    #[test]
    fn rejects_explicit_response_country_mismatch() {
        let value: Value = serde_json::from_str(MATCH_COUNTRY_MISMATCH).unwrap();
        let hits = current_records_to_hits(
            &value,
            "acct-fixture-1",
            "Example Manufacturing AG",
            true,
            Some("DE"),
        );
        assert!(hits.is_empty());
        let accepted = current_records_to_hits(
            &value,
            "acct-fixture-1",
            "Example Manufacturing AG",
            true,
            Some("AT"),
        );
        assert_eq!(accepted.len(), 1);
        assert_eq!(accepted[0].title, "Example Manufacturing AG");
    }

    #[test]
    fn current_api_rejects_country_mismatch_without_search_fallback_hit() {
        let mock = MockApi::spawn(|req| {
            if req.path.starts_with("/v1/companies/match") {
                return (200, MATCH_COUNTRY_MISMATCH.to_string());
            }
            if req.path.starts_with("/v1/companies/search") {
                return (200, r#"{"data":[]}"#.to_string());
            }
            panic!("unexpected {}", req.path);
        });
        let root = TestRoot::new();
        root.write_env(&[(ACCOUNT_ID_KEY, "acct-fixture-1")]);
        let err = fetch_current(&root.ctx(), "Example Manufacturing AG", &mock.transport())
            .expect("engages")
            .expect_err("country mismatch");
        assert!(matches!(err, SourceError::NoMatch));
    }

    #[test]
    fn rejects_multiple_distinct_ids_for_the_same_name() {
        let value: Value = serde_json::from_str(MATCH_AMBIGUOUS_IDS).unwrap();
        let hits = current_records_to_hits(
            &value,
            "acct-fixture-1",
            "Example Manufacturing AG",
            true,
            Some("DE"),
        );
        assert_eq!(hits.len(), 2);
        let err = unique_identity_hits(hits).expect_err("ambiguous ids");
        match err {
            SourceError::Other(inner) => {
                assert!(inner.to_string().contains("ambiguous_company_identity"));
            }
            other => panic!("unexpected {other:?}"),
        }
    }

    #[test]
    fn current_api_does_not_silently_select_ambiguous_ids() {
        let mock = MockApi::spawn(|req| {
            if req.path.starts_with("/v1/companies/match") {
                return (200, MATCH_AMBIGUOUS_IDS.to_string());
            }
            panic!("must not fall through after ambiguous match {}", req.path);
        });
        let root = TestRoot::new();
        root.write_env(&[(ACCOUNT_ID_KEY, "acct-fixture-1")]);
        let err = fetch_current(&root.ctx(), "Example Manufacturing AG", &mock.transport())
            .expect("engages")
            .expect_err("ambiguous");
        match err {
            SourceError::Other(inner) => {
                assert!(inner.to_string().contains("ambiguous_company_identity"));
            }
            other => panic!("unexpected {other:?}"),
        }
    }

    #[test]
    fn extract_from_hits_does_not_merge_distinct_ids() {
        let ctx = SourceCtx {
            root: Path::new("/tmp/ctox-nonexistent-leadfeeder"),
            country: Some(Country::De),
            mode: ResearchMode::NewRecord,
        };
        let hits = vec![
            SourceHit {
                title: "Example Manufacturing AG".to_string(),
                url:
                    "https://api.leadfeeder.com/v1/companies/co-fixture-1?account_id=acct-fixture-1"
                        .to_string(),
                snippet: encode_hit_fields(
                    "co-fixture-1",
                    Some("DE"),
                    "example-manufacturing.test",
                    "101-500",
                    "",
                ),
            },
            SourceHit {
                title: "Example Manufacturing AG".to_string(),
                url:
                    "https://api.leadfeeder.com/v1/companies/co-fixture-2?account_id=acct-fixture-1"
                        .to_string(),
                snippet: encode_hit_fields(
                    "co-fixture-2",
                    Some("DE"),
                    "example-manufacturing-alt.test",
                    "51-200",
                    "",
                ),
            },
        ];
        let fields = module().extract_from_hits(&ctx, "Example Manufacturing AG", &hits);
        assert!(fields.is_empty());
    }

    #[test]
    #[ignore = "live network; run with: cargo test -p ctox-web-stack -- --ignored sources::leadfeeder"]
    fn live_credential_missing_or_smoke() {
        let ctx = SourceCtx {
            root: Path::new("/tmp/ctox-leadfeeder-live"),
            country: Some(Country::De),
            mode: ResearchMode::NewRecord,
        };
        let result = module()
            .fetch_direct(&ctx, "Example Manufacturing AG")
            .expect("DACH context engages");
        match result {
            Err(SourceError::Other(inner)) => {
                assert_eq!(
                    inner.to_string(),
                    CredentialResolveError::Unavailable.to_string()
                );
            }
            other => panic!("live path without resolver must be unavailable, got: {other:?}"),
        }
    }
}
