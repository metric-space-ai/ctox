//! `app.dnbhoovers.com` — Tier C, DACH (DE/AT/CH).
//!
//! D&B Direct+ API: WZ-Code (Branche), Umsatz, Mitarbeiterzahl, Firmen-E-Mail.
//! Requires tenant subscription token (`DNB_DIRECT_API_KEY` im CTOX
//! Secret-/Runtime-Store).
//!
//! ## Endpoints (dokumentiert unter `developer.dnb.com`)
//!
//! * `POST https://plus.dnb.com/v3/token`
//!   OAuth Client Credentials → Bearer-Token. Body
//!   `{ "grant_type": "client_credentials" }`, `Authorization: Basic
//!   <base64(client_id:client_secret)>`.
//!   Antwort: `{ "access_token": "...", "expiresIn": 86400 }`.
//!   In CTOX akzeptieren wir vereinfachend einen vorab-gerefreshten Bearer
//!   im Secret-Store (`DNB_DIRECT_API_KEY = "<token>"`) ODER ein
//!   `client_id:client_secret`-Tupel, das wir vor Ort tauschen.
//! * `GET https://plus.dnb.com/v1/search/companyList?searchTerm=<firma>&countryISOAlpha2Code=<ISO>`
//!   Suche nach Firmenname. Antwort enthält `searchCandidates[].organization`.
//! * `GET https://plus.dnb.com/v1/data/duns/<DUNS>`
//!   Detail-Antwort mit `primaryIndustryCode`, `financials[*]`,
//!   `numberOfEmployees[*]` / `employeeFigures.numberOfEmployees`, `email[*]`.
//!
//! ## Felder + Confidence
//!
//! Pro Excel-Quellenmatrix ist D&B Hoovers autoritativ für `wz_code`,
//! `umsatz`, `mitarbeiter` (alle High — strukturierter API-Output) und
//! `firma_email` (Medium — D&B-Datensätze haben gemeldete Generic-Mailboxen,
//! die in der Praxis oft veraltet sind).

use std::time::Duration;

use anyhow::anyhow;
use serde_json::Value;

use super::{
    Confidence, Country, FieldEvidence, FieldKey, ShapedQuery, SourceCtx, SourceError, SourceHit,
    SourceModule, SourceReadResult, Tier,
};
use crate::runtime_config;

const API_BASE: &str = "https://plus.dnb.com/v1";
const TOKEN_ENDPOINT: &str = "https://plus.dnb.com/v3/token";
const PROFILE_BASE: &str = "https://plus.dnb.com/data/duns";
const SECRET_NAME: &str = "DNB_DIRECT_API_KEY";
const MAX_HITS: usize = 8;
const TIMEOUT_MS: u64 = 12_000;
const USER_AGENT: &str = "ctox-web-stack/0.1 (+https://ctox.local)";

struct DnbHoovers;

impl SourceModule for DnbHoovers {
    fn id(&self) -> &'static str {
        "dnbhoovers.com"
    }

    fn aliases(&self) -> &'static [&'static str] {
        &["dnb", "dnbhoovers", "hoovers"]
    }

    fn host_suffixes(&self) -> &'static [&'static str] {
        // Token-Endpoint und Direct+ API leben unter `plus.dnb.com`.
        &["plus.dnb.com"]
    }

    fn tier(&self) -> Tier {
        Tier::C
    }

    fn countries(&self) -> &'static [Country] {
        &[Country::De, Country::At, Country::Ch]
    }

    fn authoritative_for(&self) -> &'static [FieldKey] {
        &[
            FieldKey::WzCode,
            FieldKey::Umsatz,
            FieldKey::Mitarbeiter,
            FieldKey::FirmaEmail,
        ]
    }

    fn requires_credential(&self) -> Option<&'static str> {
        Some(SECRET_NAME)
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
        // Nicht-DACH-Tenants signalisieren wir mit `None` zurück, damit der
        // Orchestrator die Quelle still überspringt.
        if matches!(
            ctx.country,
            Some(country) if !matches!(country, Country::De | Country::At | Country::Ch)
        ) {
            return None;
        }

        let trimmed = company.trim();
        if trimmed.is_empty() {
            return Some(Err(SourceError::NoMatch));
        }

        let token = match resolve_token(ctx) {
            Some(t) => t,
            None => {
                return Some(Err(SourceError::CredentialMissing {
                    secret_name: SECRET_NAME,
                }));
            }
        };

        let bearer = match exchange_or_passthrough_token(&token) {
            Ok(b) => b,
            Err(err) => return Some(Err(err)),
        };

        let agent = build_agent();
        let iso = ctx.country.map(|c| c.as_iso()).unwrap_or("DE");
        Some(perform_search(&agent, &bearer, trimmed, iso))
    }

    fn extract_fields(&self, page: &SourceReadResult) -> Vec<(FieldKey, FieldEvidence)> {
        let trimmed = page.text.trim_start();
        if !trimmed.starts_with('{') {
            return Vec::new();
        }
        let value: Value = match serde_json::from_str(trimmed) {
            Ok(v) => v,
            Err(_) => return Vec::new(),
        };
        let organization = match value.get("organization") {
            Some(org) if org.is_object() => org,
            _ => return Vec::new(),
        };
        extract_from_organization(organization, &page.url)
    }
}

