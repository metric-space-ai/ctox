//! `leadfeeder.com` — Tier C, DACH.
//!
//! Leadfeeder (heute Dealfront Leadfeeder) ist ein Visitor-Identification-
//! Werkzeug, das B2B-Webseitenbesucher zu Firmen und Kontakten zuordnet.
//! Im CTOX-Webstack ist Leadfeeder die einzige Quelle, die die
//! Thesen-Quellenmatrix (`EXCEL_MATRIX.md`) für `person_email` in
//! Deutschland, Österreich und der Schweiz listet, und sie taucht als
//! Sekundär-Quelle für `firma_email` und `firma_domain` auf.
//!
//! ## Endpoints
//!
//! Leadfeeder exponiert eine versionierte REST-API unter
//! `https://api.leadfeeder.com/`. Authentifizierung ist ein API-Token im
//! `Authorization`-Header (Format: `Token token=<key>`). Beide Endpoints,
//! die wir hier benutzen, hängen am gewählten Account:
//!
//! * `GET https://api.leadfeeder.com/accounts/<account_id>/leads?company_name=<firma>`
//!   liefert Firmen-Stammdaten (`website_url`, `email`, `industry`,
//!   `employee_count`).
//! * `GET https://api.leadfeeder.com/accounts/<account_id>/contacts?search=<firma>`
//!   liefert Kontakte (`name`, `email`, `title`).
//!
//! Beide Antworten folgen lose dem JSON:API-Format: eine Liste unter
//! `data[]`, jedes Element mit `id`, `type`, und einem `attributes`-Block.
//!
//! ## Credentials
//!
//! Der API-Key wird über die SQLite-Runtime-Config gelesen
//! (`runtime_env_kv`-Tabelle, Key `LEADFEEDER_API_KEY`). Der Account-Id
//! kommt aus derselben Tabelle unter `LEADFEEDER_ACCOUNT_ID`; fehlt sie,
//! verwenden wir das dokumentierte `me` (das die API auf den eigenen
//! Default-Account auflöst), damit ein Single-Account-Tenant out-of-the-box
//! funktioniert. Ohne Token gibt `fetch_direct` ein
//! `CredentialMissing { secret_name: "LEADFEEDER_API_KEY" }` zurück und der
//! Orchestrator probiert die nächste Quelle in der Priority-Liste.
//!
//! ## Confidence
//!
//! * `firma_email`, `firma_domain` — `High`. Beides sind strukturierte
//!   Pflicht-/Schlüsselfelder eines Leadfeeder-Leads und werden nicht
//!   heuristisch hergeleitet.
//! * `person_email` — `Medium`. Leadfeeder kombiniert verifizierte Mails
//!   mit „guessed"-Mails (z. B. aus dem Domain-Muster); für die
//!   Aussenwelt bleibt das eine Medium-Confidence-Aussage.

use std::time::Duration;

use anyhow::anyhow;
use serde_json::Value;

use super::{
    Confidence, Country, FieldEvidence, FieldKey, ShapedQuery, SourceCtx, SourceError,
    SourceHit, SourceModule, SourceReadResult, Tier,
};
use crate::runtime_config;

const API_BASE: &str = "https://api.leadfeeder.com";
const ACCOUNT_DEFAULT: &str = "me";
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

    fn tier(&self) -> Tier {
        Tier::C
    }

    fn countries(&self) -> &'static [Country] {
        &[Country::De, Country::At, Country::Ch]
    }

    fn authoritative_for(&self) -> &'static [FieldKey] {
        &[
            FieldKey::FirmaEmail,
            FieldKey::FirmaDomain,
            FieldKey::PersonEmail,
        ]
    }

    fn requires_credential(&self) -> Option<&'static str> {
        Some("LEADFEEDER_API_KEY")
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

        let token = match runtime_config::get(ctx.root, "LEADFEEDER_API_KEY") {
            Some(t) => t,
            None => {
                return Some(Err(SourceError::CredentialMissing {
                    secret_name: "LEADFEEDER_API_KEY",
                }));
            }
        };
        let account_id = runtime_config::get(ctx.root, "LEADFEEDER_ACCOUNT_ID")
            .unwrap_or_else(|| ACCOUNT_DEFAULT.to_string());

        let agent = build_agent();
        Some(perform_search(&agent, &token, &account_id, trimmed))
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
        .build()
}

fn auth_header(token: &str) -> String {
    format!("Token token={token}")
}

fn perform_search(
    agent: &ureq::Agent,
    token: &str,
    account_id: &str,
    company: &str,
) -> Result<Vec<SourceHit>, SourceError> {
    let leads = fetch_leads(agent, token, account_id, company)?;
    let contacts = match fetch_contacts(agent, token, account_id, company) {
        Ok(c) => c,
        // Contacts-Endpoint ist optional je nach Subscription-Stufe;
        // ein 403/blocked dort darf den Lead-Pfad nicht killen.
        Err(SourceError::Blocked { .. }) => Value::Null,
        Err(other) => return Err(other),
    };

    let mut hits = leads_to_hits(&leads, account_id);
    hits.extend(contacts_to_hits(&contacts, account_id));

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
    company: &str,
) -> Result<Value, SourceError> {
    let url = format!("{API_BASE}/accounts/{account_id}/leads");
    let response = agent
        .get(&url)
        .set("Authorization", &auth_header(token))
        .set("accept", "application/json")
        .query("company_name", company)
        .call();
    decode_json_response(response)
}

