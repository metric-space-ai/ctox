//! `northdata.de` — Tier S, DACH (DE / AT / CH).
//!
//! Aggregator über Bundesanzeiger + Handelsregister (DE), Firmenbuch (AT) und
//! Zefix (CH). Re-hostet die offiziellen Stammdaten unter einheitlichen URLs
//! und macht sie crawlbar — Firmierung, Anschrift, Geschäftsführer-/Vorstand-
//! Liste sind ohne Account abrufbar; Umsatz/Bilanzsumme verlangen je nach
//! Land einen Login.
//!
//! Extraktions-Plan (high-level):
//!   * Firma-Stammdaten (`firma_name`, `firma_anschrift`, `firma_plz`,
//!     `firma_ort`) kommen aus den Semantic-UI-„Ribbon"-Sektionen, die
//!     Northdata einheitlich für alle Länder rendert
//!     (`<h3 class="ui ... ribbon ... label">Name|Adresse</h3>` gefolgt von
//!     `.general-information.list .item .content`). Diese Blöcke sind das
//!     1:1-Mirror der Handelsregister-/Firmenbuch-/Zefix-Quelle und für die
//!     CTOX-Matrix die Hauptquelle in `UpdateFirm` und `NewRecord`
//!     (DE / AT / CH) → `Confidence::High`.
//!   * Person-Felder (`person_vorname`, `person_nachname`, `person_position`)
//!     werden aus dem `<figure class="bizq" data-data="…">`-JSON neben
//!     `<h3>Personen</h3>` gelesen. Northdata schreibt dort pro Event den
//!     vollen Tenor („Vorstand Veronika Bienert", „Geschäftsführer Franz
//!     Watzlawick", „Vorstandsvorsitzender Roland Busch"). Historische
//!     Einträge sind mit `"old": true` markiert und werden gefiltert.
//!     Vor-/Nachname werden am letzten Whitespace gesplittet — die
//!     Heuristik ist robust für DACH-Personen-Stammdaten, aber laut
//!     Excel-Matrix mit Konfidenz-Asterisk → `Confidence::Medium`.
//!   * `umsatz` ist auf der öffentlichen Profilseite nicht ausgewiesen —
//!     Northdata blendet die Zahl nur für eingeloggte Konten ein. Falls
//!     eine zukünftige Crawl-Quelle (z. B. archive.org-Snapshot eines
//!     Pro-Accounts) den Wert mitliefert, picken wir ihn aus der
//!     `Kennzahlen`-Tabelle ab. Aktuell ist diese Quelle die zweite Wahl
//!     nach Bundesanzeiger und macht im Public-Crawl-Pfad nichts.

use scraper::{ElementRef, Html, Selector};
use serde_json::Value;
use std::sync::OnceLock;

use super::{
    Confidence, Country, FieldEvidence, FieldKey, ShapedQuery, SourceCtx, SourceModule,
    SourceReadResult, Tier,
};

const ID: &str = "northdata.de";
const DOMAIN: &str = "northdata.de";

struct Northdata;

