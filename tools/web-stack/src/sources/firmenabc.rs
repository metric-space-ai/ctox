//! `firmenabc.at` — Tier S, AT only.
//!
//! Strukturierte Profilseiten mit Firma + Anschrift + Person + Geschlecht.
//! Crawl-Template-Quelle (Phase 1) und Struktur-Vorlage für weitere
//! Crawl-Adapter (Northdata, Bundesanzeiger, Companyhouse, Handelsregister).
//!
//! Extraktions-Plan (high-level):
//!   * Firma-Stammdaten (`firma_name`, `firma_anschrift`, `firma_plz`,
//!     `firma_ort`) kommen aus dem strukturierten JSON-LD-Block vom Typ
//!     `Organization`/`SportsClub`/… mit eingebetteter `PostalAddress`.
//!     Diese Blöcke werden vom CMS (TYPO3) erzeugt und sind seitenweise
//!     stabil → `Confidence::High`.
//!   * `firma_email` / `firma_domain` werden aus den ausgezeichneten
//!     `<a class="company-profile-email|company-profile-website">`-Links
//!     gelesen → `Confidence::High`.
//!   * Person-Felder (`person_geschlecht`, `person_titel`, `person_vorname`,
//!     `person_nachname`) werden aus dem „Geschäftsführer“-Block neben dem
//!     Firmenbuch-Auszug extrahiert. firmenabc.at druckt Personen als
//!     `Herr|Frau [Titel] Nachname Vorname` (Nachname zuerst); die Heuristik
//!     ist robust genug für GmbH/AG-Stammdaten, aber laut Excel-Matrix mit
//!     `*` markiert → `Confidence::Medium`.

use scraper::{ElementRef, Html, Selector};
use serde_json::Value;
use std::sync::OnceLock;

use super::{
    Confidence, Country, FieldEvidence, FieldKey, ShapedQuery, SourceCtx, SourceModule,
    SourceReadResult, Tier,
};

const ID: &str = "firmenabc.at";
const DOMAIN: &str = "firmenabc.at";

struct Firmenabc;

impl SourceModule for Firmenabc {
    fn id(&self) -> &'static str {
        ID
    }

    fn aliases(&self) -> &'static [&'static str] {
        &["firmenabc"]
    }

    fn tier(&self) -> Tier {
        Tier::S
    }

    fn countries(&self) -> &'static [Country] {
        &[Country::At]
    }

    fn authoritative_for(&self) -> &'static [FieldKey] {
        // Aus EXCEL_MATRIX.md: firmenabc.at ist AT-only, deckt Firma-Stammdaten
        // plus die Person-Stammdaten (mit Konfidenz-Asterisk in der Excel).
        &[
            FieldKey::FirmaName,
            FieldKey::FirmaAnschrift,
            FieldKey::FirmaPlz,
            FieldKey::FirmaOrt,
            FieldKey::FirmaEmail,
            FieldKey::FirmaDomain,
            FieldKey::PersonGeschlecht,
            FieldKey::PersonTitel,
            FieldKey::PersonVorname,
            FieldKey::PersonNachname,
        ]
    }

    /// Crawl-Pfad: Query wird auf die Domain gepinnt; die Volltext-Query
    /// nimmt der Suchengpass-Cascade (Google/Brave) ab. AT-only — andere
    /// Länder bekommen `None` und werden vom Orchestrator übersprungen.
    fn shape_query(&self, query: &str, ctx: &SourceCtx<'_>) -> Option<ShapedQuery> {
        if ctx.country != Some(Country::At) {
            return None;
        }
        let trimmed = query.trim();
        if trimmed.is_empty() {
            return None;
        }
        Some(ShapedQuery {
            // Domain-Pin schon im Query-Text, damit auch Provider ohne
            // `site:`-Operator (DDG, manche Brave-Modi) die Domain priorisieren.
            query: format!("{trimmed} site:{DOMAIN}"),
            domains: vec![DOMAIN.to_string()],
        })
    }

    fn extract_fields(&self, page: &SourceReadResult) -> Vec<(FieldKey, FieldEvidence)> {
        if page.is_pdf {
            return Vec::new();
        }
        extract_from_html(page.html_source(), &page.url)
    }
}

