//! `xing.com` — Tier C, DACH (Schwerpunkt DE).
//!
//! XING-API (TalentManager / E-Recruiting / Profile-API) als Phase-2-API-Adapter.
//! Die Quelle ist laut [`EXCEL_MATRIX`](./EXCEL_MATRIX.md) autoritativ für
//! `person_funktion` (aktuelle Funktionsbezeichnung aus dem Primärbeschäftiger)
//! und `person_xing` (Permalink auf das XING-Profil) — sowohl im Modus
//! `UpdatePerson` als auch in `NewRecord` für alle drei DACH-Länder.
//!
//! ## Endpoints
//!
//! XING-Partner-Programm (`https://dev.xing.com`), OAuth2 Bearer-Token:
//!
//! * `GET https://api.xing.com/v1/users/find?keywords=<firma+name>&limit=<n>`
//!   Antwort: `{"users": {"items": [User, ...], "total": <n>}}`.
//!   Jeder `User` enthält `id`, `display_name`, `permalink` und (für
//!   Such-Treffer mit Beschäftigungs-Kontext) `professional_experience.primary_company`
//!   mit Feldern `name`, `title`, `company_size`, `industry`.
//! * `GET https://api.xing.com/v1/users/<user_id>`
//!   Antwort: `{"users": [User]}` — ein-Element-Array mit dem vollen
//!   Profil-Body (gleiche Felder wie der Such-Treffer plus
//!   `business_address`, `non_primary_companies`, `wants`, `haves`, …).
//!
//! ## Extrahierte Felder
//!
//! * `person_funktion` = `professional_experience.primary_company.title`,
//!   `Confidence::High` (strukturiertes API-Feld; vom XING-User selbst gepflegt).
//! * `person_xing`     = `permalink`,
//!   `Confidence::High` (kanonische Profil-URL; OAuth-stabil).

use std::time::Duration;

use anyhow::anyhow;
use serde_json::Value;

use crate::runtime_config;

use super::{
    Confidence, Country, FieldEvidence, FieldKey, ShapedQuery, SourceCtx, SourceError,
    SourceHit, SourceModule, SourceReadResult, Tier,
};

const API_BASE: &str = "https://api.xing.com/v1";
const SECRET_NAME: &str = "XING_API_TOKEN";
const MAX_HITS: usize = 8;
const TIMEOUT_MS: u64 = 12_000;
const USER_AGENT: &str = "ctox-web-stack/0.1 (+https://ctox.local)";

struct Xing;

impl SourceModule for Xing {
    fn id(&self) -> &'static str {
        "xing.com"
    }

    fn aliases(&self) -> &'static [&'static str] {
        &["xing"]
    }

    fn host_suffixes(&self) -> &'static [&'static str] {
        &["api.xing.com"]
    }

    fn tier(&self) -> Tier {
        Tier::C
    }

    fn countries(&self) -> &'static [Country] {
        &[Country::De, Country::At, Country::Ch]
    }

    fn authoritative_for(&self) -> &'static [FieldKey] {
        &[FieldKey::PersonFunktion, FieldKey::PersonXing]
    }

    fn requires_credential(&self) -> Option<&'static str> {
        Some(SECRET_NAME)
    }

    fn shape_query(&self, _query: &str, _ctx: &SourceCtx<'_>) -> Option<ShapedQuery> {
        // API-Quelle: kein Search-Engine-Fallback.
        None
    }

    fn fetch_direct(
        &self,
        ctx: &SourceCtx<'_>,
        company: &str,
    ) -> Option<Result<Vec<SourceHit>, SourceError>> {
        // DACH-only. Andere Länder werden still übersprungen, damit der
        // Orchestrator die Quelle nicht in seine Priority-Liste aufnimmt.
        if matches!(ctx.country, Some(country) if !matches!(country, Country::De | Country::At | Country::Ch))
        {
            return None;
        }

        let trimmed = company.trim();
        if trimmed.is_empty() {
            return Some(Err(SourceError::NoMatch));
        }

        let token = match runtime_config::get(ctx.root, SECRET_NAME) {
            Some(value) => value,
            None => {
                return Some(Err(SourceError::CredentialMissing {
                    secret_name: SECRET_NAME,
                }));
            }
        };

        let agent = build_agent();
        Some(perform_search(&agent, &token, trimmed))
    }

    fn extract_fields(&self, page: &SourceReadResult) -> Vec<(FieldKey, FieldEvidence)> {
        let value = match serde_json::from_str::<Value>(page.text.trim_start()) {
            Ok(v) => v,
            Err(_) => return Vec::new(),
        };
        let user = match pick_user(&value) {
            Some(u) => u,
            None => return Vec::new(),
        };
        extract_from_user(user, &page.url)
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

fn perform_search(
    agent: &ureq::Agent,
    token: &str,
    keywords: &str,
) -> Result<Vec<SourceHit>, SourceError> {
    let url = format!("{API_BASE}/users/find");
    let response = agent
        .get(&url)
        .query("keywords", keywords)
        .query("limit", &MAX_HITS.to_string())
        .set("accept", "application/json")
        .set("authorization", &format!("Bearer {token}"))
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
            SourceError::Other(anyhow!("xing http {status}: {detail}"))
        }
    }
}