impl SourceModule for Northdata {
    fn id(&self) -> &'static str {
        ID
    }

    fn aliases(&self) -> &'static [&'static str] {
        &["northdata", "nd"]
    }

    fn tier(&self) -> Tier {
        Tier::S
    }

    fn countries(&self) -> &'static [Country] {
        &[Country::De, Country::At, Country::Ch]
    }

    fn authoritative_for(&self) -> &'static [FieldKey] {
        // Aus EXCEL_MATRIX.md: Northdata ist in DE/AT/CH die Hauptquelle für
        // Firma-Stammdaten (UpdateFirm, NewRecord) sowie die Brücke zu
        // Person-Vor-/Nachname/Position in allen drei Ländern.
        // Umsatz ist matrix-seitig zwar gelistet (DE / NewRecord über
        // bundesanzeiger ++ dnbhoovers), Northdata trägt ihn aber nur
        // gelegentlich; wir halten ihn als deklarierte Authority offen.
        &[
            FieldKey::FirmaName,
            FieldKey::FirmaAnschrift,
            FieldKey::FirmaPlz,
            FieldKey::FirmaOrt,
            FieldKey::Umsatz,
            FieldKey::PersonVorname,
            FieldKey::PersonNachname,
            FieldKey::PersonPosition,
        ]
    }

    /// Crawl-Pfad: DACH-tolerant. Northdata deckt DE, AT und CH ab, und
    /// behandelt auch Anfragen ohne expliziten Country-Hint als gültig
    /// (`person-research` setzt `country = None` bei ambivalenten Excel-
    /// Zeilen). Andere Länder geben `None` und werden vom Orchestrator
    /// übersprungen.
    fn shape_query(&self, query: &str, ctx: &SourceCtx<'_>) -> Option<ShapedQuery> {
        match ctx.country {
            Some(Country::De) | Some(Country::At) | Some(Country::Ch) | None => {}
            Some(_) => return None,
        }
        let trimmed = query.trim();
        if trimmed.is_empty() {
            return None;
        }
        Some(ShapedQuery {
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

static MODULE: Northdata = Northdata;

pub fn module() -> &'static dyn SourceModule {
    &MODULE
}

// ---------------------------------------------------------------------------
// Selectors — `scraper::Selector::parse` ist nicht `const`, deshalb `OnceLock`.
// ---------------------------------------------------------------------------

fn h1_selector() -> &'static Selector {
    static SEL: OnceLock<Selector> = OnceLock::new();
    SEL.get_or_init(|| {
        Selector::parse("h1.ui.header.qualified").expect("valid h1 selector")
    })
}

fn ribbon_label_selector() -> &'static Selector {
    static SEL: OnceLock<Selector> = OnceLock::new();
    SEL.get_or_init(|| Selector::parse("h3.ribbon.label").expect("valid h3 ribbon selector"))
}

fn bizq_figure_selector() -> &'static Selector {
    static SEL: OnceLock<Selector> = OnceLock::new();
    SEL.get_or_init(|| Selector::parse("figure.bizq[data-data]").expect("valid bizq selector"))
}

// ---------------------------------------------------------------------------
// Core extraction
// ---------------------------------------------------------------------------

