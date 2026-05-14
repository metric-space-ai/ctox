//! `bundesanzeiger.de` — Tier P, DE only.
//!
//! Autoritative Pflichtveröffentlichung deutscher Jahresabschlüsse nach
//! HGB §325. Jeder offenlegungspflichtige Jahresabschluss landet im
//! Bundesanzeiger und ist rechtlich verbindlich — daher Tier P, und die
//! Tabellenwerte (`Umsatzerlöse`, `Arbeitnehmer im Jahresdurchschnitt`)
//! tragen `Confidence::High`, sobald sie aus der amtlichen GuV /
//! Anhangs-Tabelle gezogen werden.
//!
//! Extraktions-Plan (Crawl-Pfad):
//!
//!   * **Such-Trefferseite (`/pub/de/suche?...`)** — HTML mit
//!     Treffer-Tabelle. Jeder Treffer hat ein `data-id`-Attribut, der
//!     Firmenname steht im `.first` / `.title` Block; daraus lesen wir
//!     `firma_name` (Confidence::High, weil Treffer kommt aus dem
//!     offiziellen Index).
//!   * **Detail-Dokument (`/pub/de/jahresabschluss?...`)** — meistens als
//!     PDF ausgeliefert. Der Orchestrator parst das PDF vor dem Aufruf
//!     von `extract_fields` mit `ctox_pdf_parse::parse_pdf_bytes` (siehe
//!     `tools/web-stack/src/web_search.rs::extract_pdf_sections_guided`)
//!     und liefert den Plaintext in `SourceReadResult.text` mit
//!     `is_pdf = true`. Wir wenden hier `regex`-basierte Extraktion auf
//!     diesen Plaintext an, um Umsatzerlöse und Mitarbeiter-Zahl zu
//!     ziehen.
//!
//! Bundesanzeiger setzt Anti-Bot-Throttling ein (HTTP 429); der Crawler
//! reicht das als `SourceError::RateLimited` an `person-research` zurück
//! — dieses Modul reicht selbst keinen Netzwerk-Code aus, weil
//! `fetch_direct` für den Crawl-Pfad bewusst `None` bleibt; die
//! Behandlung passiert in der Provider-Cascade des Webstacks.

use regex::Regex;
use scraper::{ElementRef, Html, Selector};
use std::sync::OnceLock;

use super::{
    Confidence, Country, FieldEvidence, FieldKey, ShapedQuery, SourceCtx, SourceModule,
    SourceReadResult, Tier,
};

const ID: &str = "bundesanzeiger.de";
const DOMAIN: &str = "bundesanzeiger.de";

struct Bundesanzeiger;

impl SourceModule for Bundesanzeiger {
    fn id(&self) -> &'static str {
        ID
    }

    fn aliases(&self) -> &'static [&'static str] {
        &["bundesanzeiger", "ba"]
    }

    fn tier(&self) -> Tier {
        Tier::P
    }

    fn countries(&self) -> &'static [Country] {
        &[Country::De]
    }

    fn authoritative_for(&self) -> &'static [FieldKey] {
        // Aus EXCEL_MATRIX.md: `bundesanzeiger.de` ist DE-only und in
        // `NewRecord` autoritativ für `firma_name`, `umsatz`, `mitarbeiter`.
        &[
            FieldKey::FirmaName,
            FieldKey::Umsatz,
            FieldKey::Mitarbeiter,
        ]
    }

    /// Crawl-Pfad: Query wird mit dem Schlüsselwort „Jahresabschluss"
    /// und einem Domain-Pin angereichert. Nicht-DE Kontexte werden vom
    /// Orchestrator übersprungen.
    fn shape_query(&self, query: &str, ctx: &SourceCtx<'_>) -> Option<ShapedQuery> {
        // Wenn kein Land bekannt ist, lassen wir die Quelle für DE-Mode
        // trotzdem zu; explizit ein anderes Land → out.
        if let Some(country) = ctx.country {
            if country != Country::De {
                return None;
            }
        }
        let trimmed = query.trim();
        if trimmed.is_empty() {
            return None;
        }
        Some(ShapedQuery {
            query: format!("{trimmed} Jahresabschluss site:{DOMAIN}"),
            domains: vec![DOMAIN.to_string()],
        })
    }

    fn extract_fields(&self, page: &SourceReadResult) -> Vec<(FieldKey, FieldEvidence)> {
        if page.is_pdf {
            extract_from_pdf_text(&page.text, &page.url)
        } else {
            extract_from_html(&page.text, &page.url)
        }
    }
}

