//! `linkedin.com` — Tier C, DACH (DE / AT / CH).
//!
//! LinkedIn Sales Navigator (Marketing Developer Platform) API. Liefert
//! `person_funktion` und `person_linkedin` mit hoher Konfidenz, plus eine
//! schwache Heuristik für `person_geschlecht` aus dem Vornamen.
//!
//! ## TOS / Scraping
//!
//! LinkedIn ist Tier C **und** Scrape-verboten. Diese Quelle nutzt
//! ausschliesslich den authentifizierten REST-Pfad. Wenn kein Token im
//! CTOX-Runtime-Store hinterlegt ist, gibt `fetch_direct` ein
//! `SourceError::CredentialMissing` zurück — es wird **niemals** auf
//! `https://www.linkedin.com/...` HTML zurückgefallen.
//!
//! ## Endpoints
//!
//! * `GET https://api.linkedin.com/v2/peopleSearch?q=companyName&companyName=<firma>&count=<n>`
//!   Liefert eine Suchergebnisliste (`elements: [Person]`).
//! * `GET https://api.linkedin.com/v2/people/(id:<urn>)`
//!   Liefert das vollständige Personendetail mit `currentPositions`.
//!
//! Auth: `Authorization: Bearer <token>` (OAuth 2.0; Sales-Nav-Partner-
//! Programm-Scope). Der Token liegt im SQLite-Runtime-Store unter
//! `LINKEDIN_SALES_NAV_TOKEN` und wird über
//! [`runtime_config::get`](crate::runtime_config::get) gelesen.
//!
//! ## Extrahierte Felder
//!
//! * `person_funktion` — [`Confidence::High`], aus `currentPositions[0].title`
//!   (Fallback: `headline`).
//! * `person_linkedin` — [`Confidence::High`], `https://www.linkedin.com/in/<publicIdentifier>/`.
//! * `person_geschlecht` — [`Confidence::Low`], heuristisch aus dem Vornamen
//!   (LinkedIn liefert kein Geschlechtsfeld). Nur wenn die Heuristik
//!   eindeutig anschlägt — sonst weggelassen.

use std::time::Duration;

use anyhow::anyhow;
use serde_json::Value;

use super::{
    Confidence, Country, FieldEvidence, FieldKey, ShapedQuery, SourceCtx, SourceError, SourceHit,
    SourceModule, SourceReadResult, Tier,
};
use crate::runtime_config;

const API_BASE: &str = "https://api.linkedin.com/v2";
const PROFILE_BASE: &str = "https://www.linkedin.com/in";
const SECRET_NAME: &str = "LINKEDIN_SALES_NAV_TOKEN";
const MAX_HITS: usize = 10;
const TIMEOUT_MS: u64 = 12_000;
const USER_AGENT: &str = "ctox-web-stack/0.1 (+https://ctox.local)";

struct LinkedIn;