// ---------------------------------------------------------------------------
// Parsing: Search results
// ---------------------------------------------------------------------------

fn parse_search_hits(value: &Value) -> Result<Vec<SourceHit>, SourceError> {
    let items = value
        .get("users")
        .and_then(|users| users.get("items"))
        .and_then(Value::as_array)
        .ok_or_else(|| SourceError::ParseFailed {
            detail: "missing `users.items` array".to_string(),
        })?;
    if items.is_empty() {
        return Err(SourceError::NoMatch);
    }

    let mut hits = Vec::with_capacity(items.len().min(MAX_HITS));
    for entry in items.iter().take(MAX_HITS) {
        if let Some(hit) = user_to_hit(entry) {
            hits.push(hit);
        }
    }
    if hits.is_empty() {
        return Err(SourceError::NoMatch);
    }
    Ok(hits)
}

fn user_to_hit(entry: &Value) -> Option<SourceHit> {
    let permalink = entry.get("permalink").and_then(Value::as_str)?.trim();
    if permalink.is_empty() {
        return None;
    }
    let title = entry
        .get("display_name")
        .and_then(Value::as_str)
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| permalink.to_string());

    let primary = entry
        .get("professional_experience")
        .and_then(|exp| exp.get("primary_company"));
    let company = primary
        .and_then(|c| c.get("name"))
        .and_then(Value::as_str)
        .unwrap_or("")
        .trim();
    let funktion = primary
        .and_then(|c| c.get("title"))
        .and_then(Value::as_str)
        .unwrap_or("")
        .trim();
    let snippet = match (company.is_empty(), funktion.is_empty()) {
        (false, false) => format!("{funktion} · {company}"),
        (false, true) => company.to_string(),
        (true, false) => funktion.to_string(),
        (true, true) => String::new(),
    };

    Some(SourceHit {
        title,
        url: permalink.to_string(),
        snippet,
    })
}

// ---------------------------------------------------------------------------
// Parsing: Detail (User object)
// ---------------------------------------------------------------------------

/// Wählt das User-Objekt aus einem rohen XING-API-Body.
///
/// Akzeptiert sowohl die Detail-Form (`{"users": [user]}`, ein Element)
/// als auch die Such-Form (`{"users": {"items": [user, ...]}}`, dann
/// erstes Item) und schließlich einen "nackten" User-Body.
fn pick_user(value: &Value) -> Option<&Value> {
    if let Some(arr) = value.get("users").and_then(Value::as_array) {
        return arr.first();
    }
    if let Some(items) = value
        .get("users")
        .and_then(|u| u.get("items"))
        .and_then(Value::as_array)
    {
        return items.first();
    }
    // Bare user body fallback: must have permalink to be identifiable.
    if value.get("permalink").and_then(Value::as_str).is_some() {
        return Some(value);
    }
    None
}