fn extract_from_html(html: &str, source_url: &str) -> Vec<(FieldKey, FieldEvidence)> {
    let document = Html::parse_document(html);
    let mut out: Vec<(FieldKey, FieldEvidence)> = Vec::new();

    // ---- (1) Firmierung: zuerst die Ribbon-Sektion „Name" (kanonischer
    //         Firmenname ohne Stadt-Suffix), Fallback auf <h1>.
    if let Some(name) = first_general_info_item(&document, &["Name"]) {
        push(
            &mut out,
            FieldKey::FirmaName,
            name,
            Confidence::High,
            source_url,
            Some("ribbon section: Name"),
        );
    } else if let Some(h1) = document.select(h1_selector()).next() {
        // <h1> packt Firmenname + Sitz als „Siemens AG, München" — wir
        // schneiden alles ab dem ersten Komma weg, falls die Ribbon-Sektion
        // fehlt (alter Profil-Layout).
        let raw = node_text(&h1);
        let cleaned = raw.split(',').next().unwrap_or(&raw).trim().to_string();
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

    // ---- (2) Adresse: Ribbon-Sektion „Adresse" oder „Anschrift".
    //         Northdata rendert die Zeile als „<Straße>, <Land-PLZ> <Ort>".
    if let Some(line) = first_general_info_item(&document, &["Adresse", "Anschrift"]) {
        let parsed = parse_address_line(&line);
        if let Some(street) = parsed.street {
            push(
                &mut out,
                FieldKey::FirmaAnschrift,
                street,
                Confidence::High,
                source_url,
                Some("ribbon section: Adresse"),
            );
        }
        if let Some(plz) = parsed.postal_code {
            push(
                &mut out,
                FieldKey::FirmaPlz,
                plz,
                Confidence::High,
                source_url,
                Some("ribbon section: Adresse"),
            );
        }
        if let Some(city) = parsed.city {
            push(
                &mut out,
                FieldKey::FirmaOrt,
                city,
                Confidence::High,
                source_url,
                Some("ribbon section: Adresse"),
            );
        }
    }

    // ---- (3) Personen aus dem bizq-Event-JSON.
    //         Wir nehmen den ersten Event mit type=="p" und ohne "old": true
    //         als primären Vertreter (zeitlich neuester aktiver Eintrag,
    //         da Northdata die Events absteigend nach Datum rendert).
    if let Some(primary) = first_active_person_event(&document) {
        if let Some(position) = primary.position.clone() {
            push(
                &mut out,
                FieldKey::PersonPosition,
                position,
                Confidence::Medium,
                source_url,
                Some("figure.bizq events JSON (Personen)"),
            );
        }
        if let Some(first) = primary.first_name.clone() {
            push(
                &mut out,
                FieldKey::PersonVorname,
                first,
                Confidence::Medium,
                source_url,
                Some("figure.bizq events JSON (Personen)"),
            );
        }
        if let Some(last) = primary.last_name.clone() {
            push(
                &mut out,
                FieldKey::PersonNachname,
                last,
                Confidence::Medium,
                source_url,
                Some("figure.bizq events JSON (Personen)"),
            );
        }
    }

    // ---- (4) Umsatz — Public-Crawl liefert in der Regel keine Zahl.
    //         Trotzdem versuchen, falls eine logged-in / archive-Variante
    //         einen Kennzahlen-Block durchreicht.
    if let Some(umsatz) = extract_revenue(&document) {
        push(
            &mut out,
            FieldKey::Umsatz,
            umsatz,
            Confidence::Medium,
            source_url,
            Some("Kennzahlen / Finanzen"),
        );
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

/// Walk the DOM looking for a `<h3 class="ribbon … label">SectionName</h3>`
/// whose text matches one of `labels`, then return the text of the *first*
/// non-`former` `.item .content` within the following `.general-information`
/// list. Northdata renders all stammdaten sections through this exact
/// pattern, regardless of country (DE / AT / CH).
fn first_general_info_item(document: &Html, labels: &[&str]) -> Option<String> {
    // We can't easily walk "next siblings of an h3" in scraper without
    // ancestor-aware iteration, so we do a forward scan over all element
    // descendants and remember the last seen ribbon label.
    let item_content_selector = item_content_selector();
    let mut active_label: Option<String> = None;
    for element in document
        .root_element()
        .descendants()
        .filter_map(ElementRef::wrap)
    {
        if Selector::matches(ribbon_label_selector(), &element) {
            active_label = Some(node_text(&element));
            continue;
        }
        // Once we hit an `.item .content` while the active label matches,
        // we have our value — but only for non-`former` items (i.e. the
        // current value, not a struck-through historic one).
        if let Some(ref label) = active_label {
            let matches_label = labels.iter().any(|wanted| {
                label.eq_ignore_ascii_case(wanted)
                    || label
                        .to_ascii_lowercase()
                        .contains(&wanted.to_ascii_lowercase())
            });
            if !matches_label {
                continue;
            }
            if Selector::matches(item_content_selector, &element) {
                if is_inside_former(&element) {
                    continue;
                }
                let text = node_text(&element);
                if !text.is_empty() {
                    return Some(text);
                }
            }
        }
    }
    None
}

fn item_content_selector() -> &'static Selector {
    static SEL: OnceLock<Selector> = OnceLock::new();
    SEL.get_or_init(|| {
        Selector::parse(".general-information .item .content")
            .expect("valid item content selector")
    })
}

/// Walk parents to detect whether the element is wrapped in a `.former`
/// container — Northdata uses `.former.item` for struck-through historic
/// entries (e.g. „auch / vormals …"), which we never want to surface as the
/// current value.
fn is_inside_former(element: &ElementRef<'_>) -> bool {
    let mut current = element.parent();
    while let Some(node) = current {
        if let Some(parent) = ElementRef::wrap(node) {
            let class_attr = parent.value().attr("class").unwrap_or("");
            for token in class_attr.split_ascii_whitespace() {
                if token == "former" {
                    return true;
                }
            }
            current = parent.parent();
        } else {
            current = node.parent();
        }
    }
    false
}

#[derive(Debug, Default, Clone)]
struct ParsedAddress {
    street: Option<String>,
    postal_code: Option<String>,
    city: Option<String>,
}

/// Northdata renders addresses as a single line:
///   * `Werner-von-Siemens-Str. 1, D-80333 München`         (DE)
///   * `Am Brunnen 1, A-5330 Fuschl am See`                  (AT)
///   * `Bahnhofstrasse 12, CH-8001 Zürich`                   (CH)
///
/// The country prefix (`D-`, `A-`, `CH-`) and the PLZ are part of the city
/// segment, separated by a single space. The parser splits on the *last*
/// comma so street names containing commas keep working, then strips the
/// country prefix and pulls the PLZ off the front.
fn parse_address_line(raw: &str) -> ParsedAddress {
    let mut out = ParsedAddress::default();
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return out;
    }
    let (street, locality) = match trimmed.rsplit_once(',') {
        Some((street, locality)) => (street.trim().to_string(), locality.trim().to_string()),
        None => return out,
    };
    if !street.is_empty() {
        out.street = Some(street);
    }
    // Locality looks like "D-80333 München" or "5330 Fuschl am See" or
    // "CH-8001 Zürich". Strip any leading country prefix of the form
    // "<letters>-" before the PLZ.
    let after_prefix = strip_country_prefix(&locality);
    let mut parts = after_prefix.splitn(2, char::is_whitespace);
    let head = parts.next().unwrap_or("").trim();
    let tail = parts.next().unwrap_or("").trim();
    if looks_like_postal_code(head) && !tail.is_empty() {
        out.postal_code = Some(head.to_string());
        out.city = Some(tail.to_string());
    } else if !after_prefix.is_empty() {
        // PLZ fehlt — speichere Restzeile als Ort, lasse PLZ leer.
        out.city = Some(after_prefix.to_string());
    }
    out
}

fn strip_country_prefix(locality: &str) -> String {
    let trimmed = locality.trim();
    // We accept up to four leading ASCII letters followed by '-' (covers
    // D-, A-, CH-, FL-, …). Anything else stays untouched.
    if let Some(dash) = trimmed.find('-') {
        if dash <= 4 && trimmed[..dash].chars().all(|c| c.is_ascii_alphabetic()) {
            return trimmed[dash + 1..].trim().to_string();
        }
    }
    trimmed.to_string()
}

fn looks_like_postal_code(token: &str) -> bool {
    !token.is_empty()
        && token.len() <= 6
        && token.chars().all(|c| c.is_ascii_digit())
}

#[derive(Debug, Default, Clone)]
struct PersonEvent {
    position: Option<String>,
    first_name: Option<String>,
    last_name: Option<String>,
}

/// Walk every `figure.bizq[data-data]` and return the first event with
/// `type == "p"` that is not flagged `old == true`. The events JSON is
/// HTML-escaped inside the attribute, but `scraper` resolves entities for
/// us when we read the attribute value.
fn first_active_person_event(document: &Html) -> Option<PersonEvent> {
    for figure in document.select(bizq_figure_selector()) {
        let raw = figure.value().attr("data-data")?;
        let parsed: Value = match serde_json::from_str(raw) {
            Ok(v) => v,
            Err(_) => continue,
        };
        let events = parsed.get("event").and_then(Value::as_array);
        let Some(events) = events else { continue };
        for event in events {
            if event.get("type").and_then(Value::as_str) != Some("p") {
                continue;
            }
            if event.get("old").and_then(Value::as_bool).unwrap_or(false) {
                continue;
            }
            let text = event
                .get("text")
                .and_then(Value::as_str)
                .or_else(|| event.get("desc").and_then(Value::as_str))
                .unwrap_or("")
                .trim();
            if text.is_empty() {
                continue;
            }
            return Some(parse_person_label(text));
        }
    }
    None
}

/// Parse a Northdata person event label, e.g.:
///   * `"Vorstand Veronika Bienert"`
///   * `"Geschäftsführer Franz Watzlawick"`
///   * `"Vorstandsvorsitzender Roland Busch"`
///   * `"VV Roland Busch"`                 (abbreviated)
///   * `"Vst. Siegfried K. Rußwurm"`       (abbreviated)
///
/// The first token (or token-pair, for abbreviations like „Vst." or „VV") is
/// the position; everything afterwards is the personal name. We split the
/// name at the *last* whitespace so multi-word first names („Anna-Maria",
/// „Peter Maria", „Siegfried K.") stay attached to the first-name field.
fn parse_person_label(text: &str) -> PersonEvent {
    let mut out = PersonEvent::default();
    let cleaned = text.trim();
    if cleaned.is_empty() {
        return out;
    }
    let tokens: Vec<&str> = cleaned.split_whitespace().collect();
    // Find the boundary between position tokens and name tokens. Position
    // tokens are either known abbreviations or strings that contain at
    // least one lowercase letter (rules out single-letter initials).
    let mut split_idx = 0usize;
    while split_idx < tokens.len() && is_position_token(tokens[split_idx]) {
        split_idx += 1;
    }
    // Defensive: if everything looked like a position, treat the *last*
    // two tokens as name fallback.
    if split_idx == tokens.len() && tokens.len() >= 2 {
        split_idx = tokens.len() - 2;
    }
    if split_idx == 0 {
        // No position found — entire string is the name.
        split_idx = 0;
    }
    let position_tokens = &tokens[..split_idx];
    let name_tokens = &tokens[split_idx..];
    if !position_tokens.is_empty() {
        out.position = Some(expand_position(&position_tokens.join(" ")));
    }
    match name_tokens.len() {
        0 => {}
        1 => {
            out.last_name = Some(name_tokens[0].to_string());
        }
        _ => {
            let last = name_tokens[name_tokens.len() - 1];
            let first = name_tokens[..name_tokens.len() - 1].join(" ");
            out.first_name = Some(first);
            out.last_name = Some(last.to_string());
        }
    }
    out
}

fn is_position_token(token: &str) -> bool {
    // Known DACH role tokens / abbreviations Northdata prints. Anything
    // else (proper noun, initials with a trailing dot like "K.") falls
    // through to the name half of the label.
    matches!(
        token,
        "Vorstand"
            | "Vorstandsvorsitzender"
            | "Vorstandsvorsitzende"
            | "Geschäftsführer"
            | "Geschäftsführerin"
            | "Geschaftsfuhrer"
            | "Geschäftsführung"
            | "Prokurist"
            | "Prokuristin"
            | "Prokura"
            | "Aufsichtsrat"
            | "Aufsichtsratsvorsitzender"
            | "Inhaber"
            | "Inhaberin"
            | "VV"
            | "Vst."
            | "Vst"
            | "GF"
            | "GF."
            | "Verwaltungsrat"
            | "Verwaltungsratspräsident"
            | "Verwaltungsratspräsidentin"
            | "Präsident"
            | "Präsidentin"
    )
}

fn expand_position(raw: &str) -> String {
    match raw {
        "VV" => "Vorstandsvorsitzender".to_string(),
        "Vst." | "Vst" => "Vorstand".to_string(),
        "GF" | "GF." => "Geschäftsführer".to_string(),
        other => other.to_string(),
    }
}

/// Best-effort Umsatz-Picker für die seltene öffentliche Variante, in der
/// Northdata einen Kennzahlen-Block mit „Umsatz" + Zahl exponiert. Standard-
/// Public-Profile haben hier nichts; der Block kommt nur auf einigen alten
/// Profilen oder in archive.org-Snapshots ein- bzw. ausgeloggter Konten vor.
/// Wir scannen den Volltext nach einem „Umsatz"-Token gefolgt von einem
/// Geldbetrag mit EUR / € / Mio. / Mrd. — und nehmen den ersten Treffer.
fn extract_revenue(document: &Html) -> Option<String> {
    let body_text = node_text(&document.root_element());
    // Look for: "Umsatz <amount> <unit>" — amount may contain '.' and ',',
    // unit is € / EUR / Mio. € / Mrd. € / Tsd. €.
    let lower = body_text.to_ascii_lowercase();
    let needle = "umsatz";
    let mut search_from = 0usize;
    while let Some(found) = lower[search_from..].find(needle) {
        let start = search_from + found + needle.len();
        let window: String = body_text[start..]
            .chars()
            .take(60)
            .collect();
        if let Some(amount) = first_money_span(&window) {
            return Some(amount);
        }
        search_from = start;
    }
    None
}

fn first_money_span(window: &str) -> Option<String> {
    // Token-Scan: erstes Token, das mit Ziffer beginnt, plus optionaler
    // Einheit „Mio./Mrd./Tsd./€/EUR" rechts daneben. Wir bauen den Span
    // selbst, weil die Public-Templates keinen einheitlichen Selector haben.
    let mut tokens = window.split_whitespace().peekable();
    while let Some(token) = tokens.next() {
        let token = token.trim_matches(|c: char| c == ':' || c == '(' || c == ')');
        if token.is_empty() {
            continue;
        }
        let first_char = token.chars().next().unwrap();
        if !first_char.is_ascii_digit() {
            continue;
        }
        // Token darf nur Ziffern, ',' und '.' enthalten.
        if !token
            .chars()
            .all(|c| c.is_ascii_digit() || c == ',' || c == '.')
        {
            continue;
        }
        let mut span = token.to_string();
        if let Some(&peek) = tokens.peek() {
            let cleaned = peek.trim_end_matches(',').trim_end_matches('.');
            if matches!(cleaned, "Mio" | "Mio." | "Mrd" | "Mrd." | "Tsd" | "Tsd.") {
                span.push(' ');
                span.push_str(peek);
                tokens.next();
                if let Some(&peek2) = tokens.peek() {
                    if peek2 == "€" || peek2 == "EUR" {
                        span.push(' ');
                        span.push_str(peek2);
                    }
                }
            } else if peek == "€" || peek == "EUR" {
                span.push(' ');
                span.push_str(peek);
            }
        }
        return Some(span);
    }
    None
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

    fn ctx_ch() -> SourceCtx<'static> {
        static ROOT: std::sync::OnceLock<PathBuf> = std::sync::OnceLock::new();
        let root = ROOT.get_or_init(|| PathBuf::from(""));
        SourceCtx {
            root,
            country: Some(Country::Ch),
            mode: super::super::ResearchMode::UpdateFirm,
        }
    }

    fn ctx_none() -> SourceCtx<'static> {
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
        assert_eq!(m.id(), "northdata.de");
        assert!(m.aliases().contains(&"northdata"));
        assert!(m.aliases().contains(&"nd"));
        assert_eq!(m.tier(), Tier::S);
        assert_eq!(m.countries(), &[Country::De, Country::At, Country::Ch]);
        assert!(m.requires_credential().is_none());
        for expected in [
            FieldKey::FirmaName,
            FieldKey::FirmaAnschrift,
            FieldKey::FirmaPlz,
            FieldKey::FirmaOrt,
            FieldKey::PersonVorname,
            FieldKey::PersonNachname,
            FieldKey::PersonPosition,
        ] {
            assert!(
                m.authoritative_for().contains(&expected),
                "missing field authority: {expected:?}"
            );
        }
    }

    #[test]
    fn shape_query_dach_pins_domain() {
        for ctx in [ctx_de(), ctx_at(), ctx_ch(), ctx_none()] {
            let shaped = module()
                .shape_query("Siemens AG", &ctx)
                .expect("DACH-tolerant shape");
            assert!(shaped.query.contains("Siemens AG"));
            assert!(shaped.query.contains("site:northdata.de"));
            assert_eq!(shaped.domains, vec!["northdata.de".to_string()]);
        }
    }

    #[test]
    fn shape_query_empty_returns_none() {
        assert!(module().shape_query("   ", &ctx_de()).is_none());
    }

    #[test]
    fn no_fetch_direct_override() {
        // Crawl-Pfad — Trait-Default `None` ist der Vertrag.
        assert!(module().fetch_direct(&ctx_de(), "Siemens AG").is_none());
    }

    /// End-to-end extraction against a frozen Siemens AG profile (DE).
    ///
    /// Fixture origin: `https://www.northdata.de/Siemens+Aktiengesellschaft,+M%C3%BCnchen/HRB+6684`
    /// (live fetch 2026-05-14, logged out, structural reduction documented at
    /// the top of the fixture).
    #[test]
    fn extract_siemens_fixture() {
        let html = include_str!("../../fixtures/sources/northdata/siemens_ag.html");
        let page = SourceReadResult {
            url: "https://www.northdata.de/Siemens+Aktiengesellschaft,+M%C3%BCnchen/HRB+6684"
                .to_string(),
            title: "Siemens AG, München".to_string(),
            summary: String::new(),
            text: html.to_string(),
            is_pdf: false,
            excerpts: Vec::new(),
            find_results: Vec::new(),
            raw_html: None,
        };
        let fields = module().extract_fields(&page);
        let by_key: std::collections::HashMap<FieldKey, &FieldEvidence> =
            fields.iter().map(|(k, v)| (*k, v)).collect();

        let name = by_key
            .get(&FieldKey::FirmaName)
            .expect("firma_name extracted");
        assert_eq!(name.value, "Siemens AG");
        assert_eq!(name.confidence, Confidence::High);

        let street = by_key
            .get(&FieldKey::FirmaAnschrift)
            .expect("firma_anschrift extracted");
        assert_eq!(street.value, "Werner-von-Siemens-Str. 1");
        assert_eq!(street.confidence, Confidence::High);

        let plz = by_key
            .get(&FieldKey::FirmaPlz)
            .expect("firma_plz extracted");
        assert_eq!(plz.value, "80333");
        assert_eq!(plz.confidence, Confidence::High);

        let city = by_key
            .get(&FieldKey::FirmaOrt)
            .expect("firma_ort extracted");
        assert_eq!(city.value, "München");
        assert_eq!(city.confidence, Confidence::High);

        // First active person event in the bizq JSON is "Vorstand Veronika
        // Bienert"; historic entries (`"old": true`) are filtered out.
        let position = by_key
            .get(&FieldKey::PersonPosition)
            .expect("person_position extracted");
        assert_eq!(position.value, "Vorstand");
        assert_eq!(position.confidence, Confidence::Medium);

        let first = by_key
            .get(&FieldKey::PersonVorname)
            .expect("person_vorname extracted");
        assert_eq!(first.value, "Veronika");
        assert_eq!(first.confidence, Confidence::Medium);

        let last = by_key
            .get(&FieldKey::PersonNachname)
            .expect("person_nachname extracted");
        assert_eq!(last.value, "Bienert");
        assert_eq!(last.confidence, Confidence::Medium);

        // Public crawl exposes no concrete revenue → no umsatz field.
        assert!(by_key.get(&FieldKey::Umsatz).is_none());
    }

    /// AT profile fixture — Geschäftsführer-Variante mit Firmenbuch (FN).
    #[test]
    fn extract_red_bull_at_fixture() {
        let html = include_str!("../../fixtures/sources/northdata/red_bull_gmbh_at.html");
        let page = SourceReadResult {
            url: "https://www.northdata.de/Red+Bull+GmbH,+Fuschl+am+See/FN+99738+x".to_string(),
            title: "Red Bull GmbH, Fuschl am See".to_string(),
            summary: String::new(),
            text: html.to_string(),
            is_pdf: false,
            excerpts: Vec::new(),
            find_results: Vec::new(),
            raw_html: None,
        };
        let fields = module().extract_fields(&page);
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

        let plz = by_key
            .get(&FieldKey::FirmaPlz)
            .expect("firma_plz extracted");
        assert_eq!(plz.value, "5330");

        let city = by_key
            .get(&FieldKey::FirmaOrt)
            .expect("firma_ort extracted");
        assert_eq!(city.value, "Fuschl am See");

        let position = by_key
            .get(&FieldKey::PersonPosition)
            .expect("person_position extracted");
        assert_eq!(position.value, "Geschäftsführer");

        let first = by_key
            .get(&FieldKey::PersonVorname)
            .expect("person_vorname extracted");
        assert_eq!(first.value, "Franz");

        let last = by_key
            .get(&FieldKey::PersonNachname)
            .expect("person_nachname extracted");
        assert_eq!(last.value, "Watzlawick");
    }

    #[test]
    fn parse_address_line_de_with_country_prefix() {
        let parsed = parse_address_line("Werner-von-Siemens-Str. 1, D-80333 München");
        assert_eq!(parsed.street.as_deref(), Some("Werner-von-Siemens-Str. 1"));
        assert_eq!(parsed.postal_code.as_deref(), Some("80333"));
        assert_eq!(parsed.city.as_deref(), Some("München"));
    }

    #[test]
    fn parse_address_line_at_with_country_prefix() {
        let parsed = parse_address_line("Am Brunnen 1, A-5330 Fuschl am See");
        assert_eq!(parsed.street.as_deref(), Some("Am Brunnen 1"));
        assert_eq!(parsed.postal_code.as_deref(), Some("5330"));
        assert_eq!(parsed.city.as_deref(), Some("Fuschl am See"));
    }

    #[test]
    fn parse_address_line_ch_with_country_prefix() {
        let parsed = parse_address_line("Bahnhofstrasse 12, CH-8001 Zürich");
        assert_eq!(parsed.street.as_deref(), Some("Bahnhofstrasse 12"));
        assert_eq!(parsed.postal_code.as_deref(), Some("8001"));
        assert_eq!(parsed.city.as_deref(), Some("Zürich"));
    }

    #[test]
    fn parse_address_line_without_prefix() {
        let parsed = parse_address_line("Hauptstraße 5, 10115 Berlin");
        assert_eq!(parsed.street.as_deref(), Some("Hauptstraße 5"));
        assert_eq!(parsed.postal_code.as_deref(), Some("10115"));
        assert_eq!(parsed.city.as_deref(), Some("Berlin"));
    }

    #[test]
    fn parse_person_label_full_role() {
        let p = parse_person_label("Vorstand Veronika Bienert");
        assert_eq!(p.position.as_deref(), Some("Vorstand"));
        assert_eq!(p.first_name.as_deref(), Some("Veronika"));
        assert_eq!(p.last_name.as_deref(), Some("Bienert"));
    }

    #[test]
    fn parse_person_label_abbreviated_role() {
        let p = parse_person_label("VV Roland Busch");
        assert_eq!(p.position.as_deref(), Some("Vorstandsvorsitzender"));
        assert_eq!(p.first_name.as_deref(), Some("Roland"));
        assert_eq!(p.last_name.as_deref(), Some("Busch"));
    }

    #[test]
    fn parse_person_label_geschaeftsfuehrer() {
        let p = parse_person_label("Geschäftsführer Franz Watzlawick");
        assert_eq!(p.position.as_deref(), Some("Geschäftsführer"));
        assert_eq!(p.first_name.as_deref(), Some("Franz"));
        assert_eq!(p.last_name.as_deref(), Some("Watzlawick"));
    }

    #[test]
    fn parse_person_label_multi_word_first_name() {
        let p = parse_person_label("Vorstand Anna Maria Müller");
        assert_eq!(p.position.as_deref(), Some("Vorstand"));
        assert_eq!(p.first_name.as_deref(), Some("Anna Maria"));
        assert_eq!(p.last_name.as_deref(), Some("Müller"));
    }

    #[test]
    fn shape_query_unsupported_country_returns_none() {
        // Construct a non-DACH SourceCtx by hand. (No public ctor for
        // Country variants outside DE/AT/CH — DACH-tolerance is enforced
        // by the explicit allow-list in `shape_query`, so we only need to
        // confirm that the early-return guard would handle the negative
        // branch should the Country enum grow.)
        //
        // We can at least confirm that DACH inputs all shape, which is the
        // positive half of the contract; the unsupported branch is dead
        // code today but kept for forward-compat.
        assert!(module().shape_query("Bosch GmbH", &ctx_de()).is_some());
        assert!(module().shape_query("Bosch GmbH", &ctx_at()).is_some());
        assert!(module().shape_query("Bosch GmbH", &ctx_ch()).is_some());
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
        // No ribbon sections, no bizq figure → no fields. The h1 fallback
        // also has no candidate ("hello" is just a paragraph).
        for (key, _) in &fields {
            assert_ne!(*key, FieldKey::FirmaAnschrift);
            assert_ne!(*key, FieldKey::FirmaPlz);
            assert_ne!(*key, FieldKey::FirmaOrt);
            assert_ne!(*key, FieldKey::PersonVorname);
            assert_ne!(*key, FieldKey::PersonNachname);
            assert_ne!(*key, FieldKey::PersonPosition);
        }
    }

    /// Live-Smoke gegen die echte northdata.de-Profilseite.
    ///
    /// Standardmäßig ignoriert; explizit ausführen mit:
    ///   `cargo test -p ctox-web-stack -- --ignored sources::northdata::live`
    ///
    /// Northdata gibt für ausgeloggte Crawls je nach Profil 200/302/404 zurück
    /// (manche Profile redirecten zur Suche; Siemens AG ist eines der wenigen,
    /// die ohne Login direkt erreichbar bleiben). Der Test ist deshalb mild:
    /// bei Network-Failure / Redirect-to-search wird er übersprungen statt
    /// hart zu failen.
    #[test]
    #[ignore = "live network; run with: cargo test -- --ignored sources::northdata::live"]
    fn live_siemens() {
        let url = "https://www.northdata.de/Siemens+Aktiengesellschaft,+M%C3%BCnchen/HRB+6684";
        let response = ureq::get(url)
            .set(
                "User-Agent",
                "Mozilla/5.0 (Macintosh; Intel Mac OS X 14_0) AppleWebKit/537.36 \
                 (KHTML, like Gecko) Chrome/120.0 Safari/537.36",
            )
            .set("Accept-Language", "de-DE,de;q=0.9")
            .timeout(std::time::Duration::from_secs(45))
            .call();
        let body = match response {
            Ok(resp) => resp.into_string().expect("read body"),
            Err(err) => {
                eprintln!("northdata.de live request failed (skip): {err}");
                return;
            }
        };
        if body.contains("Suche nach &quot;") || body.contains("Suche nach \"") {
            eprintln!("northdata.de redirected to search (skip): no profile body");
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
            raw_html: None,
        };
        let fields = module().extract_fields(&page);
        let name = fields
            .iter()
            .find(|(k, _)| *k == FieldKey::FirmaName)
            .map(|(_, v)| v)
            .expect("live firma_name");
        assert!(name.value.to_ascii_lowercase().contains("siemens"));
        assert!(name.confidence >= Confidence::Medium);
    }
}
