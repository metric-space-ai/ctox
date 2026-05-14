//! `companyhouse.de` — Tier S, DE only.
//!
//! Aggregator über das Deutsche Handelsregister. Personen-Profile
//! (`/person/<Vorname>-<Nachname>`) listen die akademischen Titel und die
//! Mandate, an denen die Person als Vorstand / Aufsichtsrat / Geschäftsführer
//! beteiligt ist. Firmen-Profile (`/<Firmenname>`) zeigen Stammdaten plus
//! die aktuelle Geschäftsführung. Basis-Informationen sind ohne Login
//! sichtbar (Freemium); tiefe Profile sind hinter einer Paywall.
//!
//! Extraktions-Plan (high-level):
//!   * Personenseite: das `<h1>` enthält Titel + Vor- + Nachname am Stück
//!     (z. B. „Dr. Manfred Schneider", „Prof. Dr.-Ing. Anna Müller").
//!     Der Parser zieht alle führenden Titel-Tokens (Konvention: `.`-Suffix
//!     oder definierte Whitelist), das verbleibende Tail wird in Vor- und
//!     Nachname zerlegt (DE-Konvention: Vorname zuerst). Confidence: Medium,
//!     weil die HTML-Struktur stabil ist, der Heuristik-Split aber bei
//!     Doppel-Nachnamen oder Adelsprädikaten („von der") nicht perfekt
//!     trennt. Laut Excel-Matrix ist `person_titel` der primäre Beitrag
//!     dieser Quelle.
//!   * Firmenseite: das `<h1>` ist der Firmenname. Hier ist die Quelle
//!     für `firma_name` nahe-autoritativ (Confidence::High), bleibt aber
//!     in der Excel-Matrix für die Person-Felder relevant; die Person-
//!     Felder werden auf einer Firmenseite **nicht** geschrieben — dafür
//!     ist Companyhouse zu unspezifisch (Mehrere Vorstände in einer Liste,
//!     kein eindeutiger „primary contact").

use scraper::{ElementRef, Html, Selector};
use std::sync::OnceLock;

use super::{
    Confidence, Country, FieldEvidence, FieldKey, ShapedQuery, SourceCtx, SourceModule,
    SourceReadResult, Tier,
};

const ID: &str = "companyhouse.de";
const DOMAIN: &str = "companyhouse.de";

struct Companyhouse;

impl SourceModule for Companyhouse {
    fn id(&self) -> &'static str {
        ID
    }

    fn aliases(&self) -> &'static [&'static str] {
        &["companyhouse", "ch_de"]
    }

    fn tier(&self) -> Tier {
        Tier::S
    }

    fn countries(&self) -> &'static [Country] {
        &[Country::De]
    }

    fn authoritative_for(&self) -> &'static [FieldKey] {
        // Aus EXCEL_MATRIX.md: companyhouse.de ist DE-only und in der
        // Thesen-Matrix für `person_titel` (B 1 / B 2 / Neu) gelistet.
        // Firmenname wird zusätzlich angeboten, weil die Firmen-Profilseite
        // den Namen aus dem Handelsregister sauber als Heading führt; das
        // ist nicht „in der Excel" als Primärquelle vergeben, aber durch
        // das Quellen-Modul nutzbar wenn der Orchestrator den Hit liefert.
        &[FieldKey::PersonTitel]
    }

    /// Crawl-Pfad: Query mit Domain-Pin. DE-only — andere Länder bekommen
    /// `None` und werden vom Orchestrator übersprungen. Wenn `country`
    /// nicht gesetzt ist (Agent kennt das Land der Recherche noch nicht),
    /// erlauben wir die Quelle trotzdem, weil das Excel-Mapping
    /// `companyhouse.de` ausschließlich DE-seitig referenziert und ein
    /// fehlendes Land-Signal sonst zu Über-Filtering führen würde.
    fn shape_query(&self, query: &str, ctx: &SourceCtx<'_>) -> Option<ShapedQuery> {
        match ctx.country {
            Some(Country::De) | None => {}
            Some(_) => return None,
        }
        let trimmed = query.trim();
        if trimmed.is_empty() {
            return None;
        }
        Some(ShapedQuery {
            // Domain-Pin redundant im Query-Text, damit Provider ohne
            // explizites Domain-Filter (DDG / Brave fallback) die Quelle
            // priorisieren.
            query: format!("{trimmed} site:{DOMAIN}"),
            domains: vec![DOMAIN.to_string()],
        })
    }

    fn extract_fields(&self, page: &SourceReadResult) -> Vec<(FieldKey, FieldEvidence)> {
        if page.is_pdf {
            return Vec::new();
        }
        extract_from_html(&page.text, &page.url)
    }
}