fn fetch_contacts(
    agent: &ureq::Agent,
    token: &str,
    account_id: &str,
    company: &str,
) -> Result<Value, SourceError> {
    let url = format!("{API_BASE}/accounts/{account_id}/contacts");
    let response = agent
        .get(&url)
        .set("Authorization", &auth_header(token))
        .set("accept", "application/json")
        .query("search", company)
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
            secret_name: "LEADFEEDER_API_KEY",
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

fn leads_to_hits(value: &Value, account_id: &str) -> Vec<SourceHit> {
    let mut hits = Vec::new();
    for record in lead_records(value) {
        if let Some(hit) = lead_to_hit(record, account_id) {
            hits.push(hit);
        }
    }
    hits
}

fn contacts_to_hits(value: &Value, account_id: &str) -> Vec<SourceHit> {
    let mut hits = Vec::new();
    for record in lead_records(value) {
        if let Some(hit) = contact_to_hit(record, account_id) {
            hits.push(hit);
        }
    }
    hits
}

fn lead_to_hit(record: &Value, account_id: &str) -> Option<SourceHit> {
    let id = record.get("id").and_then(Value::as_str).unwrap_or("");
    let attrs = record.get("attributes")?;
    let name = attrs.get("name").and_then(Value::as_str).unwrap_or("").trim();
    if name.is_empty() {
        return None;
    }
    let domain = attrs
        .get("website_url")
        .and_then(Value::as_str)
        .map(domain_from_url)
        .unwrap_or_default();
    let industry = attrs
        .get("industry")
        .and_then(Value::as_str)
        .unwrap_or("");
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

fn contact_to_hit(record: &Value, account_id: &str) -> Option<SourceHit> {
    let id = record.get("id").and_then(Value::as_str).unwrap_or("");
    let attrs = record.get("attributes")?;
    let name = attrs.get("name").and_then(Value::as_str).unwrap_or("").trim();
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
    let snippet_parts: Vec<&str> = [title, email].into_iter().filter(|s| !s.is_empty()).collect();
    Some(SourceHit {
        title: name.to_string(),
        url: format!("{API_BASE}/accounts/{account_id}/contacts/{id}"),
        snippet: snippet_parts.join(" · "),
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
        let record_type = record
            .get("type")
            .and_then(Value::as_str)
            .unwrap_or("");
        let attrs = match record.get("attributes") {
            Some(a) => a,
            None => continue,
        };
        match record_type {
            "leads" | "lead" | "companies" | "company" => {
                extract_lead_fields(attrs, &url, &mut out);
            }
            "contacts" | "contact" | "people" | "person" => {
                extract_contact_fields(attrs, &url, &mut out);
            }
            _ => {
                // Unbekannter Typ: bestmöglich beide Pfade probieren.
                extract_lead_fields(attrs, &url, &mut out);
                extract_contact_fields(attrs, &url, &mut out);
            }
        }
    }
    out
}

fn extract_lead_fields(
    attrs: &Value,
    url: &str,
    out: &mut Vec<(FieldKey, FieldEvidence)>,
) {
    if let Some(email) = attrs.get("email").and_then(Value::as_str) {
        let clean = email.trim();
        if looks_like_email(clean) {
            push(out, FieldKey::FirmaEmail, clean, url, Confidence::High);
        }
    }
    if let Some(website) = attrs.get("website_url").and_then(Value::as_str) {
        let domain = domain_from_url(website);
        if !domain.is_empty() {
            push(out, FieldKey::FirmaDomain, &domain, url, Confidence::High);
        }
    } else if let Some(domain) = attrs.get("domain").and_then(Value::as_str) {
        let clean = domain.trim().trim_start_matches("www.");
        if !clean.is_empty() {
            push(out, FieldKey::FirmaDomain, clean, url, Confidence::High);
        }
    }
}

fn extract_contact_fields(
    attrs: &Value,
    url: &str,
    out: &mut Vec<(FieldKey, FieldEvidence)>,
) {
    if let Some(email) = attrs.get("email").and_then(Value::as_str) {
        let clean = email.trim();
        if looks_like_email(clean) {
            push(out, FieldKey::PersonEmail, clean, url, Confidence::Medium);
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

fn looks_like_email(value: &str) -> bool {
    // Sehr kleine Validierung: enthält genau ein '@', mindestens ein '.'
    // im Domain-Teil, keine Whitespaces. Reicht, um leere Strings,
    // `"unknown"` oder Telefon-Nummern auszusortieren.
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
        }
    }

    #[test]
    fn module_metadata() {
        let m = module();
        assert_eq!(m.id(), "leadfeeder.com");
        assert_eq!(m.aliases(), &["leadfeeder", "lf"]);
        assert!(matches!(m.tier(), Tier::C));
        assert_eq!(
            m.countries(),
            &[Country::De, Country::At, Country::Ch]
        );
        assert_eq!(m.requires_credential(), Some("LEADFEEDER_API_KEY"));
        let auth = m.authoritative_for();
        assert!(auth.contains(&FieldKey::FirmaEmail));
        assert!(auth.contains(&FieldKey::FirmaDomain));
        assert!(auth.contains(&FieldKey::PersonEmail));
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
        let result = module().fetch_direct(&ctx, "Wittenstein SE").expect("DACH engages");
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
            !fields.iter().any(|(k, _)| matches!(k, FieldKey::FirmaEmail)),
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
        assert_eq!(domain_from_url("https://www.Wittenstein.de/de"), "wittenstein.de");
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
                assert!(!hits.is_empty(), "live response must contain at least one hit");
            }
            Err(other) => panic!("unexpected live error: {other:?}"),
        }
    }
}