impl SourceModule for LinkedIn {
    fn id(&self) -> &'static str {
        "linkedin.com"
    }

    fn aliases(&self) -> &'static [&'static str] {
        &["linkedin", "li", "sales-nav"]
    }

    fn host_suffixes(&self) -> &'static [&'static str] {
        // Antworten kommen von `api.linkedin.com`, profile-URLs zeigen auf
        // `www.linkedin.com`; beide gehören zu diesem Modul.
        &["api.linkedin.com"]
    }

    fn tier(&self) -> Tier {
        Tier::C
    }

    fn countries(&self) -> &'static [Country] {
        &[Country::De, Country::At, Country::Ch]
    }

    fn authoritative_for(&self) -> &'static [FieldKey] {
        &[
            FieldKey::PersonFunktion,
            FieldKey::PersonGeschlecht,
            FieldKey::PersonLinkedin,
        ]
    }

    fn requires_credential(&self) -> Option<&'static str> {
        Some(SECRET_NAME)
    }

    fn shape_query(&self, _query: &str, _ctx: &SourceCtx<'_>) -> Option<ShapedQuery> {
        // API-Pfad: kein Search-Engine-Fallback. Scrape ist TOS-verboten.
        None
    }

    fn fetch_direct(
        &self,
        ctx: &SourceCtx<'_>,
        company: &str,
    ) -> Option<Result<Vec<SourceHit>, SourceError>> {
        // DACH-only. Andere Länder lassen wir den Orchestrator still
        // überspringen (None statt Err).
        if let Some(country) = ctx.country {
            if !matches!(country, Country::De | Country::At | Country::Ch) {
                return None;
            }
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

        let agent = build_agent();
        Some(perform_people_search(&agent, &token, trimmed))
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
        // Detail-Body (Single-Person-Resource) vs. Search-Body
        // (`elements: [...]`). Wir extrahieren in beiden Fällen aus dem
        // ersten verfügbaren Personen-Objekt; das schliesst sowohl die
        // `/people/(id:...)`-Antwort als auch die typische Konvention
        // `extract_fields` auf einem materialisierten Hit ein.
        let person = pick_first_person(&value);
        let Some(person) = person else {
            return Vec::new();
        };
        extract_from_person(person, &page.url)
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

fn perform_people_search(
    agent: &ureq::Agent,
    token: &str,
    company: &str,
) -> Result<Vec<SourceHit>, SourceError> {
    let url = format!("{API_BASE}/peopleSearch");
    let response = agent
        .get(&url)
        .set("authorization", &format!("Bearer {token}"))
        .set("accept", "application/json")
        .set("x-restli-protocol-version", "2.0.0")
        .query("q", "companyName")
        .query("companyName", company)
        .query("count", &MAX_HITS.to_string())
        .call();

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
    let value: Value = serde_json::from_str(&text).map_err(|err| SourceError::ParseFailed {
        detail: err.to_string(),
    })?;

    parse_search_hits(&value)
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
        401 | 403 => SourceError::Blocked {
            reason: format!("http {status}"),
        },
        400 | 404 => SourceError::NoMatch,
        _ => {
            let detail = resp
                .into_string()
                .unwrap_or_else(|_| format!("http {status}"));
            SourceError::Other(anyhow!("linkedin http {status}: {detail}"))
        }
    }
}

// ---------------------------------------------------------------------------
// Parsing: Search
// ---------------------------------------------------------------------------

fn parse_search_hits(value: &Value) -> Result<Vec<SourceHit>, SourceError> {
    let elements = value
        .get("elements")
        .and_then(Value::as_array)
        .ok_or_else(|| SourceError::ParseFailed {
            detail: "missing `elements` array".to_string(),
        })?;
    if elements.is_empty() {
        return Err(SourceError::NoMatch);
    }

    let mut hits = Vec::with_capacity(elements.len().min(MAX_HITS));
    for entry in elements.iter().take(MAX_HITS) {
        if let Some(hit) = person_to_hit(entry) {
            hits.push(hit);
        }
    }
    if hits.is_empty() {
        return Err(SourceError::NoMatch);
    }
    Ok(hits)
}

fn person_to_hit(entry: &Value) -> Option<SourceHit> {
    let public_id = entry
        .get("publicIdentifier")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|s| !s.is_empty())?;
    let first = read_localized(entry.get("firstName")).unwrap_or_default();
    let last = read_localized(entry.get("lastName")).unwrap_or_default();
    let title = compose_title(&first, &last);
    if title.is_empty() {
        return None;
    }

    let url = format!("{PROFILE_BASE}/{public_id}/");

    let headline = read_localized(entry.get("headline")).unwrap_or_default();
    let position_title = entry
        .get("currentPositions")
        .and_then(Value::as_array)
        .and_then(|a| a.first())
        .and_then(|p| p.get("title"))
        .and_then(Value::as_str)
        .unwrap_or("");
    let company = entry
        .get("currentPositions")
        .and_then(Value::as_array)
        .and_then(|a| a.first())
        .and_then(|p| p.get("companyName"))
        .and_then(Value::as_str)
        .unwrap_or("");

    let mut parts: Vec<&str> = Vec::new();
    if !headline.is_empty() {
        parts.push(&headline);
    } else if !position_title.is_empty() {
        parts.push(position_title);
    }
    if !company.is_empty() && !headline.contains(company) {
        parts.push(company);
    }
    let snippet = parts.join(" · ");

    Some(SourceHit {
        title,
        url,
        snippet,
    })
}