static MODULE: Companyhouse = Companyhouse;

pub fn module() -> &'static dyn SourceModule {
    &MODULE
}

// ---------------------------------------------------------------------------
// Selectors
// ---------------------------------------------------------------------------

fn h1_selector() -> &'static Selector {
    static SEL: OnceLock<Selector> = OnceLock::new();
    SEL.get_or_init(|| Selector::parse("h1").expect("valid h1 selector"))
}

// ---------------------------------------------------------------------------
// Core extraction
// ---------------------------------------------------------------------------

fn extract_from_html(html: &str, source_url: &str) -> Vec<(FieldKey, FieldEvidence)> {
    let document = Html::parse_document(html);

    let h1 = match document.select(h1_selector()).next() {
        Some(node) => node,
        None => return Vec::new(),
    };
    let heading = node_text(&h1);
    if heading.is_empty() {
        return Vec::new();
    }

    let mut out: Vec<(FieldKey, FieldEvidence)> = Vec::new();

    if is_person_url(source_url) {
        if let Some(parsed) = parse_person_heading(&heading) {
            if let Some(title) = parsed.title {
                push(
                    &mut out,
                    FieldKey::PersonTitel,
                    title,
                    Confidence::Medium,
                    source_url,
                    Some("companyhouse person <h1> title prefix"),
                );
            }
            if let Some(first) = parsed.first_name {
                push(
                    &mut out,
                    FieldKey::PersonVorname,
                    first,
                    Confidence::Medium,
                    source_url,
                    Some("companyhouse person <h1> first name"),
                );
            }
            if let Some(last) = parsed.last_name {
                push(
                    &mut out,
                    FieldKey::PersonNachname,
                    last,
                    Confidence::Medium,
                    source_url,
                    Some("companyhouse person <h1> last name"),
                );
            }
        }
    } else if is_company_url(source_url) {
        // Firmen-Profile haben den Firmennamen sauber im <h1>.
        push(
            &mut out,
            FieldKey::FirmaName,
            heading,
            Confidence::High,
            source_url,
            Some("companyhouse company <h1>"),
        );
    }
    // Wenn weder Person- noch Firmen-Pfad erkennbar ist (z. B. Suchergebnis,
    // Statusseite, Login-Redirect), liefern wir gracefully eine leere Liste.

    out
}

fn push(
    out: &mut Vec<(FieldKey, FieldEvidence)>,
    key: FieldKey,
    value: String,
    confidence: Confidence,
    source_url: &str,
    note: Option<&str>,
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
            source_url: source_url.to_string(),
            note: note.map(|s| s.to_string()),
        },
    ));
}

fn node_text(node: &ElementRef<'_>) -> String {
    let raw: String = node.text().collect::<Vec<_>>().join(" ");
    let mut buf = String::with_capacity(raw.len());
    let mut prev_space = false;
    for ch in raw.chars() {
        if ch.is_whitespace() {
            if !prev_space {
                buf.push(' ');
                prev_space = true;
            }
        } else {
            buf.push(ch);
            prev_space = false;
        }
    }
    buf.trim().to_string()
}

/// `/person/<Vorname>-<Nachname>` is the canonical person path. We accept any
/// case and any depth under `/person/`, but require the segment to be there
/// somewhere — otherwise a randomly-named company profile starting with the
/// letters `person` (e.g. `/Personalmanagement-Berlin-First-UG-Berlin`)
/// would mis-route to the person parser.
fn is_person_url(url: &str) -> bool {
    let lower = url.to_ascii_lowercase();
    lower.contains("/person/") || lower.contains("/personen/")
}

/// Companyhouse company profiles live at the bare host root
/// (`/<Firmenname>-<Ort>` or `/<Firmenname>`). Everything that is not a
/// person path and lives under `companyhouse.de` is treated as a company
/// candidate; the `<h1>` heuristic then decides whether to keep the field.
fn is_company_url(url: &str) -> bool {
    let lower = url.to_ascii_lowercase();
    if !lower.contains("companyhouse.de") {
        return false;
    }
    if is_person_url(&lower) {
        return false;
    }
    // Skip obvious non-profile paths so we don't hallucinate a firma_name
    // from a search-results or login page heading.
    const NON_PROFILE_SEGMENTS: &[&str] = &[
        "/login",
        "/register",
        "/suche",
        "/search",
        "/impressum",
        "/agb",
        "/datenschutz",
        "/faq",
        "/preise",
        "/kontakt",
    ];
    !NON_PROFILE_SEGMENTS.iter().any(|seg| lower.contains(seg))
}