// ---------------------------------------------------------------------------
// Credential resolution
// ---------------------------------------------------------------------------

/// Liest den Token bevorzugt aus dem (verschlüsselten) Secret-Store über die
/// `ctox secret get`-CLI; als Fallback aus dem unverschlüsselten Runtime-Env.
/// Beides ist pragmatisch — der Sub-Agent kann beides setzen und der
/// orchestrator-Pfad bleibt funktional, auch wenn nur eine Quelle bedient
/// ist.
fn resolve_token(ctx: &SourceCtx<'_>) -> Option<String> {
    if let Some(value) = runtime_config::get(ctx.root, SECRET_NAME) {
        let trimmed = value.trim();
        if !trimmed.is_empty() {
            return Some(trimmed.to_string());
        }
    }
    None
}

/// Akzeptiert zwei Eingabeformate für `DNB_DIRECT_API_KEY`:
///
/// * `"<bearer-token>"` — bereits gültiger Access-Token, direkt nutzbar.
/// * `"<client_id>:<client_secret>"` — wird gegen `/v3/token` getauscht.
///
/// Heuristik: enthält der Wert genau einen `:` und KEINE Punkte (Punkt ist
/// das Trennzeichen in JWT-/JWS-Tokens), interpretieren wir ihn als
/// Client-Credentials. Sonst durchreichen.
fn exchange_or_passthrough_token(raw: &str) -> Result<String, SourceError> {
    let looks_like_credentials =
        raw.matches(':').count() == 1 && !raw.contains('.') && !raw.contains(' ');
    if !looks_like_credentials {
        return Ok(raw.to_string());
    }
    let (client_id, client_secret) = raw.split_once(':').ok_or_else(|| SourceError::Other(
        anyhow!("DNB_DIRECT_API_KEY: client credentials malformed"),
    ))?;
    if client_id.is_empty() || client_secret.is_empty() {
        return Err(SourceError::Other(anyhow!(
            "DNB_DIRECT_API_KEY: empty client_id or client_secret"
        )));
    }
    exchange_client_credentials(client_id, client_secret)
}