fn compose_title(first: &str, last: &str) -> String {
    let f = first.trim();
    let l = last.trim();
    match (f.is_empty(), l.is_empty()) {
        (true, true) => String::new(),
        (true, false) => l.to_string(),
        (false, true) => f.to_string(),
        (false, false) => format!("{f} {l}"),
    }
}

// ---------------------------------------------------------------------------
// Parsing: Person → fields
// ---------------------------------------------------------------------------

fn pick_first_person(value: &Value) -> Option<&Value> {
    // Direkter Detail-Body: hat `publicIdentifier` selbst.
    if value.get("publicIdentifier").is_some() {
        return Some(value);
    }
    // Such-Body: erstes Element in `elements`.
    if let Some(first) = value
        .get("elements")
        .and_then(Value::as_array)
        .and_then(|a| a.first())
    {
        return Some(first);
    }
    None
}

fn extract_from_person(person: &Value, source_url: &str) -> Vec<(FieldKey, FieldEvidence)> {
    let mut out = Vec::new();

    // `person_linkedin` (High) — bevorzugt aus publicIdentifier, sonst fällt
    // auf die übergebene Page-URL zurück, wenn sie schon ein /in/-Pfad ist.
    let profile_url = person
        .get("publicIdentifier")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(|id| format!("{PROFILE_BASE}/{id}/"))
        .unwrap_or_else(|| {
            if source_url.contains("/in/") {
                source_url.to_string()
            } else {
                String::new()
            }
        });
    if !profile_url.is_empty() {
        push_high(&mut out, FieldKey::PersonLinkedin, &profile_url, source_url);
    }

    // `person_funktion` (High) — `currentPositions[0].title` schlägt
    // `headline`. `headline` bekommt nur das Vor-`bei`-Segment, damit nicht
    // der Firmenname als Funktion durchrutscht.
    let position_title = person
        .get("currentPositions")
        .and_then(Value::as_array)
        .and_then(|a| a.first())
        .and_then(|p| p.get("title"))
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string);

    let funktion = match position_title {
        Some(t) => Some(t),
        None => read_localized(person.get("headline")).and_then(|h| extract_role_from_headline(&h)),
    };
    if let Some(f) = funktion {
        push_high(&mut out, FieldKey::PersonFunktion, &f, source_url);
    }

    // `person_geschlecht` (Low) — Heuristik aus dem deutschen Vornamen.
    if let Some(first) = read_localized(person.get("firstName")) {
        if let Some(g) = guess_gender_from_firstname(&first) {
            push_low(&mut out, FieldKey::PersonGeschlecht, g, source_url);
        }
    }

    out
}

/// LinkedIn-Localized-String: `{ "localized": { "<locale>": "<value>" }, "preferredLocale": ... }`.
/// Wir bevorzugen `preferredLocale`, fallen sonst auf den ersten Wert
/// (Sales-Nav-Bodies haben hin und wieder nur eine Locale).
fn read_localized(value: Option<&Value>) -> Option<String> {
    let v = value?;
    if let Some(s) = v.as_str() {
        let trimmed = s.trim();
        return if trimmed.is_empty() {
            None
        } else {
            Some(trimmed.to_string())
        };
    }
    let localized = v.get("localized")?.as_object()?;
    let preferred_key = v.get("preferredLocale").map(|loc| {
        let language = loc.get("language").and_then(Value::as_str).unwrap_or("");
        let country = loc.get("country").and_then(Value::as_str).unwrap_or("");
        format!("{language}_{country}")
    });
    if let Some(key) = preferred_key {
        if let Some(val) = localized.get(&key).and_then(Value::as_str) {
            let trimmed = val.trim();
            if !trimmed.is_empty() {
                return Some(trimmed.to_string());
            }
        }
    }
    localized
        .values()
        .filter_map(|v| v.as_str())
        .map(str::trim)
        .find(|s| !s.is_empty())
        .map(str::to_string)
}