static MODULE: Firmenabc = Firmenabc;

pub fn module() -> &'static dyn SourceModule {
    &MODULE
}

// ---------------------------------------------------------------------------
// Selectors — `scraper::Selector::parse` ist nicht `const`, deshalb `OnceLock`.
// ---------------------------------------------------------------------------

fn json_ld_selector() -> &'static Selector {
    static SEL: OnceLock<Selector> = OnceLock::new();
    SEL.get_or_init(|| {
        Selector::parse(r#"script[type="application/ld+json"]"#)
            .expect("valid json-ld selector")
    })
}

fn email_selector() -> &'static Selector {
    static SEL: OnceLock<Selector> = OnceLock::new();
    SEL.get_or_init(|| {
        Selector::parse("a.company-profile-email[href^=\"mailto:\"]")
            .expect("valid email selector")
    })
}

fn website_selector() -> &'static Selector {
    static SEL: OnceLock<Selector> = OnceLock::new();
    SEL.get_or_init(|| {
        Selector::parse("a.company-profile-website[href]")
            .expect("valid website selector")
    })
}

fn person_heading_selector() -> &'static Selector {
    static SEL: OnceLock<Selector> = OnceLock::new();
    SEL.get_or_init(|| Selector::parse("h4").expect("valid h4 selector"))
}

fn person_link_selector() -> &'static Selector {
    static SEL: OnceLock<Selector> = OnceLock::new();
    SEL.get_or_init(|| {
        Selector::parse("a[href^=\"/person/\"]")
            .expect("valid person link selector")
    })
}

fn h1_selector() -> &'static Selector {
    static SEL: OnceLock<Selector> = OnceLock::new();
    SEL.get_or_init(|| Selector::parse("h1").expect("valid h1 selector"))
}

// ---------------------------------------------------------------------------
// Core extraction
// ---------------------------------------------------------------------------