fn extract_from_user(user: &Value, source_url: &str) -> Vec<(FieldKey, FieldEvidence)> {
    let mut out = Vec::new();
    // `permalink` ist die kanonische XING-Profil-URL und das einzige
    // Feld, das wir als `person_xing` schreiben dürfen. Fehlt der
    // Permalink, taggen wir die Person-Evidence stattdessen mit der
    // Load-URL — aber emittieren KEIN `person_xing` (das Feld wäre sonst
    // ein API-Endpoint, kein Profil-Link).
    let permalink = user
        .get("permalink")
        .and_then(Value::as_str)
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty());
    let evidence_url = permalink.clone().unwrap_or_else(|| source_url.to_string());

    if let Some(ref link) = permalink {
        push_high(&mut out, FieldKey::PersonXing, link, &evidence_url);
    }

    if let Some(title) = user
        .get("professional_experience")
        .and_then(|exp| exp.get("primary_company"))
        .and_then(|c| c.get("title"))
        .and_then(Value::as_str)
    {
        push_high(&mut out, FieldKey::PersonFunktion, title, &evidence_url);
    }

    out
}

// ---------------------------------------------------------------------------
// Field-evidence helpers
// ---------------------------------------------------------------------------

fn push_high(out: &mut Vec<(FieldKey, FieldEvidence)>, key: FieldKey, value: &str, url: &str) {
    push(out, key, value, url, Confidence::High);
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

static MODULE: Xing = Xing;

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
        include_str!("../../fixtures/sources/xing/users_find_wittenstein.json");
    const DETAIL_FIXTURE: &str =
        include_str!("../../fixtures/sources/xing/users_detail_10368_abcdef.json");

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
        assert_eq!(m.id(), "xing.com");
        assert_eq!(m.aliases(), &["xing"]);
        assert!(matches!(m.tier(), Tier::C));
        assert_eq!(m.countries(), &[Country::De, Country::At, Country::Ch]);
        assert_eq!(m.requires_credential(), Some("XING_API_TOKEN"));
        assert!(m
            .authoritative_for()
            .iter()
            .any(|k| matches!(k, FieldKey::PersonFunktion)));
        assert!(m
            .authoritative_for()
            .iter()
            .any(|k| matches!(k, FieldKey::PersonXing)));
    }

    #[test]
    fn shape_query_is_none_for_api_source() {
        let ctx = SourceCtx {
            root: Path::new("/tmp/ctox-test"),
            country: Some(Country::De),
            mode: ResearchMode::NewRecord,
        };
        assert!(module().shape_query("WITTENSTEIN SE", &ctx).is_none());
    }

    #[test]
    fn fetch_direct_engages_for_de_without_token_reports_credential_missing() {
        // The `Country` enum currently only carries DE/AT/CH, so the
        // non-DACH guard branch is defensive (forward-compatible). For
        // each in-DACH country we instead exercise the credential branch:
        // without a token in the runtime config, we MUST surface
        // `CredentialMissing` so person-research can skip the source.
        let ctx = SourceCtx {
            root: Path::new("/tmp/ctox-test-xing-no-secret"),
            country: Some(Country::De),
            mode: ResearchMode::NewRecord,
        };
        let result = module()
            .fetch_direct(&ctx, "WITTENSTEIN SE")
            .expect("DACH context must engage");
        assert!(matches!(
            result,
            Err(SourceError::CredentialMissing {
                secret_name: "XING_API_TOKEN"
            })
        ));
    }

    #[test]
    fn fetch_direct_empty_company_short_circuits_no_match() {
        // Even without a token: empty company string must map to
        // `NoMatch` before we ever look at credentials, so the test stays
        // hermetic.
        let ctx = SourceCtx {
            root: Path::new("/tmp/ctox-test-xing-empty"),
            country: Some(Country::De),
            mode: ResearchMode::NewRecord,
        };
        let r = module()
            .fetch_direct(&ctx, "   ")
            .expect("DACH context must engage");
        assert!(matches!(r, Err(SourceError::NoMatch)));
    }

    #[test]
    fn fetch_direct_credential_missing_for_at_and_ch() {
        for country in [Country::At, Country::Ch] {
            let ctx = SourceCtx {
                root: Path::new("/tmp/ctox-test-xing-no-secret"),
                country: Some(country),
                mode: ResearchMode::NewRecord,
            };
            let result = module()
                .fetch_direct(&ctx, "WITTENSTEIN SE")
                .expect("DACH context must engage");
            assert!(matches!(
                result,
                Err(SourceError::CredentialMissing {
                    secret_name: "XING_API_TOKEN"
                })
            ));
        }
    }

    #[test]
    fn fetch_direct_with_unknown_country_engages() {
        // country=None means "orchestrator did not yet bind a country" —
        // we still engage and report credential_missing rather than
        // returning None.
        let ctx = SourceCtx {
            root: Path::new("/tmp/ctox-test-xing-no-secret"),
            country: None,
            mode: ResearchMode::NewRecord,
        };
        let result = module()
            .fetch_direct(&ctx, "WITTENSTEIN SE")
            .expect("country=None must engage");
        assert!(matches!(
            result,
            Err(SourceError::CredentialMissing { .. })
        ));
    }

    #[test]
    fn parses_search_fixture_into_hits() {
        let value: Value = serde_json::from_str(SEARCH_FIXTURE).expect("fixture json");
        let hits = parse_search_hits(&value).expect("fixture has hits");
        assert_eq!(hits.len(), 3, "fixture has three users");
        let anna = hits
            .iter()
            .find(|h| h.url == "https://www.xing.com/profile/Anna_Schmidt10")
            .expect("Anna Schmidt hit");
        assert_eq!(anna.title, "Anna Schmidt");
        assert!(
            anna.snippet.contains("Leiterin Forschung & Entwicklung"),
            "expected funktion in snippet, got: {}",
            anna.snippet
        );
        assert!(
            anna.snippet.contains("WITTENSTEIN SE"),
            "expected company in snippet, got: {}",
            anna.snippet
        );
    }

    #[test]
    fn parses_search_fixture_caps_hits_at_max() {
        let value: Value = serde_json::from_str(SEARCH_FIXTURE).expect("fixture json");
        let hits = parse_search_hits(&value).expect("hits");
        assert!(hits.len() <= MAX_HITS);
    }

    #[test]
    fn empty_items_list_maps_to_no_match() {
        let value: Value =
            serde_json::from_str(r#"{"users": {"items": [], "total": 0}}"#).unwrap();
        let err = parse_search_hits(&value).unwrap_err();
        assert!(matches!(err, SourceError::NoMatch));
    }

    #[test]
    fn missing_items_field_maps_to_parse_failed() {
        let value: Value = serde_json::from_str(r#"{"error": "boom"}"#).unwrap();
        let err = parse_search_hits(&value).unwrap_err();
        assert!(matches!(err, SourceError::ParseFailed { .. }));
    }

    #[test]
    fn extracts_funktion_and_xing_from_detail_json_with_high_confidence() {
        let page = dummy_page(
            DETAIL_FIXTURE,
            "https://api.xing.com/v1/users/10368_abcdef",
        );
        let fields = module().extract_fields(&page);

        let funktion = fields
            .iter()
            .find(|(k, _)| matches!(k, FieldKey::PersonFunktion))
            .expect("person_funktion");
        assert_eq!(funktion.1.value, "Leiterin Forschung & Entwicklung");
        assert!(matches!(funktion.1.confidence, Confidence::High));

        let xing = fields
            .iter()
            .find(|(k, _)| matches!(k, FieldKey::PersonXing))
            .expect("person_xing");
        assert_eq!(xing.1.value, "https://www.xing.com/profile/Anna_Schmidt10");
        assert!(matches!(xing.1.confidence, Confidence::High));

        // Both evidence rows must point at the canonical XING permalink,
        // not at the raw API URL — that's what downstream consumers expect.
        for (_, ev) in &fields {
            assert_eq!(
                ev.source_url, "https://www.xing.com/profile/Anna_Schmidt10",
                "evidence source_url must be the permalink, got {}",
                ev.source_url
            );
        }
    }

    #[test]
    fn extracts_from_search_fixture_top_item_as_user() {
        // `pick_user` also accepts the search-result shape and returns the
        // first item. Useful when the orchestrator passes a `users/find`
        // body through `extract_fields` after a fetch_direct hit.
        let page = dummy_page(SEARCH_FIXTURE, "https://api.xing.com/v1/users/find");
        let fields = module().extract_fields(&page);
        let funktion = fields
            .iter()
            .find(|(k, _)| matches!(k, FieldKey::PersonFunktion))
            .expect("person_funktion");
        assert_eq!(funktion.1.value, "Leiterin Forschung & Entwicklung");
        let xing = fields
            .iter()
            .find(|(k, _)| matches!(k, FieldKey::PersonXing))
            .expect("person_xing");
        assert_eq!(xing.1.value, "https://www.xing.com/profile/Anna_Schmidt10");
    }

    #[test]
    fn extract_fields_returns_empty_for_non_json_body() {
        let html = "<html><head><title>XING – Anna Schmidt</title></head><body></body></html>";
        let page = dummy_page(html, "https://www.xing.com/profile/Anna_Schmidt10");
        let fields = module().extract_fields(&page);
        assert!(
            fields.is_empty(),
            "HTML pages are not the API contract; should yield no evidence"
        );
    }

    #[test]
    fn extract_fields_ignores_user_object_without_permalink_or_funktion() {
        // A user body that has neither a permalink nor a primary_company.title
        // must not emit any evidence at all.
        let raw = r#"{"users": [{"id": "x", "display_name": "Anon"}]}"#;
        let page = dummy_page(raw, "https://api.xing.com/v1/users/x");
        let fields = module().extract_fields(&page);
        assert!(fields.is_empty(), "got unexpected fields: {fields:?}");
    }

    #[test]
    fn pick_user_accepts_bare_user_body() {
        let raw = r#"{"id": "bare", "display_name": "Bare User",
                       "permalink": "https://www.xing.com/profile/Bare_User",
                       "professional_experience": {
                         "primary_company": {"name": "X AG", "title": "CEO"}
                       }}"#;
        let value: Value = serde_json::from_str(raw).unwrap();
        let user = pick_user(&value).expect("bare body must be accepted");
        let fields = extract_from_user(user, "https://api.xing.com/v1/users/bare");
        let funktion = fields
            .iter()
            .find(|(k, _)| matches!(k, FieldKey::PersonFunktion))
            .expect("person_funktion");
        assert_eq!(funktion.1.value, "CEO");
    }

    #[test]
    #[ignore = "live network; run with: cargo test -p ctox-web-stack -- --ignored sources::xing"]
    fn live_search_credential_missing_smoke() {
        // Phase 2 live-test: without a configured `XING_API_TOKEN` in the
        // test-only runtime root, the adapter MUST report
        // `CredentialMissing` rather than touching the network or
        // surfacing a generic error. This is the contract person-research
        // relies on to skip the source on tenants that have not yet
        // onboarded the XING partner program.
        let tmp = std::env::temp_dir().join("ctox-web-stack-xing-live-smoke");
        // Ensure the runtime DB at `<root>/runtime/ctox.sqlite3` does NOT
        // exist (or contains no XING token) — using a non-existent dir is
        // the cleanest way: `runtime_config::get` returns `None`.
        let ctx = SourceCtx {
            root: tmp.as_path(),
            country: Some(Country::De),
            mode: ResearchMode::NewRecord,
        };
        let result = module()
            .fetch_direct(&ctx, "WITTENSTEIN SE")
            .expect("DACH context must engage");
        assert!(
            matches!(
                result,
                Err(SourceError::CredentialMissing {
                    secret_name: "XING_API_TOKEN"
                })
            ),
            "expected credential_missing, got: {result:?}"
        );
    }
}