fn exchange_client_credentials(client_id: &str, client_secret: &str) -> Result<String, SourceError> {
    use std::io::Read;
    let agent = build_agent();
    let basic = base64_encode(format!("{client_id}:{client_secret}").as_bytes());
    let response = agent
        .post(TOKEN_ENDPOINT)
        .set("authorization", &format!("Basic {basic}"))
        .set("content-type", "application/json")
        .set("accept", "application/json")
        .send_string(r#"{"grant_type":"client_credentials"}"#);
    let response = match response {
        Ok(r) => r,
        Err(ureq::Error::Status(status, resp)) => {
            return Err(classify_status(status, resp));
        }
        Err(err) => return Err(SourceError::Network(anyhow!(err))),
    };
    let mut buf = String::new();
    response
        .into_reader()
        .take(64 * 1024)
        .read_to_string(&mut buf)
        .map_err(|err| SourceError::Network(anyhow!(err)))?;
    let value: Value = serde_json::from_str(&buf).map_err(|err| SourceError::ParseFailed {
        detail: err.to_string(),
    })?;
    value
        .get("access_token")
        .and_then(Value::as_str)
        .map(str::to_string)
        .ok_or_else(|| SourceError::ParseFailed {
            detail: "token endpoint missing access_token".to_string(),
        })
}

// Minimaler Base64-Encoder. Bewusst lokal, um keinen Crate-Dep zu
// erzwingen — die Aufruf-Site ist eine einzelne kurze Zeichenfolge.
fn base64_encode(input: &[u8]) -> String {
    const ALPHABET: &[u8; 64] =
        b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity((input.len() + 2) / 3 * 4);
    let chunks = input.chunks(3);
    for chunk in chunks {
        let b0 = chunk[0];
        let b1 = if chunk.len() > 1 { chunk[1] } else { 0 };
        let b2 = if chunk.len() > 2 { chunk[2] } else { 0 };
        out.push(ALPHABET[(b0 >> 2) as usize] as char);
        out.push(ALPHABET[(((b0 & 0b11) << 4) | (b1 >> 4)) as usize] as char);
        if chunk.len() > 1 {
            out.push(ALPHABET[(((b1 & 0b1111) << 2) | (b2 >> 6)) as usize] as char);
        } else {
            out.push('=');
        }
        if chunk.len() > 2 {
            out.push(ALPHABET[(b2 & 0b111111) as usize] as char);
        } else {
            out.push('=');
        }
    }
    out
}

// ---------------------------------------------------------------------------
// HTTP — Search
// ---------------------------------------------------------------------------

fn build_agent() -> ureq::Agent {
    ureq::AgentBuilder::new()
        .user_agent(USER_AGENT)
        .timeout(Duration::from_millis(TIMEOUT_MS))
        .build()
}

fn perform_search(
    agent: &ureq::Agent,
    bearer: &str,
    company: &str,
    country_iso: &str,
) -> Result<Vec<SourceHit>, SourceError> {
    let url = format!("{API_BASE}/search/companyList");
    let response = agent
        .get(&url)
        .set("authorization", &format!("Bearer {bearer}"))
        .set("accept", "application/json")
        .query("searchTerm", company)
        .query("countryISOAlpha2Code", country_iso)
        .call();

    let response = match response {
        Ok(r) => r,
        Err(ureq::Error::Status(status, resp)) => return Err(classify_status(status, resp)),
        Err(err) => return Err(SourceError::Network(anyhow!(err))),
    };
    let text = response
        .into_string()
        .map_err(|err| SourceError::Network(anyhow!(err)))?;
    let value: Value = serde_json::from_str(&text).map_err(|err| SourceError::ParseFailed {
        detail: err.to_string(),
    })?;
    parse_search_hits(&value)
}

fn classify_status(status: u16, resp: ureq::Response) -> SourceError {
    match status {
        401 | 403 => SourceError::CredentialMissing {
            secret_name: SECRET_NAME,
        },
        429 => {
            let retry = resp
                .header("retry-after")
                .and_then(|v| v.parse::<u64>().ok())
                .map(|secs| secs.saturating_mul(1_000));
            SourceError::RateLimited {
                retry_after_ms: retry,
            }
        }
        400 | 404 => SourceError::NoMatch,
        _ => {
            let detail = resp
                .into_string()
                .unwrap_or_else(|_| format!("http {status}"));
            SourceError::Other(anyhow!("dnbhoovers http {status}: {detail}"))
        }
    }
}

// ---------------------------------------------------------------------------
// Parsing: Search response
// ---------------------------------------------------------------------------

fn parse_search_hits(value: &Value) -> Result<Vec<SourceHit>, SourceError> {
    let candidates =
        value
            .get("searchCandidates")
            .and_then(Value::as_array)
            .ok_or_else(|| SourceError::ParseFailed {
                detail: "missing `searchCandidates` array".to_string(),
            })?;
    if candidates.is_empty() {
        return Err(SourceError::NoMatch);
    }

    let mut hits = Vec::with_capacity(candidates.len().min(MAX_HITS));
    for entry in candidates.iter().take(MAX_HITS) {
        if let Some(hit) = candidate_to_hit(entry) {
            hits.push(hit);
        }
    }
    if hits.is_empty() {
        return Err(SourceError::NoMatch);
    }
    Ok(hits)
}

fn candidate_to_hit(entry: &Value) -> Option<SourceHit> {
    let org = entry.get("organization")?;
    let name = org
        .get("primaryName")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|s| !s.is_empty())?;
    let duns = entry
        .get("displayedDuns")
        .and_then(Value::as_str)
        .or_else(|| org.get("duns").and_then(Value::as_str))
        .map(str::trim)
        .filter(|s| !s.is_empty())?;

    let url = format!("{PROFILE_BASE}/{duns}");

    let mut snippet_parts: Vec<String> = Vec::new();
    snippet_parts.push(format!("DUNS {duns}"));
    if let Some(addr) = org.get("primaryAddress") {
        let locality = addr
            .get("addressLocality")
            .and_then(|l| l.get("name"))
            .and_then(Value::as_str)
            .unwrap_or("");
        let country = addr
            .get("addressCountry")
            .and_then(|c| c.get("isoAlpha2Code"))
            .and_then(Value::as_str)
            .unwrap_or("");
        let seat = match (locality.trim(), country.trim()) {
            ("", "") => String::new(),
            (loc, "") => loc.to_string(),
            ("", c) => c.to_string(),
            (loc, c) => format!("{loc}, {c}"),
        };
        if !seat.is_empty() {
            snippet_parts.push(seat);
        }
    }
    if let Some(industry) = org
        .get("primaryIndustryCode")
        .and_then(|i| i.get("usSicV4Description"))
        .and_then(Value::as_str)
    {
        let industry = industry.trim();
        if !industry.is_empty() {
            snippet_parts.push(industry.to_string());
        }
    }

    Some(SourceHit {
        title: name.to_string(),
        url,
        snippet: snippet_parts.join(" · "),
    })
}