#[derive(Debug, Default, PartialEq, Eq)]
struct PersonHeading {
    title: Option<String>,
    first_name: Option<String>,
    last_name: Option<String>,
}

/// Parse a companyhouse `<h1>` person heading like:
///   `Dr. Manfred Schneider`           → title="Dr.", first="Manfred", last="Schneider"
///   `Prof. Dr.-Ing. Anna Müller`      → title="Prof. Dr.-Ing.", first="Anna", last="Müller"
///   `Hans Meier`                       → title=None, first="Hans", last="Meier"
///   `Dr. Anna-Maria von der Heide`     → title="Dr.", first="Anna-Maria", last="von der Heide"
///
/// Strategy: peel off academic-title tokens from the front (whitelist plus
/// any token ending in `.`), then split the remainder into first and last
/// name using nobility-particle awareness ("von", "von der", "zu", "de").
fn parse_person_heading(heading: &str) -> Option<PersonHeading> {
    let tokens: Vec<&str> = heading.split_whitespace().collect();
    if tokens.is_empty() {
        return None;
    }

    let mut idx = 0;
    let mut title_parts: Vec<String> = Vec::new();
    while idx < tokens.len() && is_title_token(tokens[idx]) {
        title_parts.push(tokens[idx].to_string());
        idx += 1;
    }
    let remaining: Vec<&str> = tokens[idx..].to_vec();
    if remaining.is_empty() {
        // Heading is *only* titles (extremely unlikely, but be defensive).
        return None;
    }

    let mut result = PersonHeading::default();
    if !title_parts.is_empty() {
        result.title = Some(title_parts.join(" "));
    }

    match remaining.len() {
        0 => {}
        1 => {
            // Only one token — treat as last name (no first name).
            result.last_name = Some(remaining[0].to_string());
        }
        _ => {
            // Find the start of the last-name run by walking from the right
            // and gobbling any nobility particles ("von", "von der", "zu",
            // "de", "van", "van der") together with the final surname token.
            let split = surname_start(&remaining);
            let first_tokens = &remaining[..split];
            let last_tokens = &remaining[split..];
            if !first_tokens.is_empty() {
                result.first_name = Some(first_tokens.join(" "));
            }
            if !last_tokens.is_empty() {
                result.last_name = Some(last_tokens.join(" "));
            }
        }
    }

    Some(result)
}

fn is_title_token(token: &str) -> bool {
    // Conservative whitelist of common DE academic / professional titles
    // plus the generic "ends in `.`" rule for combos like `Dr.-Ing.` or
    // `Dipl.-Kfm.` that the whitelist may not cover.
    if token.ends_with('.') {
        return true;
    }
    matches!(
        token,
        "Prof"
            | "Dr"
            | "Mag"
            | "Dipl"
            | "Ing"
            | "MBA"
            | "MSc"
            | "MA"
            | "BA"
            | "LL.M"
            | "PhD"
            | "DDr"
    )
}

/// Walk the remaining name tokens from the right to find where the surname
/// starts. The surname is the last token, plus any preceding nobility
/// particles ("von", "von der", "zu", "de", "van", "van der", "del", "di").
fn surname_start(tokens: &[&str]) -> usize {
    if tokens.len() <= 1 {
        return 0;
    }
    // Start: the last token is always part of the surname.
    let mut start = tokens.len() - 1;
    // Walk backwards over particles.
    while start > 0 && is_nobility_particle(tokens[start - 1]) {
        start -= 1;
    }
    // Defensive: every name must have at least one first-name token if we
    // have ≥ 2 tokens total. If particles consumed everything, fall back to
    // "last token is surname, rest is first name".
    if start == 0 {
        return tokens.len() - 1;
    }
    start
}