fn extract_from_html(html: &str, source_url: &str) -> Vec<(FieldKey, FieldEvidence)> {
    let document = Html::parse_document(html);
    let mut out: Vec<(FieldKey, FieldEvidence)> = Vec::new();

    // ---- (1) JSON-LD: Organization / SportsClub / LocalBusiness mit PostalAddress
    if let Some(org) = find_organization_json_ld(&document) {
        if let Some(name) = string_field(&org, "name") {
            push(
                &mut out,
                FieldKey::FirmaName,
                name,
                Confidence::High,
                source_url,
                Some("schema.org Organization.name"),
            );
        }
        if let Some(address) = org.get("address").and_then(Value::as_object) {
            if let Some(street) = address.get("streetAddress").and_then(Value::as_str) {
                push(
                    &mut out,
                    FieldKey::FirmaAnschrift,
                    street.trim().to_string(),
                    Confidence::High,
                    source_url,
                    Some("schema.org PostalAddress.streetAddress"),
                );
            }
            if let Some(zip) = address.get("postalCode").and_then(Value::as_str) {
                push(
                    &mut out,
                    FieldKey::FirmaPlz,
                    zip.trim().to_string(),
                    Confidence::High,
                    source_url,
                    Some("schema.org PostalAddress.postalCode"),
                );
            }
            if let Some(city) = address.get("addressLocality").and_then(Value::as_str) {
                push(
                    &mut out,
                    FieldKey::FirmaOrt,
                    city.trim().to_string(),
                    Confidence::High,
                    source_url,
                    Some("schema.org PostalAddress.addressLocality"),
                );
            }
        }
    }

    // ---- (1b) Fallback: <h1>-Titel, falls JSON-LD fehlt.
    if !out.iter().any(|(k, _)| *k == FieldKey::FirmaName) {
        if let Some(h1) = document.select(h1_selector()).next() {
            let raw = node_text(&h1);
            // firmenabc.at packt häufig die Standort-Zeile als zweiten span
            // ("Red Bull GmbH" + "in Fuschl am See"). Der Firmenname ist alles
            // vor dem ersten "in <Ort>"-Marker.
            let cleaned = raw.split(" in ").next().unwrap_or(&raw).trim().to_string();
            if !cleaned.is_empty() {
                push(
                    &mut out,
                    FieldKey::FirmaName,
                    cleaned,
                    Confidence::Medium,
                    source_url,
                    Some("h1 fallback"),
                );
            }
        }
    }

    // ---- (2) Email + Domain aus den ausgezeichneten Contact-Links
    if let Some(anchor) = document.select(email_selector()).next() {
        if let Some(href) = anchor.value().attr("href") {
            let email = href.trim_start_matches("mailto:").trim().to_string();
            if !email.is_empty() {
                push(
                    &mut out,
                    FieldKey::FirmaEmail,
                    email,
                    Confidence::High,
                    source_url,
                    Some("a.company-profile-email"),
                );
            }
        }
    }
    if let Some(anchor) = document.select(website_selector()).next() {
        if let Some(href) = anchor.value().attr("href") {
            if let Some(domain) = host_of(href) {
                push(
                    &mut out,
                    FieldKey::FirmaDomain,
                    domain,
                    Confidence::High,
                    source_url,
                    Some("a.company-profile-website"),
                );
            }
        }
    }

    // ---- (3) Personen aus dem „Geschäftsführer“-Block (erste juristische
    //         Vertretung). Layout:
    //              <h4>Geschäftsführer</h4>
    //              … <a href="/person/…">Herr|Frau [Titel.] Nachname Vorname</a>
    //         Wir nehmen nur den ersten Geschäftsführer-Eintrag als
    //         „primary person“; weitere Personen sind für die Excel-Spalten
    //         redundant und Sache von person-research-Phase 4.
    if let Some(person_anchor) = first_management_person(&document) {
        let parsed = parse_management_label(&person_anchor);
        if let Some(gender) = parsed.gender {
            push(
                &mut out,
                FieldKey::PersonGeschlecht,
                gender.to_string(),
                Confidence::Medium,
                source_url,
                Some("Geschäftsführer-Prefix (Herr/Frau)"),
            );
        }
        if let Some(title) = parsed.title {
            push(
                &mut out,
                FieldKey::PersonTitel,
                title,
                Confidence::Medium,
                source_url,
                Some("Geschäftsführer-Titel-Prefix"),
            );
        }
        if let Some(first) = parsed.first_name {
            push(
                &mut out,
                FieldKey::PersonVorname,
                first,
                Confidence::Medium,
                source_url,
                Some("Geschäftsführer-Eintrag (Nachname zuerst)"),
            );
        }
        if let Some(last) = parsed.last_name {
            push(
                &mut out,
                FieldKey::PersonNachname,
                last,
                Confidence::Medium,
                source_url,
                Some("Geschäftsführer-Eintrag (Nachname zuerst)"),
            );
        }
    }

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

fn string_field(value: &Value, key: &str) -> Option<String> {
    value
        .get(key)
        .and_then(Value::as_str)
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
}

/// Find the first JSON-LD object that looks like a company (has both `name`
/// and a `PostalAddress` under `address`). The page also has WebPage and
/// BreadcrumbList JSON-LD blocks — those are skipped by this predicate.
fn find_organization_json_ld(document: &Html) -> Option<Value> {
    for script in document.select(json_ld_selector()) {
        let raw = script.text().collect::<String>();
        let parsed: Value = match serde_json::from_str(raw.trim()) {
            Ok(value) => value,
            Err(_) => continue,
        };
        for candidate in flatten_json_ld(parsed) {
            if looks_like_organization(&candidate) {
                return Some(candidate);
            }
        }
    }
    None
}

/// JSON-LD can be a single object, an array, or wrap entries inside `@graph`.
fn flatten_json_ld(value: Value) -> Vec<Value> {
    match value {
        Value::Array(items) => items.into_iter().flat_map(flatten_json_ld).collect(),
        Value::Object(ref obj) if obj.contains_key("@graph") => {
            match obj.get("@graph").cloned().unwrap_or(Value::Null) {
                Value::Array(items) => items.into_iter().flat_map(flatten_json_ld).collect(),
                other => vec![other],
            }
        }
        other => vec![other],
    }
}

fn looks_like_organization(value: &Value) -> bool {
    let Some(obj) = value.as_object() else {
        return false;
    };
    // Has a non-empty `name`.
    let name_ok = obj
        .get("name")
        .and_then(Value::as_str)
        .map(|s| !s.trim().is_empty())
        .unwrap_or(false);
    // Address must be a PostalAddress-shaped object.
    let addr_ok = obj
        .get("address")
        .and_then(Value::as_object)
        .map(|addr| {
            addr.get("streetAddress").is_some()
                || addr.get("postalCode").is_some()
                || addr.get("addressLocality").is_some()
        })
        .unwrap_or(false);
    name_ok && addr_ok
}

/// Extract the bare hostname from an URL string, stripping `www.` and any
/// trailing path/query. Returns `None` for relative / malformed URLs.
fn host_of(raw: &str) -> Option<String> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return None;
    }
    // Tolerate URLs without scheme (firmenabc.at sometimes prints `redbull.at`).
    let with_scheme = if trimmed.starts_with("http://") || trimmed.starts_with("https://") {
        trimmed.to_string()
    } else {
        format!("http://{trimmed}")
    };
    let parsed = url::Url::parse(&with_scheme).ok()?;
    let host = parsed.host_str()?;
    let bare = host.strip_prefix("www.").unwrap_or(host);
    if bare.is_empty() {
        None
    } else {
        Some(bare.to_ascii_lowercase())
    }
}