// ---------------------------------------------------------------------------
// Parsing: Detail (organization)
// ---------------------------------------------------------------------------

fn extract_from_organization(org: &Value, source_url: &str) -> Vec<(FieldKey, FieldEvidence)> {
    let mut out = Vec::new();
    let url = source_url.to_string();

    // wz_code — bevorzugt NACE Rev. 2 (das ist die WZ-Code-äquivalente
    // europäische Klassifikation); Fallback auf SIC v4. High Confidence, der
    // Wert ist eine harte Klassifikation, kein Freitext.
    if let Some(industry) = pick_industry_code(org) {
        push_field(
            &mut out,
            FieldKey::WzCode,
            &industry.code,
            &url,
            Confidence::High,
            Some(industry.note),
        );
    }

    // umsatz — D&B liefert `financials[].yearlyRevenue[]` und teils
    // `financials[].salesRevenue`. Wir nehmen den ersten Eintrag mit
    // Information-Scope "Consolidated" und priorisieren EUR vor USD.
    if let Some(revenue) = pick_yearly_revenue(org) {
        push_field(
            &mut out,
            FieldKey::Umsatz,
            &revenue.value,
            &url,
            Confidence::High,
            Some(revenue.note),
        );
    }

    // mitarbeiter — `employeeFigures.numberOfEmployees` ist die kanonische
    // Stelle; Fallback auf `numberOfEmployees[]` mit "Consolidated"-Scope.
    if let Some(employees) = pick_employees(org) {
        push_field(
            &mut out,
            FieldKey::Mitarbeiter,
            &employees.value,
            &url,
            Confidence::High,
            Some(employees.note),
        );
    }

    // firma_email — `email[].address`; Medium Confidence, weil D&B-Mailboxen
    // typischerweise generische "info@"-Adressen sind, die in der Praxis
    // veralten.
    if let Some(email) = pick_email(org) {
        push_field(
            &mut out,
            FieldKey::FirmaEmail,
            &email,
            &url,
            Confidence::Medium,
            None,
        );
    }

    out
}

struct IndustryPick {
    code: String,
    note: String,
}

fn pick_industry_code(org: &Value) -> Option<IndustryPick> {
    // 1) `industryCodes[]` durchsuchen: bevorzuge NACE Rev. 2; sonst
    //    Eintrag mit niedrigster `priority`.
    if let Some(arr) = org.get("industryCodes").and_then(Value::as_array) {
        let mut nace: Option<&Value> = None;
        let mut first: Option<&Value> = None;
        let mut best_priority: i64 = i64::MAX;
        for entry in arr {
            let type_desc = entry
                .get("typeDescription")
                .and_then(Value::as_str)
                .unwrap_or("");
            if type_desc.eq_ignore_ascii_case("NACE Revision 2")
                || type_desc.eq_ignore_ascii_case("NACE Rev. 2")
            {
                nace = Some(entry);
            }
            let prio = entry
                .get("priority")
                .and_then(Value::as_i64)
                .unwrap_or(i64::MAX);
            if prio < best_priority {
                best_priority = prio;
                first = Some(entry);
            }
        }
        if let Some(entry) = nace.or(first) {
            let code = entry
                .get("code")
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|s| !s.is_empty())?;
            let desc = entry.get("description").and_then(Value::as_str).unwrap_or("");
            let scheme = entry
                .get("typeDescription")
                .and_then(Value::as_str)
                .unwrap_or("industry code");
            return Some(IndustryPick {
                code: code.to_string(),
                note: if desc.is_empty() {
                    scheme.to_string()
                } else {
                    format!("{scheme}: {desc}")
                },
            });
        }
    }
    // 2) Fallback: `primaryIndustryCode.usSicV4`.
    if let Some(prim) = org.get("primaryIndustryCode") {
        if let Some(code) = prim.get("usSicV4").and_then(Value::as_str).map(str::trim) {
            if !code.is_empty() {
                let desc = prim
                    .get("usSicV4Description")
                    .and_then(Value::as_str)
                    .unwrap_or("");
                return Some(IndustryPick {
                    code: code.to_string(),
                    note: if desc.is_empty() {
                        "SIC v4".to_string()
                    } else {
                        format!("SIC v4: {desc}")
                    },
                });
            }
        }
    }
    None
}