fn is_nobility_particle(token: &str) -> bool {
    matches!(
        token.to_ascii_lowercase().as_str(),
        "von" | "vom" | "zu" | "zur" | "der" | "den" | "de" | "del" | "di" | "van" | "ten" | "ter"
    )
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn ctx_de() -> SourceCtx<'static> {
        static ROOT: std::sync::OnceLock<PathBuf> = std::sync::OnceLock::new();
        let root = ROOT.get_or_init(|| PathBuf::from(""));
        SourceCtx {
            root,
            country: Some(Country::De),
            mode: super::super::ResearchMode::UpdateFirm,
        }
    }

    fn ctx_at() -> SourceCtx<'static> {
        static ROOT: std::sync::OnceLock<PathBuf> = std::sync::OnceLock::new();
        let root = ROOT.get_or_init(|| PathBuf::from(""));
        SourceCtx {
            root,
            country: Some(Country::At),
            mode: super::super::ResearchMode::UpdateFirm,
        }
    }

    fn ctx_unknown() -> SourceCtx<'static> {
        static ROOT: std::sync::OnceLock<PathBuf> = std::sync::OnceLock::new();
        let root = ROOT.get_or_init(|| PathBuf::from(""));
        SourceCtx {
            root,
            country: None,
            mode: super::super::ResearchMode::UpdateFirm,
        }
    }

    #[test]
    fn registry_metadata() {
        let m = module();
        assert_eq!(m.id(), "companyhouse.de");
        assert!(m.aliases().contains(&"companyhouse"));
        assert!(m.aliases().contains(&"ch_de"));
        assert_eq!(m.tier(), Tier::S);
        assert_eq!(m.countries(), &[Country::De]);
        assert!(m.requires_credential().is_none());
        assert!(m
            .authoritative_for()
            .iter()
            .any(|f| *f == FieldKey::PersonTitel));
    }

    #[test]
    fn shape_query_de_pins_domain() {
        let shaped = module()
            .shape_query("Manfred Schneider", &ctx_de())
            .expect("DE must shape");
        assert!(shaped.query.contains("Manfred Schneider"));
        assert!(shaped.query.contains("companyhouse.de"));
        assert_eq!(shaped.domains, vec!["companyhouse.de".to_string()]);
    }

    #[test]
    fn shape_query_unknown_country_still_shapes() {
        // Companyhouse-Matrix referenziert keine Nicht-DE-Zeile, also ist
        // None ⇒ erlaubt; Source-Filtering nach Land übernimmt Phase 4.
        assert!(module()
            .shape_query("Manfred Schneider", &ctx_unknown())
            .is_some());
    }

    #[test]
    fn shape_query_non_de_returns_none() {
        assert!(module()
            .shape_query("Manfred Schneider", &ctx_at())
            .is_none());
    }

    #[test]
    fn shape_query_empty_returns_none() {
        assert!(module().shape_query("   ", &ctx_de()).is_none());
    }

    #[test]
    fn no_fetch_direct_override() {
        // Crawl-Pfad — Trait-Default `None` ist der Vertrag.
        assert!(module()
            .fetch_direct(&ctx_de(), "Bayer AG")
            .is_none());
    }

    /// End-to-end extraction against a frozen Companyhouse person profile.
    ///
    /// Fixture-Ursprung: rekonstruiertes Layout einer
    /// `companyhouse.de/person/<Vorname>-<Nachname>`-Seite, manuell aus
    /// dokumentierten Schema-Konventionen aufgebaut, weil die Live-Site
    /// hinter Cloudflare und der Archive-Service zum Implementations-
    /// Zeitpunkt offline waren. Struktur entspricht dem öffentlich
    /// indexierten Heading-Pattern (Titel + Vorname + Nachname im `<h1>`,
    /// gefolgt von Mandaten-Liste).
    #[test]
    fn extract_person_fixture() {
        let html = include_str!(
            "../../fixtures/sources/companyhouse/person_manfred_schneider.html"
        );
        let page = SourceReadResult {
            url: "https://www.companyhouse.de/person/Manfred-Schneider".to_string(),
            title: "Dr. Manfred Schneider - Mandate, Beteiligungen | CompanyHouse".to_string(),
            summary: String::new(),
            text: html.to_string(),
            is_pdf: false,
            excerpts: Vec::new(),
            find_results: Vec::new(),
        };
        let fields = module().extract_fields(&page);
        let by_key: std::collections::HashMap<FieldKey, &FieldEvidence> =
            fields.iter().map(|(k, v)| (*k, v)).collect();

        let title = by_key
            .get(&FieldKey::PersonTitel)
            .expect("person_titel extracted");
        assert_eq!(title.value, "Dr.");
        assert_eq!(title.confidence, Confidence::Medium);

        let first = by_key
            .get(&FieldKey::PersonVorname)
            .expect("person_vorname extracted");
        assert_eq!(first.value, "Manfred");
        assert_eq!(first.confidence, Confidence::Medium);

        let last = by_key
            .get(&FieldKey::PersonNachname)
            .expect("person_nachname extracted");
        assert_eq!(last.value, "Schneider");
        assert_eq!(last.confidence, Confidence::Medium);

        // Person-Seite schreibt keinen Firmennamen.
        assert!(by_key.get(&FieldKey::FirmaName).is_none());
    }

    /// End-to-end extraction against a frozen Companyhouse company profile.
    #[test]
    fn extract_company_fixture() {
        let html = include_str!(
            "../../fixtures/sources/companyhouse/firma_bayer_ag.html"
        );
        let page = SourceReadResult {
            url: "https://www.companyhouse.de/Bayer-AG-Leverkusen".to_string(),
            title: "Bayer AG, Leverkusen | CompanyHouse".to_string(),
            summary: String::new(),
            text: html.to_string(),
            is_pdf: false,
            excerpts: Vec::new(),
            find_results: Vec::new(),
        };
        let fields = module().extract_fields(&page);
        let by_key: std::collections::HashMap<FieldKey, &FieldEvidence> =
            fields.iter().map(|(k, v)| (*k, v)).collect();

        let name = by_key
            .get(&FieldKey::FirmaName)
            .expect("firma_name extracted");
        assert_eq!(name.value, "Bayer AG");
        assert_eq!(name.confidence, Confidence::High);

        // Firmen-Seite schreibt keine Personen-Felder.
        assert!(by_key.get(&FieldKey::PersonTitel).is_none());
        assert!(by_key.get(&FieldKey::PersonVorname).is_none());
        assert!(by_key.get(&FieldKey::PersonNachname).is_none());
    }

    #[test]
    fn extract_unrelated_html_returns_nothing() {
        let page = SourceReadResult {
            url: "https://www.companyhouse.de/login".to_string(),
            title: "Login".to_string(),
            summary: String::new(),
            text: "<html><body><h1>Anmelden</h1></body></html>".to_string(),
            is_pdf: false,
            excerpts: Vec::new(),
            find_results: Vec::new(),
        };
        let fields = module().extract_fields(&page);
        assert!(
            fields.is_empty(),
            "login page must not emit fields, got: {fields:?}"
        );
    }

    #[test]
    fn extract_no_h1_returns_nothing() {
        let page = SourceReadResult {
            url: "https://www.companyhouse.de/person/Empty".to_string(),
            title: String::new(),
            summary: String::new(),
            text: "<html><body><p>no heading here</p></body></html>".to_string(),
            is_pdf: false,
            excerpts: Vec::new(),
            find_results: Vec::new(),
        };
        let fields = module().extract_fields(&page);
        assert!(fields.is_empty());
    }

    #[test]
    fn extract_skips_pdf() {
        let page = SourceReadResult {
            url: "https://www.companyhouse.de/person/Manfred-Schneider".to_string(),
            title: String::new(),
            summary: String::new(),
            text: "Dr. Manfred Schneider".to_string(),
            is_pdf: true,
            excerpts: Vec::new(),
            find_results: Vec::new(),
        };
        let fields = module().extract_fields(&page);
        assert!(fields.is_empty(), "PDFs are skipped by contract");
    }

    #[test]
    fn parse_person_heading_simple() {
        let parsed = parse_person_heading("Dr. Manfred Schneider").expect("parsed");
        assert_eq!(parsed.title.as_deref(), Some("Dr."));
        assert_eq!(parsed.first_name.as_deref(), Some("Manfred"));
        assert_eq!(parsed.last_name.as_deref(), Some("Schneider"));
    }

    #[test]
    fn parse_person_heading_multi_title() {
        let parsed = parse_person_heading("Prof. Dr.-Ing. Anna Müller").expect("parsed");
        assert_eq!(parsed.title.as_deref(), Some("Prof. Dr.-Ing."));
        assert_eq!(parsed.first_name.as_deref(), Some("Anna"));
        assert_eq!(parsed.last_name.as_deref(), Some("Müller"));
    }

    #[test]
    fn parse_person_heading_no_title() {
        let parsed = parse_person_heading("Hans Meier").expect("parsed");
        assert!(parsed.title.is_none());
        assert_eq!(parsed.first_name.as_deref(), Some("Hans"));
        assert_eq!(parsed.last_name.as_deref(), Some("Meier"));
    }

    #[test]
    fn parse_person_heading_with_nobility_particle() {
        let parsed = parse_person_heading("Dr. Anna-Maria von der Heide").expect("parsed");
        assert_eq!(parsed.title.as_deref(), Some("Dr."));
        assert_eq!(parsed.first_name.as_deref(), Some("Anna-Maria"));
        assert_eq!(parsed.last_name.as_deref(), Some("von der Heide"));
    }

    #[test]
    fn parse_person_heading_single_token() {
        let parsed = parse_person_heading("Schneider").expect("parsed");
        assert!(parsed.title.is_none());
        assert!(parsed.first_name.is_none());
        assert_eq!(parsed.last_name.as_deref(), Some("Schneider"));
    }

    #[test]
    fn parse_person_heading_empty() {
        assert!(parse_person_heading("").is_none());
        assert!(parse_person_heading("   ").is_none());
    }

    #[test]
    fn is_person_url_matches_canonical_path() {
        assert!(is_person_url(
            "https://www.companyhouse.de/person/Manfred-Schneider"
        ));
        assert!(is_person_url(
            "https://www.companyhouse.de/Personen/Foo"
        ));
        assert!(!is_person_url("https://www.companyhouse.de/Bayer-AG"));
        assert!(!is_person_url(
            "https://www.companyhouse.de/Personalmanagement-Berlin-First-UG-Berlin"
        ));
    }

    #[test]
    fn is_company_url_excludes_known_non_profiles() {
        assert!(is_company_url(
            "https://www.companyhouse.de/Bayer-AG-Leverkusen"
        ));
        assert!(!is_company_url(
            "https://www.companyhouse.de/person/Manfred-Schneider"
        ));
        assert!(!is_company_url("https://www.companyhouse.de/login"));
        assert!(!is_company_url(
            "https://www.companyhouse.de/suche?q=Bayer"
        ));
        assert!(!is_company_url("https://example.com/Bayer-AG"));
    }

    /// Live-Smoke gegen die echte companyhouse.de-Profilseite.
    ///
    /// Standardmäßig ignoriert. Companyhouse ist Cloudflare-geschützt; der
    /// Test akzeptiert einen Block (403, Captcha-HTML) als „skip" — er
    /// schlägt nur dann fehl, wenn ein 200-Body zurückkommt und der Parser
    /// nichts daraus zieht.
    ///   `cargo test -p ctox-web-stack -- --ignored sources::companyhouse::live`
    #[test]
    #[ignore = "live network; run with: cargo test -- --ignored sources::companyhouse::live_person"]
    fn live_person() {
        let url = "https://www.companyhouse.de/person/Manfred-Schneider";
        let response = ureq::get(url)
            .set(
                "User-Agent",
                "Mozilla/5.0 (Macintosh; Intel Mac OS X 14_0) AppleWebKit/537.36 \
                 (KHTML, like Gecko) Chrome/120.0 Safari/537.36",
            )
            .set("Accept-Language", "de-DE,de;q=0.9")
            .call();
        let body = match response {
            Ok(resp) => resp.into_string().expect("read body"),
            Err(ureq::Error::Status(code, resp)) => {
                eprintln!("companyhouse.de live request returned HTTP {code} (skip)");
                let _ = resp.into_string();
                return;
            }
            Err(err) => {
                eprintln!("companyhouse.de live request failed (skip): {err}");
                return;
            }
        };
        // Detect Cloudflare interstitial — treat as skip, not failure.
        if body.contains("Cloudflare") && body.contains("gesperrt") {
            eprintln!("companyhouse.de live request hit Cloudflare block (skip)");
            return;
        }
        let page = SourceReadResult {
            url: url.to_string(),
            title: String::new(),
            summary: String::new(),
            text: body,
            is_pdf: false,
            excerpts: Vec::new(),
            find_results: Vec::new(),
        };
        let fields = module().extract_fields(&page);
        let name_field = fields
            .iter()
            .find(|(k, _)| *k == FieldKey::PersonNachname)
            .map(|(_, v)| v)
            .expect("live person_nachname");
        assert!(!name_field.value.is_empty());
        assert!(name_field.confidence >= Confidence::Medium);
    }
}