/// Walk the DOM looking for an `<h4>Geschäftsführer</h4>` heading and return
/// the first `<a href="/person/...">` that appears after it. The CMS keeps the
/// block in DOM order, so a forward scan is enough — we don't need ancestor
/// climbing.
fn first_management_person<'a>(document: &'a Html) -> Option<ElementRef<'a>> {
    // Some profiles use "Vorstand" (AG) or "Geschäftsführer" (GmbH).
    let mut accept_following = false;
    for element in document.root_element().descendants().filter_map(ElementRef::wrap) {
        if Selector::matches(person_heading_selector(), &element)
            && is_management_heading(&element)
        {
            accept_following = true;
            continue;
        }
        if accept_following
            && Selector::matches(person_link_selector(), &element)
            && !node_text(&element).is_empty()
        {
            return Some(element);
        }
    }
    None
}

fn is_management_heading(node: &ElementRef<'_>) -> bool {
    let text = node_text(node).to_ascii_lowercase();
    text == "geschäftsführer"
        || text == "geschaftsfuhrer"
        || text == "geschäftsführerin"
        || text == "vorstand"
}

#[derive(Debug, Default)]
struct PersonLabel {
    gender: Option<&'static str>,
    title: Option<String>,
    first_name: Option<String>,
    last_name: Option<String>,
}