struct RevenuePick {
    value: String,
    note: String,
}

fn pick_yearly_revenue(org: &Value) -> Option<RevenuePick> {
    let financials = org.get("financials").and_then(Value::as_array)?;
    // 1) Consolidated zuerst, sonst erster Eintrag.
    let consolidated = financials.iter().find(|f| {
        f.get("informationScopeDescription")
            .and_then(Value::as_str)
            .map(|s| s.eq_ignore_ascii_case("Consolidated"))
            .unwrap_or(false)
    });
    let entry = consolidated.or_else(|| financials.first())?;
    let to_date = entry
        .get("financialStatementToDate")
        .and_then(Value::as_str)
        .unwrap_or("");
    let scope = entry
        .get("informationScopeDescription")
        .and_then(Value::as_str)
        .unwrap_or("");
    // a) `yearlyRevenue[]` — bevorzuge EUR > USD > erster Eintrag.
    let mut chosen: Option<(f64, &str)> = None;
    if let Some(arr) = entry.get("yearlyRevenue").and_then(Value::as_array) {
        let mut eur: Option<(f64, &str)> = None;
        let mut usd: Option<(f64, &str)> = None;
        let mut first: Option<(f64, &str)> = None;
        for rev in arr {
            let value = rev.get("value").and_then(Value::as_f64);
            let currency = rev.get("currency").and_then(Value::as_str).unwrap_or("");
            if let Some(v) = value {
                if first.is_none() {
                    first = Some((v, currency));
                }
                match currency {
                    "EUR" => {
                        eur = Some((v, currency));
                    }
                    "USD" => {
                        usd = Some((v, currency));
                    }
                    _ => {}
                }
            }
        }
        chosen = eur.or(usd).or(first);
    }
    // b) Fallback: `salesRevenue` + `currency` direkt am Financial-Eintrag.
    if chosen.is_none() {
        if let (Some(val), currency) = (
            entry.get("salesRevenue").and_then(Value::as_f64),
            entry.get("currency").and_then(Value::as_str).unwrap_or(""),
        ) {
            chosen = Some((val, currency));
        }
    }
    let (value, currency) = chosen?;
    let value_str = format_revenue(value);
    let year = to_date
        .split('-')
        .next()
        .filter(|y| y.len() == 4)
        .unwrap_or("");
    let mut note_parts: Vec<String> = Vec::new();
    if !currency.is_empty() {
        note_parts.push(currency.to_string());
    }
    if !year.is_empty() {
        note_parts.push(format!("FY{year}"));
    }
    if !scope.is_empty() {
        note_parts.push(scope.to_string());
    }
    Some(RevenuePick {
        value: value_str,
        note: note_parts.join(" · "),
    })
}

fn format_revenue(value: f64) -> String {
    // Integer-Darstellung ohne wissenschaftliche Notation. D&B-Umsätze sind
    // immer ganzzahlige Cent-Beträge, daher genügt das Trimmen der
    // Fließkomma-Repräsentation.
    if value.is_finite() && value.fract() == 0.0 {
        format!("{}", value as i128)
    } else {
        format!("{value}")
    }
}

struct EmployeesPick {
    value: String,
    note: String,
}