static MODULE: Bundesanzeiger = Bundesanzeiger;

pub fn module() -> &'static dyn SourceModule {
    &MODULE
}

// ---------------------------------------------------------------------------
// HTML — search-results page
// ---------------------------------------------------------------------------

fn result_row_selector() -> &'static Selector {
    static SEL: OnceLock<Selector> = OnceLock::new();
    SEL.get_or_init(|| {
        Selector::parse("[data-id]").expect("valid bundesanzeiger row selector")
    })
}

fn result_title_selector() -> &'static Selector {
    static SEL: OnceLock<Selector> = OnceLock::new();
    // The Bundesanzeiger search markup uses both `.title` (newer layout)
    // and `.company` (legacy layout) for the company name.
    SEL.get_or_init(|| Selector::parse(".title, .company").expect("valid title selector"))
}

fn extract_from_html(html: &str, source_url: &str) -> Vec<(FieldKey, FieldEvidence)> {
    let document = Html::parse_document(html);
    let mut out: Vec<(FieldKey, FieldEvidence)> = Vec::new();

    // Each search hit lives in a container with `data-id="…"`. We take the
    // first hit's company name as `firma_name`. If the operator wants more
    // than one hit they get them via Phase 4 person-research, not from a
    // single source-extract call.
    for row in document.select(result_row_selector()) {
        if !is_search_hit(&row) {
            continue;
        }
        if let Some(name) = first_title_text(&row) {
            push(
                &mut out,
                FieldKey::FirmaName,
                name,
                Confidence::High,
                source_url,
                Some("bundesanzeiger search hit (data-id)"),
            );
            break;
        }
    }

    out
}

/// A search-hit `[data-id]` element is the row that *contains* the hit
/// metadata, not the inline `<a data-id>` link inside it. We treat any
/// element with `data-id` and at least one descendant matching the title
/// selector as a hit row; that drops noise like submit-buttons or
/// pagination markers that also occasionally carry `data-id`.
fn is_search_hit(row: &ElementRef<'_>) -> bool {
    row.select(result_title_selector()).next().is_some()
}

fn first_title_text(row: &ElementRef<'_>) -> Option<String> {
    let title = row.select(result_title_selector()).next()?;
    let text = node_text(&title);
    if text.is_empty() {
        None
    } else {
        Some(text)
    }
}

// ---------------------------------------------------------------------------
// PDF plaintext — Jahresabschluss
// ---------------------------------------------------------------------------

/// Matches an `Umsatzerlöse` row in a GuV table. Captures the numeric
/// amount as it appears in the PDF (German thousand separators / decimal
/// commas / parenthesised negatives). The PDF parser flattens the table
/// to whitespace-separated tokens, so we anchor on the label and grab the
/// first numeric token after it.
fn umsatz_regex() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(
            r"(?im)Umsatzerl(?:ö|oe)se[^\d\-]*([\-\(]?\s*[\d][\d\.\s]*(?:,\d+)?\)?)",
        )
        .expect("valid umsatz regex")
    })
}

/// Matches the standard German Anhangs-Angabe „Arbeitnehmer im
/// Jahresdurchschnitt" or its variant „Mitarbeiter im Jahresdurchschnitt".
fn mitarbeiter_regex() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(
            r"(?im)(?:Arbeitnehmer|Mitarbeiter)\s+im\s+Jahresdurchschnitt[^\d]*(\d[\d\.\s]*)",
        )
        .expect("valid mitarbeiter regex")
    })
}

/// First non-empty line of the PDF dump — Bundesanzeiger renders the
/// company name as the title block at the top of every Jahresabschluss.
fn first_nonempty_line(text: &str) -> Option<&str> {
    for line in text.lines() {
        let trimmed = line.trim();
        if !trimmed.is_empty() {
            return Some(trimmed);
        }
    }
    None
}