/// firmenabc.at renders management entries as:
///   `Herr Watzlawick Franz`
///   `Frau Dr. Maier Anna-Maria`
///   `Herr Mag. Müller Peter`
/// i.e. "[Anrede] [Titel.] Nachname Vorname". This parser is forgiving:
/// it accepts missing title, double-barrel first names, and umlauts.
fn parse_management_label(node: &ElementRef<'_>) -> PersonLabel {
    let text = node_text(node);
    let mut tokens: Vec<&str> = text.split_whitespace().collect();
    let mut label = PersonLabel::default();

    if let Some(first) = tokens.first().copied() {
        match first {
            "Herr" => {
                label.gender = Some("m");
                tokens.remove(0);
            }
            "Frau" => {
                label.gender = Some("w");
                tokens.remove(0);
            }
            _ => {}
        }
    }

    // Collect title tokens (anything ending in '.' and not a name fragment).
    let mut title_parts: Vec<String> = Vec::new();
    while let Some(token) = tokens.first().copied() {
        if is_title_token(token) {
            title_parts.push(token.to_string());
            tokens.remove(0);
        } else {
            break;
        }
    }
    if !title_parts.is_empty() {
        label.title = Some(title_parts.join(" "));
    }

    // What's left should be `<Nachname> <Vorname...>`. The Austrian convention
    // on firmenabc.at is last-name-first; we honour it but stay defensive:
    // if there is exactly one token, treat it as last name.
    match tokens.len() {
        0 => {}
        1 => {
            label.last_name = Some(tokens[0].to_string());
        }
        _ => {
            label.last_name = Some(tokens[0].to_string());
            label.first_name = Some(tokens[1..].join(" "));
        }
    }
    label
}