/// Aus einer `headline` wie "Geschäftsführerin Vertrieb bei WITTENSTEIN SE"
/// das Vor-`bei`/`@`/`|`-Segment ziehen.
fn extract_role_from_headline(headline: &str) -> Option<String> {
    let separators = [" bei ", " at ", " @ ", " | "];
    let mut cut = headline.len();
    for sep in separators {
        if let Some(idx) = headline.find(sep) {
            if idx < cut {
                cut = idx;
            }
        }
    }
    let role = headline[..cut].trim();
    if role.is_empty() {
        None
    } else {
        Some(role.to_string())
    }
}

/// Sehr enge Geschlechtsheuristik aus deutschen Vornamen — bewusst nur die
/// gröbste Endungs-Regel + harte Whitelist. Treffer ist `Confidence::Low`.
/// Wenn das Wörterbuch ambig ist, geben wir `None` zurück; `person_geschlecht`
/// kommt dann aus anderen Quellen (oder bleibt leer).
fn guess_gender_from_firstname(name: &str) -> Option<&'static str> {
    let first_token = name.split_whitespace().next()?.trim_matches(|c: char| {
        c.is_ascii_punctuation() && c != '-' && c != '\''
    });
    if first_token.is_empty() {
        return None;
    }
    let lower = first_token.to_lowercase();
    const FEMALE: &[&str] = &[
        "anna", "anne", "andrea", "barbara", "birgit", "brigitte", "christa",
        "christina", "christine", "claudia", "claire", "daniela", "diana",
        "elisabeth", "elke", "eva", "franziska", "gabriele", "hannah",
        "heike", "helga", "ingrid", "irene", "jana", "jasmin", "johanna",
        "julia", "karin", "katharina", "katja", "kerstin", "klara", "laura",
        "lena", "linda", "lisa", "lubomira", "marie", "maria", "marion",
        "martina", "melanie", "michaela", "monika", "nadine", "nicole",
        "petra", "regina", "renate", "sabine", "sandra", "silke", "simone",
        "stefanie", "susanne", "tanja", "ute", "verena",
    ];
    const MALE: &[&str] = &[
        "alexander", "andreas", "armin", "bernd", "bernhard", "bruno",
        "christian", "christoph", "daniel", "david", "dieter", "dirk",
        "felix", "florian", "frank", "georg", "gerhard", "guido", "günther",
        "hans", "harald", "heinz", "helmut", "holger", "jakob", "jan",
        "jens", "johannes", "jonas", "jörg", "josef", "julian", "jürgen",
        "karl", "klaus", "kurt", "lars", "lukas", "manfred", "marcel",
        "marco", "markus", "martin", "matthias", "max", "michael", "norbert",
        "oliver", "patrick", "paul", "peter", "philipp", "ralf", "reiner",
        "robert", "rolf", "rudolf", "sebastian", "simon", "stefan",
        "stephan", "thomas", "tobias", "ulrich", "uwe", "volker", "walter",
        "werner", "wolfgang",
    ];
    if FEMALE.iter().any(|n| *n == lower) {
        return Some("weiblich");
    }
    if MALE.iter().any(|n| *n == lower) {
        return Some("männlich");
    }
    // Endungs-Heuristik nur als letzter Anker — viele ambige Fälle
    // (Andrea, Sascha) werden bewusst weggelassen.
    if lower.ends_with('a') && lower.len() >= 3 {
        return Some("weiblich");
    }
    None
}