fn extract_from_pdf_text(text: &str, source_url: &str) -> Vec<(FieldKey, FieldEvidence)> {
    let mut out: Vec<(FieldKey, FieldEvidence)> = Vec::new();

    // ---- firma_name: title line of the Jahresabschluss PDF
    if let Some(name_line) = first_nonempty_line(text) {
        // Skip obvious non-name first lines (footers / metadata / page
        // headers). Real Bundesanzeiger PDFs begin with the company name.
        if !name_line.starts_with("Bundesanzeiger") && !name_line.starts_with("Seite ") {
            push(
                &mut out,
                FieldKey::FirmaName,
                name_line.to_string(),
                Confidence::High,
                source_url,
                Some("PDF-Titelzeile Jahresabschluss"),
            );
        }
    }

    // ---- umsatz: from GuV row
    if let Some(captures) = umsatz_regex().captures(text) {
        if let Some(raw) = captures.get(1) {
            let normalised = normalise_amount(raw.as_str());
            if !normalised.is_empty() {
                push(
                    &mut out,
                    FieldKey::Umsatz,
                    normalised,
                    Confidence::High,
                    source_url,
                    Some("GuV-Zeile Umsatzerlöse"),
                );
            }
        }
    }

    // ---- mitarbeiter: from Anhangs-Angabe
    if let Some(captures) = mitarbeiter_regex().captures(text) {
        if let Some(raw) = captures.get(1) {
            let normalised = normalise_count(raw.as_str());
            if !normalised.is_empty() {
                push(
                    &mut out,
                    FieldKey::Mitarbeiter,
                    normalised,
                    Confidence::High,
                    source_url,
                    Some("Anhang Arbeitnehmer im Jahresdurchschnitt"),
                );
            }
        }
    }

    out
}

/// Normalise an Umsatzerlöse amount captured from a German GuV table.
/// Strips whitespace and parenthesised-negative wrapping; keeps thousand
/// separators (`.`) and decimal separator (`,`) intact because that is
/// how the Excel-Pipeline of person-research wants them.
fn normalise_amount(raw: &str) -> String {
    let mut s = raw.trim().to_string();
    // Parenthesised negative: "(123)" → "-123"
    if s.starts_with('(') && s.ends_with(')') {
        s = format!("-{}", &s[1..s.len() - 1]);
    }
    // Collapse internal whitespace ("485 .732 .145,00" → "485.732.145,00").
    let collapsed: String = s.chars().filter(|c| !c.is_whitespace()).collect();
    collapsed
}

/// Normalise an Arbeitnehmer count: drop thousand separators and
/// whitespace so we end up with a plain integer string ("1.842" → "1842").
fn normalise_count(raw: &str) -> String {
    raw.chars()
        .filter(|c| c.is_ascii_digit())
        .collect::<String>()
}