fn is_title_token(token: &str) -> bool {
    // Conservative whitelist of common AT academic / honorific titles.
    // Anything not in this list is treated as a name fragment so that
    // surnames like "Dr.-Ing." or company name fragments don't get
    // mis-classified as titles.
    matches!(
        token,
        "Dr."
            | "Dr"
            | "Mag."
            | "Mag"
            | "DI"
            | "Dipl.-Ing."
            | "Dipl."
            | "Ing."
            | "Prof."
            | "Prof"
            | "MBA"
            | "MSc"
            | "MA"
            | "BA"
            | "DDr."
            | "Mag.a"
            | "Dr.in"
    )
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn ctx_at() -> SourceCtx<'static> {
        // Static empty path is fine — the firmenabc module never reads
        // anything from `root`, and the borrow lives as long as the test.
        static ROOT: std::sync::OnceLock<PathBuf> = std::sync::OnceLock::new();
        let root = ROOT.get_or_init(|| PathBuf::from(""));
        SourceCtx {
            root,
            country: Some(Country::At),
            mode: super::super::ResearchMode::UpdateFirm,
        }
    }

    fn ctx_de() -> SourceCtx<'static> {
        static ROOT: std::sync::OnceLock<PathBuf> = std::sync::OnceLock::new();
        let root = ROOT.get_or_init(|| PathBuf::from(""));
        SourceCtx {
            root,
            country: Some(Country::De),
            mode: super::super::ResearchMode::UpdateFirm,
        }
    }

    #[test]
    fn registry_metadata() {
        let m = module();
        assert_eq!(m.id(), "firmenabc.at");
        assert!(m.aliases().contains(&"firmenabc"));
        assert_eq!(m.tier(), Tier::S);
        assert_eq!(m.countries(), &[Country::At]);
        assert!(m.requires_credential().is_none());
        assert!(m
            .authoritative_for()
            .iter()
            .any(|f| *f == FieldKey::FirmaName));
        assert!(m
            .authoritative_for()
            .iter()
            .any(|f| *f == FieldKey::PersonGeschlecht));
    }

    #[test]
    fn shape_query_at_pins_domain() {
        let shaped = module()
            .shape_query("Red Bull GmbH", &ctx_at())
            .expect("AT must shape");
        assert!(shaped.query.contains("Red Bull GmbH"));
        assert!(shaped.query.contains("firmenabc.at"));
        assert_eq!(shaped.domains, vec!["firmenabc.at".to_string()]);
    }

    #[test]
    fn shape_query_non_at_returns_none() {
        assert!(module().shape_query("Bosch GmbH", &ctx_de()).is_none());
    }

    #[test]
    fn shape_query_empty_returns_none() {
        assert!(module().shape_query("   ", &ctx_at()).is_none());
    }

    #[test]
    fn no_fetch_direct_override() {
        // Crawl-Pfad — der Trait-Default ist `None`, das ist der Vertrag.
        assert!(module().fetch_direct(&ctx_at(), "Red Bull GmbH").is_none());
    }

    /// End-to-end extraction against a frozen Red Bull GmbH profile.
    ///
    /// Fixture-Ursprung: `https://www.firmenabc.at/red-bull-gmbh_gsn`,
    /// abgerufen über `web.archive.org` Snapshot `20250917001953`
    /// (Originaldatum 2025-09-17). Gespeichert als HTML-Body genau so,
    /// wie ihn ein crawl-pfad-Adapter vom Webstack bekäme.
    #[test]
    fn extract_red_bull_fixture() {
        let html = include_str!(
            "../../fixtures/sources/firmenabc/red_bull_gmbh.html"
        );
        let page = SourceReadResult {
            url: "https://www.firmenabc.at/red-bull-gmbh_gsn".to_string(),
            title: "Red Bull GmbH in Fuschl am See".to_string(),
            summary: String::new(),
            text: html.to_string(),
            is_pdf: false,
            excerpts: Vec::new(),
            find_results: Vec::new(),
            raw_html: None,
        };
        let fields = module().extract_fields(&page);

        // Convert to lookup for readability.
        let by_key: std::collections::HashMap<FieldKey, &FieldEvidence> =
            fields.iter().map(|(k, v)| (*k, v)).collect();

        let name = by_key
            .get(&FieldKey::FirmaName)
            .expect("firma_name extracted");
        assert_eq!(name.value, "Red Bull GmbH");
        assert_eq!(name.confidence, Confidence::High);

        let street = by_key
            .get(&FieldKey::FirmaAnschrift)
            .expect("firma_anschrift extracted");
        assert_eq!(street.value, "Am Brunnen 1");
        assert_eq!(street.confidence, Confidence::High);

        let zip = by_key
            .get(&FieldKey::FirmaPlz)
            .expect("firma_plz extracted");
        assert_eq!(zip.value, "5330");
        assert_eq!(zip.confidence, Confidence::High);

        let city = by_key
            .get(&FieldKey::FirmaOrt)
            .expect("firma_ort extracted");
        assert_eq!(city.value, "Fuschl am See");
        assert_eq!(city.confidence, Confidence::High);

        let email = by_key
            .get(&FieldKey::FirmaEmail)
            .expect("firma_email extracted");
        assert_eq!(email.value, "info@redbull.at");
        assert_eq!(email.confidence, Confidence::High);

        let domain = by_key
            .get(&FieldKey::FirmaDomain)
            .expect("firma_domain extracted");
        assert_eq!(domain.value, "redbull.at");
        assert_eq!(domain.confidence, Confidence::High);

        // Geschäftsführer: erster Eintrag im Profil ist „Herr Watzlawick Franz“.
        let gender = by_key
            .get(&FieldKey::PersonGeschlecht)
            .expect("person_geschlecht extracted");
        assert_eq!(gender.value, "m");
        assert_eq!(gender.confidence, Confidence::Medium);

        let last = by_key
            .get(&FieldKey::PersonNachname)
            .expect("person_nachname extracted");
        assert_eq!(last.value, "Watzlawick");
        assert_eq!(last.confidence, Confidence::Medium);

        let first = by_key
            .get(&FieldKey::PersonVorname)
            .expect("person_vorname extracted");
        assert_eq!(first.value, "Franz");
        assert_eq!(first.confidence, Confidence::Medium);

        // Kein akademischer Titel beim ersten GF → person_titel fehlt.
        assert!(by_key.get(&FieldKey::PersonTitel).is_none());
    }

    #[test]
    fn parse_management_label_with_title() {
        // Construct a tiny DOM around a synthesized link.
        let html = r#"<html><body><a href="/person/maier-anna_x">Frau Dr. Maier Anna-Maria</a></body></html>"#;
        let doc = Html::parse_document(html);
        let anchor = doc
            .select(&Selector::parse("a").expect("selector"))
            .next()
            .expect("anchor");
        let parsed = parse_management_label(&anchor);
        assert_eq!(parsed.gender, Some("w"));
        assert_eq!(parsed.title.as_deref(), Some("Dr."));
        assert_eq!(parsed.last_name.as_deref(), Some("Maier"));
        assert_eq!(parsed.first_name.as_deref(), Some("Anna-Maria"));
    }

    #[test]
    fn host_of_strips_www_and_path() {
        assert_eq!(host_of("http://www.redbull.at").as_deref(), Some("redbull.at"));
        assert_eq!(
            host_of("https://WWW.example.AT/some/path?q=1").as_deref(),
            Some("example.at")
        );
        assert_eq!(host_of("example.org").as_deref(), Some("example.org"));
        assert_eq!(host_of(""), None);
    }

    #[test]
    fn extract_empty_on_unrelated_html() {
        let page = SourceReadResult {
            url: "https://example.com/".to_string(),
            title: "x".to_string(),
            summary: String::new(),
            text: "<html><body><p>hello</p></body></html>".to_string(),
            is_pdf: false,
            excerpts: Vec::new(),
            find_results: Vec::new(),
            raw_html: None,
        };
        let fields = module().extract_fields(&page);
        // No JSON-LD, no anchors → keine Felder. Reine HTML-Heuristiken
        // (h1-Fallback) sind hier zwar erlaubt, "hello" wird aber gefiltert,
        // weil das pure h1-Fallback im Test-DOM gar kein h1 hat.
        assert!(
            fields
                .iter()
                .all(|(k, _)| *k != FieldKey::FirmaAnschrift
                    && *k != FieldKey::FirmaPlz
                    && *k != FieldKey::FirmaOrt
                    && *k != FieldKey::FirmaEmail
                    && *k != FieldKey::FirmaDomain),
            "unexpected fields: {fields:?}"
        );
    }

    /// Live-Smoke gegen die echte firmenabc.at-Profilseite.
    ///
    /// Standardmäßig ignoriert; explizit ausführen mit:
    ///   `cargo test -p ctox-web-stack -- --ignored sources::firmenabc::live`
    #[test]
    #[ignore = "live network; run with: cargo test -- --ignored sources::firmenabc::live_red_bull"]
    fn live_red_bull() {
        let url = "https://www.firmenabc.at/red-bull-gmbh_gsn";
        let response = ureq::get(url)
            .set(
                "User-Agent",
                "Mozilla/5.0 (Macintosh; Intel Mac OS X 14_0) AppleWebKit/537.36 \
                 (KHTML, like Gecko) Chrome/120.0 Safari/537.36",
            )
            .set("Accept-Language", "de-AT,de;q=0.9")
            .call();
        let body = match response {
            Ok(resp) => resp.into_string().expect("read body"),
            Err(err) => {
                eprintln!("firmenabc.at live request failed (skip): {err}");
                return;
            }
        };
        let page = SourceReadResult {
            url: url.to_string(),
            title: String::new(),
            summary: String::new(),
            text: body,
            is_pdf: false,
            excerpts: Vec::new(),
            find_results: Vec::new(),
            raw_html: None,
        };
        let fields = module().extract_fields(&page);
        let name = fields
            .iter()
            .find(|(k, _)| *k == FieldKey::FirmaName)
            .map(|(_, v)| v)
            .expect("live firma_name");
        assert!(name.value.contains("Red Bull"));
        assert!(name.confidence >= Confidence::Medium);
    }
}