fn pick_employees(org: &Value) -> Option<EmployeesPick> {
    // 1) `employeeFigures.numberOfEmployees` — kanonischer Pfad.
    if let Some(figures) = org.get("employeeFigures") {
        if let Some(value) = figures.get("numberOfEmployees").and_then(Value::as_i64) {
            let scope = figures
                .get("informationScopeDescription")
                .and_then(Value::as_str)
                .unwrap_or("");
            let reliability = figures
                .get("reliabilityDescription")
                .and_then(Value::as_str)
                .unwrap_or("");
            let mut note: Vec<&str> = Vec::new();
            if !scope.is_empty() {
                note.push(scope);
            }
            if !reliability.is_empty() {
                note.push(reliability);
            }
            return Some(EmployeesPick {
                value: value.to_string(),
                note: note.join(" · "),
            });
        }
    }
    // 2) `numberOfEmployees[]` — bevorzuge Consolidated.
    if let Some(arr) = org.get("numberOfEmployees").and_then(Value::as_array) {
        let consolidated = arr.iter().find(|e| {
            e.get("informationScopeDescription")
                .and_then(Value::as_str)
                .map(|s| s.eq_ignore_ascii_case("Consolidated"))
                .unwrap_or(false)
        });
        let entry = consolidated.or_else(|| arr.first())?;
        let value = entry.get("value").and_then(Value::as_i64)?;
        let scope = entry
            .get("informationScopeDescription")
            .and_then(Value::as_str)
            .unwrap_or("");
        let reliability = entry
            .get("reliabilityDescription")
            .and_then(Value::as_str)
            .unwrap_or("");
        let mut note: Vec<&str> = Vec::new();
        if !scope.is_empty() {
            note.push(scope);
        }
        if !reliability.is_empty() {
            note.push(reliability);
        }
        return Some(EmployeesPick {
            value: value.to_string(),
            note: note.join(" · "),
        });
    }
    None
}

fn pick_email(org: &Value) -> Option<String> {
    let arr = org.get("email").and_then(Value::as_array)?;
    for entry in arr {
        if let Some(addr) = entry.get("address").and_then(Value::as_str) {
            let addr = addr.trim();
            if !addr.is_empty() && addr.contains('@') {
                return Some(addr.to_string());
            }
        }
    }
    None
}

// ---------------------------------------------------------------------------
// Field-evidence helper
// ---------------------------------------------------------------------------

fn push_field(
    out: &mut Vec<(FieldKey, FieldEvidence)>,
    key: FieldKey,
    value: &str,
    url: &str,
    confidence: Confidence,
    note: Option<String>,
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
            note: note.filter(|n| !n.trim().is_empty()),
        },
    ));
}

// ---------------------------------------------------------------------------
// Registry hook
// ---------------------------------------------------------------------------