// ---------------------------------------------------------------------------
// Shared helpers
// ---------------------------------------------------------------------------

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
            mode: super::super::ResearchMode::NewRecord,
        }
    }

    fn ctx_at() -> SourceCtx<'static> {
        static ROOT: std::sync::OnceLock<PathBuf> = std::sync::OnceLock::new();
        let root = ROOT.get_or_init(|| PathBuf::from(""));
        SourceCtx {
            root,
            country: Some(Country::At),
            mode: super::super::ResearchMode::NewRecord,
        }
    }

    fn ctx_unknown_country() -> SourceCtx<'static> {
        static ROOT: std::sync::OnceLock<PathBuf> = std::sync::OnceLock::new();
        let root = ROOT.get_or_init(|| PathBuf::from(""));
        SourceCtx {
            root,
            country: None,
            mode: super::super::ResearchMode::NewRecord,
        }
    }

    #[test]
    fn registry_metadata() {
        let m = module();
        assert_eq!(m.id(), "bundesanzeiger.de");
        assert!(m.aliases().contains(&"bundesanzeiger"));
        assert!(m.aliases().contains(&"ba"));
        assert_eq!(m.tier(), Tier::P);
        assert_eq!(m.countries(), &[Country::De]);
        assert!(m.requires_credential().is_none());
        let auth = m.authoritative_for();
        assert!(auth.contains(&FieldKey::FirmaName));
        assert!(auth.contains(&FieldKey::Umsatz));
        assert!(auth.contains(&FieldKey::Mitarbeiter));
    }

    #[test]
    fn shape_query_de_pins_domain_and_keyword() {
        let shaped = module()
            .shape_query("WITTENSTEIN SE", &ctx_de())
            .expect("DE must shape");
        assert!(shaped.query.contains("WITTENSTEIN SE"));
        assert!(shaped.query.contains("Jahresabschluss"));
        assert!(shaped.query.contains("bundesanzeiger.de"));
        assert_eq!(shaped.domains, vec!["bundesanzeiger.de".to_string()]);
    }

    #[test]
    fn shape_query_unknown_country_still_allowed() {
        // No country pin → still in. The orchestrator may not have a
        // country at hand for the initial query phase.
        let shaped = module()
            .shape_query("WITTENSTEIN SE", &ctx_unknown_country())
            .expect("unknown country must shape");
        assert!(shaped.query.contains("Jahresabschluss"));
    }

    #[test]
    fn shape_query_non_de_returns_none() {
        assert!(module()
            .shape_query("Red Bull GmbH", &ctx_at())
            .is_none());
    }

    #[test]
    fn shape_query_empty_returns_none() {
        assert!(module().shape_query("   ", &ctx_de()).is_none());
    }

    #[test]
    fn no_fetch_direct_override() {
        // Crawl-Pfad — kein direkter API-Call.
        assert!(module()
            .fetch_direct(&ctx_de(), "WITTENSTEIN SE")
            .is_none());
    }

    /// Extracts `firma_name` from a frozen Bundesanzeiger search-results
    /// page.
    ///
    /// Fixture-Ursprung: Layout 1:1 nach dem öffentlichen Markup der
    /// Bundesanzeiger-Volltextsuche
    /// (`https://www.bundesanzeiger.de/pub/de/suche?13` mit `name=Wittenstein`),
    /// auf eine Treffer-Zeile reduziert; Treffer-Zeile mit `data-id` und
    /// dem `.title`-Block ist die offizielle DOM-Struktur.
    #[test]
    fn extract_search_results_html_fixture() {
        let html = include_str!(
            "../../fixtures/sources/bundesanzeiger/search_wittenstein.html"
        );
        let page = SourceReadResult {
            url: "https://www.bundesanzeiger.de/pub/de/suche?name=Wittenstein".to_string(),
            title: "Suche - Bundesanzeiger".to_string(),
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
        assert_eq!(name.value, "WITTENSTEIN SE");
        assert_eq!(name.confidence, Confidence::High);

        // Search-results page has no GuV/Anhang data → keine Finanzfelder.
        assert!(by_key.get(&FieldKey::Umsatz).is_none());
        assert!(by_key.get(&FieldKey::Mitarbeiter).is_none());
    }

    /// Extracts `umsatz` and `mitarbeiter` from a Jahresabschluss
    /// plaintext dump.
    ///
    /// Fixture-Ursprung: handgebaut aus der offiziellen
    /// Bundesanzeiger-Veröffentlichungsstruktur eines HGB-§325-Pflicht-
    /// Jahresabschlusses (Titelzeile + GuV-Block + Anhang-Angabe
    /// „Arbeitnehmer im Jahresdurchschnitt"). Zahlen synthetisch, Layout
    /// repräsentativ — der Orchestrator-Pfad liefert genau diesen
    /// Plaintext nach Lauf durch `ctox_pdf_parse::parse_pdf_bytes` mit
    /// `OutputFormat::Text`.
    #[test]
    fn extract_jahresabschluss_pdf_fixture() {
        let text = include_str!(
            "../../fixtures/sources/bundesanzeiger/detail_wittenstein.txt"
        );
        let page = SourceReadResult {
            url: "https://www.bundesanzeiger.de/pub/de/jahresabschluss?id=JA_4711_2023".to_string(),
            title: String::new(),
            summary: String::new(),
            text: text.to_string(),
            is_pdf: true,
            excerpts: Vec::new(),
            find_results: Vec::new(),
        };
        let fields = module().extract_fields(&page);
        let by_key: std::collections::HashMap<FieldKey, &FieldEvidence> =
            fields.iter().map(|(k, v)| (*k, v)).collect();

        let name = by_key
            .get(&FieldKey::FirmaName)
            .expect("firma_name extracted");
        assert_eq!(name.value, "WITTENSTEIN SE");
        assert_eq!(name.confidence, Confidence::High);

        let umsatz = by_key
            .get(&FieldKey::Umsatz)
            .expect("umsatz extracted");
        assert_eq!(umsatz.value, "485.732.145,00");
        assert_eq!(umsatz.confidence, Confidence::High);

        let mitarbeiter = by_key
            .get(&FieldKey::Mitarbeiter)
            .expect("mitarbeiter extracted");
        assert_eq!(mitarbeiter.value, "1842");
        assert_eq!(mitarbeiter.confidence, Confidence::High);
    }

    #[test]
    fn extract_pdf_handles_mitarbeiter_alias() {
        // Some publishers print "Mitarbeiter im Jahresdurchschnitt" instead
        // of "Arbeitnehmer im Jahresdurchschnitt"; the regex must accept both.
        let text = "BEISPIEL GMBH\n\nMitarbeiter im Jahresdurchschnitt   42\n";
        let page = SourceReadResult {
            url: "https://www.bundesanzeiger.de/pub/de/jahresabschluss?id=X".to_string(),
            title: String::new(),
            summary: String::new(),
            text: text.to_string(),
            is_pdf: true,
            excerpts: Vec::new(),
            find_results: Vec::new(),
        };
        let fields = module().extract_fields(&page);
        let mitarbeiter = fields
            .iter()
            .find(|(k, _)| *k == FieldKey::Mitarbeiter)
            .map(|(_, v)| v)
            .expect("mitarbeiter via Mitarbeiter-Alias");
        assert_eq!(mitarbeiter.value, "42");
    }

    #[test]
    fn extract_pdf_with_thousands_separator_in_count() {
        let text = "FOO AG\n\nArbeitnehmer im Jahresdurchschnitt   1.842\n";
        let page = SourceReadResult {
            url: "https://www.bundesanzeiger.de/pub/de/jahresabschluss?id=Y".to_string(),
            title: String::new(),
            summary: String::new(),
            text: text.to_string(),
            is_pdf: true,
            excerpts: Vec::new(),
            find_results: Vec::new(),
        };
        let fields = module().extract_fields(&page);
        let mitarbeiter = fields
            .iter()
            .find(|(k, _)| *k == FieldKey::Mitarbeiter)
            .map(|(_, v)| v)
            .expect("mitarbeiter with thousands sep");
        assert_eq!(mitarbeiter.value, "1842");
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
        };
        let fields = module().extract_fields(&page);
        // No data-id rows → keine Felder.
        assert!(fields.is_empty(), "unexpected fields: {fields:?}");
    }

    #[test]
    fn normalise_amount_parens_to_minus() {
        assert_eq!(normalise_amount(" (123.456,00) "), "-123.456,00");
    }

    #[test]
    fn normalise_amount_strips_inner_whitespace() {
        assert_eq!(normalise_amount("485 .732 .145,00"), "485.732.145,00");
    }

    /// Live-Smoke gegen die echte Bundesanzeiger-Suche.
    ///
    /// Standardmäßig ignoriert; explizit ausführen mit:
    ///   `cargo test -p ctox-web-stack -- --ignored sources::bundesanzeiger::live`
    ///
    /// Bundesanzeiger drosselt aggressiv (HTTP 429), deshalb soft-skipped
    /// dieser Test bei Netzwerk- oder Status-Fehlern statt zu failen.
    #[test]
    #[ignore = "live network; run with: cargo test -- --ignored sources::bundesanzeiger::live_wittenstein"]
    fn live_wittenstein() {
        let url = "https://www.bundesanzeiger.de/pub/de/suche?13&name=Wittenstein";
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
            Err(ureq::Error::Status(429, _)) => {
                eprintln!("bundesanzeiger.de rate-limited (429) — soft skip");
                return;
            }
            Err(err) => {
                eprintln!("bundesanzeiger.de live request failed (skip): {err}");
                return;
            }
        };
        // Live HTML lives behind a session-cookie acknowledgement; we
        // accept either: (a) the real results page with our hit, or
        // (b) the cookie/landing wall — both confirm we reached the host.
        let page = SourceReadResult {
            url: url.to_string(),
            title: String::new(),
            summary: String::new(),
            text: body.clone(),
            is_pdf: false,
            excerpts: Vec::new(),
            find_results: Vec::new(),
        };
        let _fields = module().extract_fields(&page);
        assert!(
            body.contains("Bundesanzeiger") || body.contains("bundesanzeiger"),
            "live response did not look like Bundesanzeiger HTML"
        );
    }
}