// ---------------------------------------------------------------------------
// Field-evidence helpers
// ---------------------------------------------------------------------------

fn push_high(out: &mut Vec<(FieldKey, FieldEvidence)>, key: FieldKey, value: &str, url: &str) {
    push(out, key, value, url, Confidence::High);
}

fn push_low(out: &mut Vec<(FieldKey, FieldEvidence)>, key: FieldKey, value: &str, url: &str) {
    push(out, key, value, url, Confidence::Low);
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

static MODULE: LinkedIn = LinkedIn;

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
        include_str!("../../fixtures/sources/linkedin/peoplesearch_wittenstein.json");
    const DETAIL_FIXTURE: &str =
        include_str!("../../fixtures/sources/linkedin/people_detail_abc123.json");

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
        assert_eq!(m.id(), "linkedin.com");
        assert!(m.aliases().contains(&"linkedin"));
        assert!(matches!(m.tier(), Tier::C));
        assert_eq!(
            m.countries(),
            &[Country::De, Country::At, Country::Ch]
        );
        assert_eq!(m.requires_credential(), Some("LINKEDIN_SALES_NAV_TOKEN"));
        assert!(m
            .authoritative_for()
            .iter()
            .any(|k| matches!(k, FieldKey::PersonLinkedin)));
        assert!(m
            .authoritative_for()
            .iter()
            .any(|k| matches!(k, FieldKey::PersonFunktion)));
        assert!(m
            .authoritative_for()
            .iter()
            .any(|k| matches!(k, FieldKey::PersonGeschlecht)));
    }

    #[test]
    fn shape_query_is_none_for_api_source() {
        let ctx = SourceCtx {
            root: Path::new("/tmp/ctox-test-linkedin"),
            country: Some(Country::De),
            mode: ResearchMode::NewRecord,
        };
        assert!(module().shape_query("WITTENSTEIN SE", &ctx).is_none());
    }

    #[test]
    fn fetch_direct_skips_non_dach_countries() {
        // No non-DACH country exists in Country enum today, so this is a
        // structural guard: if Country ever grows (e.g. FR), the explicit
        // `matches!` in `fetch_direct` continues to short-circuit.
        // The reachable equivalent test: with country=None we engage but
        // immediately surface CredentialMissing because no token is set.
        let ctx = SourceCtx {
            root: Path::new("/tmp/ctox-test-linkedin-unset"),
            country: None,
            mode: ResearchMode::NewRecord,
        };
        let r = module()
            .fetch_direct(&ctx, "WITTENSTEIN SE")
            .expect("country=None must engage");
        assert!(
            matches!(
                r,
                Err(SourceError::CredentialMissing {
                    secret_name: "LINKEDIN_SALES_NAV_TOKEN"
                })
            ),
            "expected credential_missing when no token in runtime store"
        );
    }

    #[test]
    fn fetch_direct_empty_company_is_no_match() {
        let ctx = SourceCtx {
            root: Path::new("/tmp/ctox-test-linkedin"),
            country: Some(Country::De),
            mode: ResearchMode::NewRecord,
        };
        let r = module()
            .fetch_direct(&ctx, "   ")
            .expect("DE context must engage");
        assert!(matches!(r, Err(SourceError::NoMatch)));
    }

    #[test]
    fn fetch_direct_missing_token_returns_credential_missing() {
        // Pointing at a directory with no SQLite runtime store at all is
        // the equivalent of "key not set" — runtime_config::get returns
        // None, and we must surface CredentialMissing.
        let ctx = SourceCtx {
            root: Path::new("/tmp/ctox-this-path-does-not-exist-linkedin"),
            country: Some(Country::De),
            mode: ResearchMode::NewRecord,
        };
        let r = module()
            .fetch_direct(&ctx, "WITTENSTEIN SE")
            .expect("DE context must engage");
        match r {
            Err(SourceError::CredentialMissing { secret_name }) => {
                assert_eq!(secret_name, "LINKEDIN_SALES_NAV_TOKEN");
            }
            other => panic!("expected CredentialMissing, got {other:?}"),
        }
    }

    #[test]
    fn parses_search_fixture_into_hits() {
        let value: Value = serde_json::from_str(SEARCH_FIXTURE).expect("fixture json");
        let hits = parse_search_hits(&value).expect("fixture has hits");
        assert_eq!(hits.len(), 3);
        let anna = hits
            .iter()
            .find(|h| h.title == "Anna Müller")
            .expect("Anna Müller hit");
        assert_eq!(
            anna.url,
            "https://www.linkedin.com/in/anna-mueller-wittenstein/"
        );
        assert!(anna.snippet.contains("Geschäftsführerin"));
    }

    #[test]
    fn empty_search_elements_maps_to_no_match() {
        let value: Value =
            serde_json::from_str(r#"{"elements": [], "paging": {"total": 0}}"#).unwrap();
        let err = parse_search_hits(&value).unwrap_err();
        assert!(matches!(err, SourceError::NoMatch));
    }

    #[test]
    fn missing_elements_field_maps_to_parse_failed() {
        let value: Value = serde_json::from_str(r#"{"oops": "boom"}"#).unwrap();
        let err = parse_search_hits(&value).unwrap_err();
        assert!(matches!(err, SourceError::ParseFailed { .. }));
    }

    #[test]
    fn extracts_funktion_and_linkedin_from_search_hit() {
        let page = dummy_page(
            SEARCH_FIXTURE,
            "https://api.linkedin.com/v2/peopleSearch?q=companyName&companyName=WITTENSTEIN%20SE",
        );
        let fields = module().extract_fields(&page);

        let funktion = fields
            .iter()
            .find(|(k, _)| matches!(k, FieldKey::PersonFunktion))
            .expect("person_funktion");
        assert_eq!(funktion.1.value, "Geschäftsführerin Vertrieb");
        assert!(matches!(funktion.1.confidence, Confidence::High));

        let linkedin = fields
            .iter()
            .find(|(k, _)| matches!(k, FieldKey::PersonLinkedin))
            .expect("person_linkedin");
        assert_eq!(
            linkedin.1.value,
            "https://www.linkedin.com/in/anna-mueller-wittenstein/"
        );
        assert!(matches!(linkedin.1.confidence, Confidence::High));
    }

    #[test]
    fn extracts_fields_from_detail_body() {
        let page = dummy_page(
            DETAIL_FIXTURE,
            "https://api.linkedin.com/v2/people/(id:urn:li:person:abc123)",
        );
        let fields = module().extract_fields(&page);

        // person_funktion = currentPositions[0].title
        let funktion = fields
            .iter()
            .find(|(k, _)| matches!(k, FieldKey::PersonFunktion))
            .expect("person_funktion");
        assert_eq!(funktion.1.value, "Geschäftsführerin Vertrieb");
        assert!(matches!(funktion.1.confidence, Confidence::High));

        // person_linkedin built from publicIdentifier
        let linkedin = fields
            .iter()
            .find(|(k, _)| matches!(k, FieldKey::PersonLinkedin))
            .expect("person_linkedin");
        assert_eq!(
            linkedin.1.value,
            "https://www.linkedin.com/in/anna-mueller-wittenstein/"
        );

        // person_geschlecht heuristic: Anna → weiblich (Confidence::Low).
        let geschlecht = fields
            .iter()
            .find(|(k, _)| matches!(k, FieldKey::PersonGeschlecht));
        let geschlecht = geschlecht.expect("person_geschlecht heuristic for 'Anna'");
        assert_eq!(geschlecht.1.value, "weiblich");
        assert!(matches!(geschlecht.1.confidence, Confidence::Low));
    }

    #[test]
    fn extract_fields_returns_nothing_on_non_json_input() {
        let page = dummy_page("<html><body>not json</body></html>", "https://x/y");
        assert!(module().extract_fields(&page).is_empty());
    }

    #[test]
    fn headline_role_extraction_strips_company_suffix() {
        assert_eq!(
            extract_role_from_headline("Geschäftsführerin Vertrieb bei WITTENSTEIN SE"),
            Some("Geschäftsführerin Vertrieb".to_string())
        );
        assert_eq!(
            extract_role_from_headline("Head of Marketing | WITTENSTEIN SE"),
            Some("Head of Marketing".to_string())
        );
        assert_eq!(
            extract_role_from_headline("CFO at SomeCo"),
            Some("CFO".to_string())
        );
        assert!(extract_role_from_headline("   bei Firma").is_none());
    }

    #[test]
    fn gender_heuristic_only_fires_for_clear_cases() {
        assert_eq!(guess_gender_from_firstname("Anna"), Some("weiblich"));
        assert_eq!(guess_gender_from_firstname("Bernd"), Some("männlich"));
        // Endungs-Heuristik: dreibuchstabig + endet auf 'a'
        assert_eq!(guess_gender_from_firstname("Eva"), Some("weiblich"));
        // Klar ambig: keine Aussage
        assert_eq!(guess_gender_from_firstname("Kim"), None);
        // Leerer Input
        assert_eq!(guess_gender_from_firstname(""), None);
    }

    #[test]
    fn read_localized_handles_plain_string_and_locale_object() {
        let raw = serde_json::json!("Hallo");
        assert_eq!(read_localized(Some(&raw)), Some("Hallo".to_string()));

        let raw = serde_json::json!({
            "localized": {"de_DE": "Hallo Welt"},
            "preferredLocale": {"country": "DE", "language": "de"},
        });
        assert_eq!(read_localized(Some(&raw)), Some("Hallo Welt".to_string()));

        // Unbekannte preferredLocale → erster Wert.
        let raw = serde_json::json!({
            "localized": {"en_US": "Hi"},
            "preferredLocale": {"country": "DE", "language": "de"},
        });
        assert_eq!(read_localized(Some(&raw)), Some("Hi".to_string()));
    }

    #[test]
    fn person_to_hit_skips_entries_without_public_identifier() {
        let raw = serde_json::json!({
            "firstName": {"localized": {"de_DE": "A"}, "preferredLocale": {"country":"DE","language":"de"}},
            "lastName":  {"localized": {"de_DE": "B"}, "preferredLocale": {"country":"DE","language":"de"}},
        });
        assert!(person_to_hit(&raw).is_none());
    }

    #[test]
    #[ignore = "live network; run with: cargo test -p ctox-web-stack -- --ignored sources::linkedin"]
    fn live_search_smoke_handles_credential_missing_cleanly() {
        // Live smoke test. With no LINKEDIN_SALES_NAV_TOKEN in the test
        // root's SQLite runtime store, this MUST surface
        // CredentialMissing — never a network panic, never a TOS-violating
        // fallback to scraping linkedin.com.
        let ctx = SourceCtx {
            root: Path::new("/tmp/ctox-linkedin-live-smoke-no-token"),
            country: Some(Country::De),
            mode: ResearchMode::NewRecord,
        };
        let result = module()
            .fetch_direct(&ctx, "WITTENSTEIN SE")
            .expect("DE context must engage");
        match result {
            Err(SourceError::CredentialMissing { secret_name }) => {
                assert_eq!(secret_name, "LINKEDIN_SALES_NAV_TOKEN");
            }
            Ok(hits) => panic!(
                "expected credential_missing without a token, got {} hits",
                hits.len()
            ),
            Err(other) => panic!("expected credential_missing, got {other:?}"),
        }
    }
}