static MODULE: DnbHoovers = DnbHoovers;

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

    const SEARCH_FIXTURE: &str =
        include_str!("../../fixtures/sources/dnbhoovers/search_sap.json");
    const DETAIL_FIXTURE: &str =
        include_str!("../../fixtures/sources/dnbhoovers/detail_sap.json");

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
        assert_eq!(m.id(), "dnbhoovers.com");
        assert_eq!(m.aliases(), &["dnb", "dnbhoovers", "hoovers"]);
        assert!(matches!(m.tier(), Tier::C));
        assert_eq!(m.countries(), &[Country::De, Country::At, Country::Ch]);
        assert_eq!(m.requires_credential(), Some(SECRET_NAME));
        for required in [
            FieldKey::WzCode,
            FieldKey::Umsatz,
            FieldKey::Mitarbeiter,
            FieldKey::FirmaEmail,
        ] {
            assert!(
                m.authoritative_for().contains(&required),
                "missing authoritative field: {required:?}"
            );
        }
    }

    #[test]
    fn shape_query_is_none_for_api_source() {
        let ctx = SourceCtx {
            root: Path::new("/tmp/ctox-test-dnb"),
            country: Some(Country::De),
            mode: ResearchMode::NewRecord,
        };
        assert!(module().shape_query("SAP SE", &ctx).is_none());
    }

    #[test]
    fn fetch_direct_skips_non_dach_countries() {
        // The enum only contains DACH today; this test guards against a
        // future Country variant accidentally engaging the D&B path.
        // We verify behavior for known DACH countries: they DO engage and
        // (without a token) short-circuit to CredentialMissing.
        for country in [Country::De, Country::At, Country::Ch] {
            let ctx = SourceCtx {
                root: Path::new("/tmp/ctox-test-dnb-no-token"),
                country: Some(country),
                mode: ResearchMode::NewRecord,
            };
            let result = module()
                .fetch_direct(&ctx, "SAP SE")
                .expect("DACH context must engage");
            assert!(
                matches!(
                    result,
                    Err(SourceError::CredentialMissing {
                        secret_name: SECRET_NAME
                    })
                ),
                "expected credential_missing, got: {result:?}"
            );
        }
    }

    #[test]
    fn fetch_direct_empty_company_is_no_match() {
        let ctx = SourceCtx {
            root: Path::new("/tmp/ctox-test-dnb"),
            country: Some(Country::De),
            mode: ResearchMode::NewRecord,
        };
        let result = module()
            .fetch_direct(&ctx, "   ")
            .expect("DACH context must engage");
        assert!(matches!(result, Err(SourceError::NoMatch)));
    }

    #[test]
    fn parses_search_fixture_into_hits() {
        let value: Value = serde_json::from_str(SEARCH_FIXTURE).expect("fixture json");
        let hits = parse_search_hits(&value).expect("fixture has hits");
        assert_eq!(hits.len(), 1);
        let sap = &hits[0];
        assert_eq!(sap.title, "SAP SE");
        assert_eq!(sap.url, "https://plus.dnb.com/data/duns/316840271");
        assert!(sap.snippet.contains("DUNS 316840271"));
        assert!(sap.snippet.contains("Walldorf"));
        assert!(sap.snippet.contains("Prepackaged Software"));
    }

    #[test]
    fn parses_search_fixture_caps_hits_at_max() {
        let value: Value = serde_json::from_str(SEARCH_FIXTURE).expect("fixture json");
        let hits = parse_search_hits(&value).expect("hits");
        assert!(hits.len() <= MAX_HITS);
    }

    #[test]
    fn empty_search_candidates_maps_to_no_match() {
        let value: Value = serde_json::from_str(r#"{"searchCandidates": []}"#).unwrap();
        let err = parse_search_hits(&value).unwrap_err();
        assert!(matches!(err, SourceError::NoMatch));
    }

    #[test]
    fn missing_candidates_field_maps_to_parse_failed() {
        let value: Value = serde_json::from_str(r#"{"error": "boom"}"#).unwrap();
        let err = parse_search_hits(&value).unwrap_err();
        assert!(matches!(err, SourceError::ParseFailed { .. }));
    }

    #[test]
    fn extract_fields_pulls_wz_code_high() {
        let page = dummy_page(DETAIL_FIXTURE, "https://plus.dnb.com/data/duns/316840271");
        let fields = module().extract_fields(&page);
        let wz = fields
            .iter()
            .find(|(k, _)| matches!(k, FieldKey::WzCode))
            .expect("wz_code");
        // The fixture has BOTH SIC and NACE Rev. 2; we prefer the NACE entry
        // because that is the European equivalent of the WZ-Code.
        assert_eq!(wz.1.value, "5820");
        assert!(matches!(wz.1.confidence, Confidence::High));
        assert!(wz
            .1
            .note
            .as_deref()
            .unwrap_or("")
            .contains("NACE Revision 2"));
    }

    #[test]
    fn extract_fields_pulls_umsatz_high_with_currency_and_year() {
        let page = dummy_page(DETAIL_FIXTURE, "https://plus.dnb.com/data/duns/316840271");
        let fields = module().extract_fields(&page);
        let revenue = fields
            .iter()
            .find(|(k, _)| matches!(k, FieldKey::Umsatz))
            .expect("umsatz");
        // EUR is preferred over USD.
        assert_eq!(revenue.1.value, "30287000000");
        assert!(matches!(revenue.1.confidence, Confidence::High));
        let note = revenue.1.note.as_deref().unwrap_or("");
        assert!(note.contains("EUR"), "currency note missing: {note}");
        assert!(note.contains("FY2024"), "year note missing: {note}");
    }

    #[test]
    fn extract_fields_pulls_mitarbeiter_high() {
        let page = dummy_page(DETAIL_FIXTURE, "https://plus.dnb.com/data/duns/316840271");
        let fields = module().extract_fields(&page);
        let employees = fields
            .iter()
            .find(|(k, _)| matches!(k, FieldKey::Mitarbeiter))
            .expect("mitarbeiter");
        assert_eq!(employees.1.value, "107602");
        assert!(matches!(employees.1.confidence, Confidence::High));
        assert!(employees
            .1
            .note
            .as_deref()
            .unwrap_or("")
            .contains("Consolidated"));
    }

    #[test]
    fn extract_fields_pulls_firma_email_medium() {
        let page = dummy_page(DETAIL_FIXTURE, "https://plus.dnb.com/data/duns/316840271");
        let fields = module().extract_fields(&page);
        let email = fields
            .iter()
            .find(|(k, _)| matches!(k, FieldKey::FirmaEmail))
            .expect("firma_email");
        assert_eq!(email.1.value, "info@sap.com");
        assert!(matches!(email.1.confidence, Confidence::Medium));
    }

    #[test]
    fn extract_fields_falls_back_to_employees_array_when_figures_block_absent() {
        let body = serde_json::json!({
            "organization": {
                "numberOfEmployees": [
                    {
                        "value": 12345,
                        "informationScopeDescription": "Consolidated",
                        "reliabilityDescription": "Estimated"
                    }
                ]
            }
        })
        .to_string();
        let page = dummy_page(&body, "https://plus.dnb.com/data/duns/000000000");
        let fields = module().extract_fields(&page);
        let employees = fields
            .iter()
            .find(|(k, _)| matches!(k, FieldKey::Mitarbeiter))
            .expect("mitarbeiter via fallback array");
        assert_eq!(employees.1.value, "12345");
        assert!(employees
            .1
            .note
            .as_deref()
            .unwrap_or("")
            .contains("Estimated"));
    }

    #[test]
    fn extract_fields_industry_falls_back_to_primary_sic() {
        let body = serde_json::json!({
            "organization": {
                "primaryIndustryCode": {
                    "usSicV4": "7389",
                    "usSicV4Description": "Services-Business Services"
                }
            }
        })
        .to_string();
        let page = dummy_page(&body, "https://plus.dnb.com/data/duns/000000000");
        let fields = module().extract_fields(&page);
        let wz = fields
            .iter()
            .find(|(k, _)| matches!(k, FieldKey::WzCode))
            .expect("wz_code via primary SIC fallback");
        assert_eq!(wz.1.value, "7389");
        assert!(wz.1.note.as_deref().unwrap_or("").contains("SIC v4"));
    }

    #[test]
    fn extract_fields_returns_empty_on_non_json_text() {
        let page = dummy_page("not a json body", "https://plus.dnb.com/data/duns/x");
        assert!(module().extract_fields(&page).is_empty());
    }

    #[test]
    fn extract_fields_returns_empty_when_organization_missing() {
        let page = dummy_page(r#"{"unrelated":true}"#, "https://plus.dnb.com/data/duns/x");
        assert!(module().extract_fields(&page).is_empty());
    }

    #[test]
    fn base64_encode_matches_rfc4648_vectors() {
        assert_eq!(base64_encode(b""), "");
        assert_eq!(base64_encode(b"f"), "Zg==");
        assert_eq!(base64_encode(b"fo"), "Zm8=");
        assert_eq!(base64_encode(b"foo"), "Zm9v");
        assert_eq!(base64_encode(b"foob"), "Zm9vYg==");
        assert_eq!(base64_encode(b"fooba"), "Zm9vYmE=");
        assert_eq!(base64_encode(b"foobar"), "Zm9vYmFy");
    }

    #[test]
    #[ignore = "live network; run with: cargo test -p ctox-web-stack -- --ignored sources::dnbhoovers"]
    fn live_credential_missing_or_smoke() {
        // Ohne aktive Subscription auf dem Operator-Host (Default) MUSS der
        // Aufruf `credential_missing` zurückgeben. Mit gesetztem
        // `DNB_DIRECT_API_KEY` im Runtime-Store darf er stattdessen einen
        // Treffer oder `NoMatch` liefern — beides ist OK.
        let ctx = SourceCtx {
            root: Path::new("/tmp/ctox-test-dnb-live"),
            country: Some(Country::De),
            mode: ResearchMode::NewRecord,
        };
        let result = module()
            .fetch_direct(&ctx, "SAP SE")
            .expect("DACH context must engage");
        match result {
            Err(SourceError::CredentialMissing { secret_name }) => {
                assert_eq!(secret_name, SECRET_NAME);
            }
            Ok(hits) => {
                assert!(!hits.is_empty(), "live API returned an empty hit list");
                for hit in hits {
                    assert!(hit.url.starts_with(PROFILE_BASE));
                }
            }
            Err(other) => panic!("unexpected error: {other}"),
        }
    }
}
