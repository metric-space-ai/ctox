//! `handelsregister.de` — Tier P, DE only.
//!
//! Offizielles, bundeslandsübergreifendes Portal der deutschen Handelsregister
//! (`https://www.handelsregister.de/rp_web/erweiterte-suche.xhtml`). Die Seite
//! ist eine JSF/PrimeFaces-SPA: jede Suche durchläuft eine Server-State-
//! Session, und nach wenigen Treffern wirft das Portal ein Captcha. Eine echte
//! Tiefen-Integration braucht den CTOX-Browser-Pfad
//! (`tools/web-stack/src/browser.rs`); im Phase-2-Scope bleibt der Adapter
//! deshalb **crawl-/best-effort**:
//!
//!   * `shape_query` pinnt die Provider-Cascade (Google/Brave) auf
//!     `site:handelsregister.de` plus den Schwester-Hub `unternehmensregister.de`.
//!     Wenn die Suchengine die Trefferliste schon abgegriffen hat (Cache,
//!     öffentlich zugängliche Snippets), liefert der Crawl bereits genug.
//!   * `extract_fields` parst **beide** Roh-Antwortformen:
//!       a) Treffer-Listen (`table.RegPortErg`) mit Firma + Sitz + Adresse,
//!       b) Detail-Seiten („Aktueller Abdruck") mit Firma, Anschrift, Sitz,
//!          Rechtsform und Personen-Block (Vorstand / Geschäftsführer /
//!          Prokura). Personenzeilen folgen dem Schema
//!          `Nachname, Vorname, Wohnort, *Geburtsdatum`.
//!     Sobald die Browser-Automation die JSF-Wall durchbricht und gerendertes
//!     HTML zurückgibt, greift dieselbe Extraktion ohne Anpassung.
//!   * `fetch_direct` bleibt `None`. Eine native API existiert nicht (RSS
//!     liefert nur Eintragungs-Diff-Streams, keine Firmen-Suche), und ein
//!     direkter POST an die Suche scheitert ohne JSF-View-State.
//!
//! Confidence-Konvention (vgl. EXCEL_MATRIX.md — handelsregister ist
//! authoritative für Firma-Stammdaten + Personen-Namen):
//!
//!   * `firma_name`, `firma_anschrift`, `firma_plz`, `firma_ort`
//!     bekommen [`Confidence::High`] (Pflichtveröffentlichung, gerichtlich
//!     geführt).
//!   * `person_vorname` / `person_nachname` bekommen [`Confidence::Medium`].
//!     Das Vorstands-/GF-Listing wechselt je Rechtsform (SE/AG: „Vorstand:",
//!     GmbH: „Geschäftsführer:", KG: „persönlich haftender Gesellschafter")
//!     und enthält gelegentlich nur historische Einträge. Die Excel-Matrix
//!     markiert diese Felder mit Asterisk; Medium ist der Default.

use std::sync::OnceLock;
use std::time::Duration;

use scraper::{ElementRef, Html, Selector};

use super::{
    Confidence, Country, FieldEvidence, FieldKey, ShapedQuery, SourceCtx, SourceModule,
    SourceReadResult, Tier,
};

const ID: &str = "handelsregister.de";
const DOMAIN: &str = "handelsregister.de";
const SISTER_DOMAIN: &str = "unternehmensregister.de";

struct Handelsregister;

impl SourceModule for Handelsregister {
    fn id(&self) -> &'static str {
        ID
    }

    fn aliases(&self) -> &'static [&'static str] {
        &["handelsregister", "hr"]
    }

    /// Handelsregister.de is JSF/PrimeFaces-rendered with a captcha wall,
    /// so the extraction pathway is delegated to the registered scrape
    /// target. Phase B: the script at
    /// `runtime/scraping/targets/handelsregister.de/scripts/current.js`
    /// mirrors the Rust `extract_from_html` below; when the DOM drifts,
    /// `universal-scraping` revises that JS file instead of the Rust
    /// code here. The Rust fallback stays as a baseline for unit-tests
    /// and for environments where no scrape target is registered.
    fn scrape_target_key(&self) -> Option<&'static str> {
        // The CTOX scrape registry normalises target keys to a dashed slug
        // (`.` → `-`); upsert-target rewrites `handelsregister.de` to
        // `handelsregister-de`.
        Some("handelsregister-de")
    }

    fn tier(&self) -> Tier {
        Tier::P
    }

    fn countries(&self) -> &'static [Country] {
        &[Country::De]
    }

    fn authoritative_for(&self) -> &'static [FieldKey] {
        // Aus EXCEL_MATRIX.md: handelsregister.de ist DE-only und über
        // „Impressum / Handelsregister" referenziert; in der Praxis deckt es
        // die Firma-Stammdaten plus die Geschäftsführer/Vorstand-Namen.
        &[
            FieldKey::FirmaName,
            FieldKey::FirmaAnschrift,
            FieldKey::FirmaPlz,
            FieldKey::FirmaOrt,
            FieldKey::PersonVorname,
            FieldKey::PersonNachname,
        ]
    }

    /// DE-only. `None` für AT/CH (Excel-Matrix führt handelsregister.de dort
    /// nicht). Leere/whitespace-Queries werden früh ausgesiebt.
    ///
    /// Die Query wird auf `site:handelsregister.de OR site:unternehmensregister.de`
    /// gepinnt. Letzteres ist der Schwester-Hub, der für börsennotierte und
    /// publizitätspflichtige Firmen häufig denselben Datenbestand spiegelt
    /// und (anders als handelsregister.de) auch ohne Captcha öffentliche
    /// Treffer-Snippets in Google-Index pusht.
    fn shape_query(&self, query: &str, ctx: &SourceCtx<'_>) -> Option<ShapedQuery> {
        // Strict: DE-only. `None`-Country wird als „unbekannt" interpretiert
        // und ebenfalls geblockt — der Orchestrator soll erst entscheiden,
        // welche Länder-Module er ranzieht.
        if ctx.country != Some(Country::De) {
            return None;
        }
        let trimmed = query.trim();
        if trimmed.is_empty() {
            return None;
        }
        Some(ShapedQuery {
            query: format!(
                "{trimmed} site:{DOMAIN} OR site:{SISTER_DOMAIN}"
            ),
            domains: vec![DOMAIN.to_string(), SISTER_DOMAIN.to_string()],
        })
    }

    fn extract_fields(&self, page: &SourceReadResult) -> Vec<(FieldKey, FieldEvidence)> {
        if page.is_pdf {
            return Vec::new();
        }
        extract_from_html(page.html_source(), &page.url)
    }
}

static MODULE: Handelsregister = Handelsregister;

pub fn module() -> &'static dyn SourceModule {
    &MODULE
}

// ---------------------------------------------------------------------------
// Selectors — `scraper::Selector::parse` ist nicht `const`, deshalb `OnceLock`.
// ---------------------------------------------------------------------------

fn result_table_selector() -> &'static Selector {
    static SEL: OnceLock<Selector> = OnceLock::new();
    SEL.get_or_init(|| Selector::parse("table.RegPortErg").expect("valid result-table selector"))
}

fn detail_table_selector() -> &'static Selector {
    // Detail-Ansicht („Aktueller Abdruck") nutzt dieselbe RegPortErg-Klasse,
    // aber die Zeilenstruktur ist label/value (zwei <td>) statt mehrspaltig.
    // Wir matchen erstmal alle RegPortErg-Tabellen und unterscheiden über
    // die Zeilen-Heuristik.
    result_table_selector()
}

fn row_selector() -> &'static Selector {
    static SEL: OnceLock<Selector> = OnceLock::new();
    SEL.get_or_init(|| Selector::parse("tr").expect("valid row selector"))
}

fn cell_selector() -> &'static Selector {
    static SEL: OnceLock<Selector> = OnceLock::new();
    SEL.get_or_init(|| Selector::parse("td").expect("valid cell selector"))
}

fn firma_cell_selector() -> &'static Selector {
    static SEL: OnceLock<Selector> = OnceLock::new();
    SEL.get_or_init(|| {
        Selector::parse("td.RegPortErg_FirmaSp").expect("valid firma-cell selector")
    })
}

fn sitz_cell_selector() -> &'static Selector {
    static SEL: OnceLock<Selector> = OnceLock::new();
    SEL.get_or_init(|| Selector::parse("td.RegPortErg_SitzSp").expect("valid sitz-cell selector"))
}

fn adresse_cell_selector() -> &'static Selector {
    static SEL: OnceLock<Selector> = OnceLock::new();
    SEL.get_or_init(|| {
        Selector::parse("td.RegPortErg_AdresseSp").expect("valid adresse-cell selector")
    })
}

// ---------------------------------------------------------------------------
// Core extraction
// ---------------------------------------------------------------------------

fn extract_from_html(html: &str, source_url: &str) -> Vec<(FieldKey, FieldEvidence)> {
    let document = Html::parse_document(html);
    let mut out: Vec<(FieldKey, FieldEvidence)> = Vec::new();

    // -----------------------------------------------------------------
    // (1) Detail-Ansicht („Aktueller Abdruck"): label/value-Tabelle.
    //     Wir scannen die RegPortErg-Tabellen und greifen die Label-Zeilen.
    //     Wenn wir hier Firma + Anschrift finden, ist das die belastbarere
    //     Quelle (gerichtlich-amtliche Pflichtveröffentlichung).
    // -----------------------------------------------------------------
    let mut had_detail = false;
    for table in document.select(detail_table_selector()) {
        let detail = parse_detail_table(&table);
        if detail.has_any() {
            had_detail = true;
            if let Some(name) = detail.firma {
                push(
                    &mut out,
                    FieldKey::FirmaName,
                    name,
                    Confidence::High,
                    source_url,
                    Some("Aktueller Abdruck — Firma"),
                );
            }
            if let Some((street, zip, city)) = detail.address {
                if !street.is_empty() {
                    push(
                        &mut out,
                        FieldKey::FirmaAnschrift,
                        street,
                        Confidence::High,
                        source_url,
                        Some("Aktueller Abdruck — Geschäftsanschrift"),
                    );
                }
                if !zip.is_empty() {
                    push(
                        &mut out,
                        FieldKey::FirmaPlz,
                        zip,
                        Confidence::High,
                        source_url,
                        Some("Aktueller Abdruck — Geschäftsanschrift"),
                    );
                }
                if !city.is_empty() {
                    push(
                        &mut out,
                        FieldKey::FirmaOrt,
                        city,
                        Confidence::High,
                        source_url,
                        Some("Aktueller Abdruck — Geschäftsanschrift"),
                    );
                }
            } else if let Some(sitz) = detail.sitz {
                // Fallback: nur Sitz, keine Straße/PLZ — schreibt nur Ort.
                push(
                    &mut out,
                    FieldKey::FirmaOrt,
                    sitz,
                    Confidence::High,
                    source_url,
                    Some("Aktueller Abdruck — Sitz"),
                );
            }
            // Personen: erster Eintrag aus Vorstand / Geschäftsführer / Inhaber.
            if let Some(person) = detail.first_person {
                if let Some(first) = person.first_name {
                    push(
                        &mut out,
                        FieldKey::PersonVorname,
                        first,
                        Confidence::Medium,
                        source_url,
                        Some(person.role_note),
                    );
                }
                if let Some(last) = person.last_name {
                    push(
                        &mut out,
                        FieldKey::PersonNachname,
                        last,
                        Confidence::Medium,
                        source_url,
                        Some(person.role_note),
                    );
                }
            }
            // Eine Detail-Tabelle reicht; weitere RegPortErg-Tabellen auf der
            // gleichen Seite sind in der Praxis Wiederholungen.
            break;
        }
    }

    // -----------------------------------------------------------------
    // (2) Treffer-Liste: wenn keine Detail-Daten extrahiert wurden, fallen
    //     wir auf die Trefferzeilen zurück. Erster Treffer = primärer Hit.
    // -----------------------------------------------------------------
    if !had_detail {
        if let Some(hit) = first_search_hit(&document) {
            if let Some(name) = hit.firma {
                push(
                    &mut out,
                    FieldKey::FirmaName,
                    name,
                    Confidence::High,
                    source_url,
                    Some("Trefferliste — Firma"),
                );
            }
            if let Some((street, zip, city)) = hit.address {
                if !street.is_empty() {
                    push(
                        &mut out,
                        FieldKey::FirmaAnschrift,
                        street,
                        Confidence::High,
                        source_url,
                        Some("Trefferliste — Adresse"),
                    );
                }
                if !zip.is_empty() {
                    push(
                        &mut out,
                        FieldKey::FirmaPlz,
                        zip,
                        Confidence::High,
                        source_url,
                        Some("Trefferliste — Adresse"),
                    );
                }
                if !city.is_empty() {
                    push(
                        &mut out,
                        FieldKey::FirmaOrt,
                        city,
                        Confidence::High,
                        source_url,
                        Some("Trefferliste — Adresse"),
                    );
                }
            } else if let Some(sitz) = hit.sitz {
                push(
                    &mut out,
                    FieldKey::FirmaOrt,
                    sitz,
                    Confidence::High,
                    source_url,
                    Some("Trefferliste — Sitz"),
                );
            }
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
    collapse_whitespace(&raw)
}

fn collapse_whitespace(input: &str) -> String {
    let mut buf = String::with_capacity(input.len());
    let mut prev_space = false;
    for ch in input.chars() {
        if ch.is_whitespace() {
            if !prev_space && !buf.is_empty() {
                buf.push(' ');
                prev_space = true;
            }
        } else {
            buf.push(ch);
            prev_space = false;
        }
    }
    if buf.ends_with(' ') {
        buf.pop();
    }
    buf
}

// ---------------------------------------------------------------------------
// Detail-Page Parser
// ---------------------------------------------------------------------------

#[derive(Debug, Default)]
struct DetailRecord {
    firma: Option<String>,
    sitz: Option<String>,
    address: Option<(String, String, String)>,
    first_person: Option<PersonRow>,
}

impl DetailRecord {
    fn has_any(&self) -> bool {
        self.firma.is_some()
            || self.sitz.is_some()
            || self.address.is_some()
            || self.first_person.is_some()
    }
}

#[derive(Debug)]
struct PersonRow {
    first_name: Option<String>,
    last_name: Option<String>,
    role_note: &'static str,
}

/// Parse a label/value RegPortErg detail table. Rows look like:
///   `<tr><td class="label">Firma:</td><td class="content">…</td></tr>`
/// We tolerate label/value swaps and additional decoration classes.
fn parse_detail_table(table: &ElementRef<'_>) -> DetailRecord {
    let mut record = DetailRecord::default();
    for row in table.select(row_selector()) {
        let cells: Vec<ElementRef<'_>> = row.select(cell_selector()).collect();
        if cells.len() < 2 {
            // colspan-Header-Zeilen (z. B. `RegPortErg_AZ`) sind hier
            // irrelevant — die Treffer-Logik unten verarbeitet sie.
            continue;
        }
        // colspan=5 (Trefferliste) ist keine Label/Value-Zeile.
        if cells.iter().any(|c| {
            c.value()
                .attr("colspan")
                .and_then(|v| v.parse::<u32>().ok())
                .map(|n| n >= 3)
                .unwrap_or(false)
        }) {
            continue;
        }
        let label_raw = node_text(&cells[0]);
        let value_raw = node_text(&cells[1]);
        let label = normalize_label(&label_raw);
        if label.is_empty() || value_raw.is_empty() {
            continue;
        }
        match label.as_str() {
            "firma" => {
                if record.firma.is_none() {
                    record.firma = Some(value_raw);
                }
            }
            "sitz" | "sitz / zweigniederlassung" | "sitz/zweigniederlassung" => {
                if record.sitz.is_none() {
                    record.sitz = Some(value_raw);
                }
            }
            "geschäftsanschrift" | "geschaftsanschrift" => {
                if record.address.is_none() {
                    record.address = Some(split_address(&value_raw));
                }
            }
            "vorstand" | "geschäftsführer" | "geschaftsfuhrer" | "geschäftsführerin"
            | "inhaber" | "persönlich haftender gesellschafter"
            | "personlich haftender gesellschafter" => {
                if record.first_person.is_none() {
                    record.first_person =
                        parse_person_block(&value_raw, role_note_for(&label));
                }
            }
            _ => {}
        }
    }
    record
}

fn normalize_label(raw: &str) -> String {
    raw.trim()
        .trim_end_matches(':')
        .trim()
        .to_ascii_lowercase()
}

fn role_note_for(label: &str) -> &'static str {
    match label {
        "vorstand" => "Aktueller Abdruck — Vorstand",
        "geschäftsführer" | "geschaftsfuhrer" | "geschäftsführerin" => {
            "Aktueller Abdruck — Geschäftsführer"
        }
        "inhaber" => "Aktueller Abdruck — Inhaber",
        "persönlich haftender gesellschafter" | "personlich haftender gesellschafter" => {
            "Aktueller Abdruck — phG"
        }
        _ => "Aktueller Abdruck — Vertretungsberechtigter",
    }
}

/// Split a single address string "Walter-Wittenstein-Straße 1, 97999 Igersheim"
/// into `(street, zip, city)`. The Handelsregister always renders this triple
/// in the same order, comma-separated. PLZ is a 5-digit prefix on the city
/// fragment. Best-effort: missing parts come back as empty strings.
fn split_address(raw: &str) -> (String, String, String) {
    let cleaned = collapse_whitespace(raw);
    let parts: Vec<&str> = cleaned.split(',').map(str::trim).collect();
    if parts.is_empty() {
        return (String::new(), String::new(), String::new());
    }
    let street = parts[0].to_string();
    let mut zip = String::new();
    let mut city = String::new();
    if parts.len() >= 2 {
        let tail = parts[1..].join(", ");
        let tail_trimmed = tail.trim();
        // Erster Token = PLZ (5 Ziffern), Rest = Ort.
        let mut split_iter = tail_trimmed.splitn(2, char::is_whitespace);
        if let Some(first) = split_iter.next() {
            if is_german_zip(first) {
                zip = first.to_string();
                city = split_iter.next().unwrap_or("").trim().to_string();
            } else {
                city = tail_trimmed.to_string();
            }
        }
    }
    (street, zip, city)
}

fn is_german_zip(token: &str) -> bool {
    token.len() == 5 && token.chars().all(|c| c.is_ascii_digit())
}

/// Parse the first vertretungsberechtigte Person from a Vorstand/GF-Block.
/// Format per row (separated by `;` or HTML `<br>` collapsed to whitespace):
///   `Nachname, Vorname, Ort, *TT.MM.JJJJ, einzelvertretungsberechtigt`
fn parse_person_block(raw: &str, role_note: &'static str) -> Option<PersonRow> {
    let normalized = collapse_whitespace(raw);
    // Mehrere Personen sind ';' getrennt; nimm die erste nicht-leere.
    let first_entry = normalized
        .split(';')
        .map(str::trim)
        .find(|s| !s.is_empty())?;
    let parts: Vec<&str> = first_entry.split(',').map(str::trim).collect();
    if parts.is_empty() || parts[0].is_empty() {
        return None;
    }
    let last_name = Some(parts[0].to_string());
    let first_name = parts.get(1).filter(|p| !p.is_empty()).map(|p| {
        // Wenn der "Vorname"-Slot ein Datum oder Funktions-Keyword ist,
        // weisen wir ihn ab (defensive — sollte das Format anders sein).
        if looks_like_date_token(p) || looks_like_function_keyword(p) {
            String::new()
        } else {
            p.to_string()
        }
    });
    let first_name = first_name.filter(|s| !s.is_empty());
    if last_name.is_none() && first_name.is_none() {
        return None;
    }
    Some(PersonRow {
        first_name,
        last_name,
        role_note,
    })
}

fn looks_like_date_token(token: &str) -> bool {
    let stripped = token.trim_start_matches('*');
    stripped
        .chars()
        .all(|c| c.is_ascii_digit() || c == '.' || c == ' ')
        && stripped.chars().any(|c| c.is_ascii_digit())
}

fn looks_like_function_keyword(token: &str) -> bool {
    let lower = token.to_ascii_lowercase();
    lower.contains("vertretungsberechtigt")
        || lower.contains("gesamt")
        || lower.contains("einzel")
        || lower.contains("prokura")
}

// ---------------------------------------------------------------------------
// Search-Result Parser
// ---------------------------------------------------------------------------

#[derive(Debug, Default)]
struct SearchHit {
    firma: Option<String>,
    sitz: Option<String>,
    address: Option<(String, String, String)>,
}

/// The first hit row in a `table.RegPortErg`. Layout:
///   `<tr class="RegPortErg_AZ"> Aktenzeichen </tr>`
///   `<tr> <td.RegPortErg_FirmaSp/> <td.RegPortErg_SitzSp/> … </tr>`
///   `<tr> <td.RegPortErg_AdresseSp colspan="5">… </td> </tr>`
fn first_search_hit(document: &Html) -> Option<SearchHit> {
    for table in document.select(result_table_selector()) {
        // Skip detail-shaped tables (label/value) — those don't have a
        // FirmaSp cell at all.
        let firma_cell = table.select(firma_cell_selector()).next();
        let firma_cell = match firma_cell {
            Some(cell) => cell,
            None => continue,
        };
        let mut hit = SearchHit {
            firma: Some(node_text(&firma_cell)),
            ..Default::default()
        };
        if let Some(sitz) = table.select(sitz_cell_selector()).next() {
            let raw = node_text(&sitz);
            if !raw.is_empty() {
                hit.sitz = Some(raw);
            }
        }
        if let Some(adresse) = table.select(adresse_cell_selector()).next() {
            let raw = node_text(&adresse);
            if !raw.is_empty() {
                hit.address = Some(split_address(&raw));
            }
        }
        // If even the firma is empty, the table is structurally broken; skip.
        if hit
            .firma
            .as_ref()
            .map(|s| !s.trim().is_empty())
            .unwrap_or(false)
        {
            return Some(hit);
        }
    }
    None
}

// ---------------------------------------------------------------------------
// Live-Probe — best-effort `ureq` GET against the public Suche.
// ---------------------------------------------------------------------------

#[cfg(test)]
fn fetch_raw_search(query: &str) -> Result<String, super::SourceError> {
    use super::SourceError;
    // Es gibt keinen stabilen Deep-Link auf die Suche (View-State); wir
    // testen lediglich, dass die Landing-Page überhaupt antwortet. Eine
    // echte Such-Round-trip braucht den Browser-Pfad.
    let url = format!(
        "https://www.handelsregister.de/rp_web/welcome.xhtml?q={}",
        urlencode(query)
    );
    let agent = ureq::AgentBuilder::new()
        .timeout(Duration::from_millis(12_000))
        .build();
    let response = agent
        .get(&url)
        .set(
            "User-Agent",
            "Mozilla/5.0 (Macintosh; Intel Mac OS X 14_0) \
             AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0 Safari/537.36",
        )
        .set("Accept-Language", "de-DE,de;q=0.9")
        .call();
    match response {
        Ok(resp) => {
            let status = resp.status();
            // Wenn die Seite ein Captcha-Layer einblendet, ist das
            // serverseitig kein 4xx — sie liefert 200 mit dem Captcha-Markup.
            // Wir treten das nach oben als `Blocked` durch, sobald die
            // Antwort verdächtig wirkt.
            let body = resp
                .into_string()
                .map_err(|err| SourceError::Network(err.into()))?;
            if looks_like_captcha(&body) {
                Err(SourceError::Blocked {
                    reason: format!("captcha wall (status {status})"),
                })
            } else {
                Ok(body)
            }
        }
        Err(ureq::Error::Status(code, _)) if matches!(code, 403 | 429) => {
            Err(SourceError::Blocked {
                reason: format!("HTTP {code}"),
            })
        }
        Err(err) => Err(SourceError::Network(err.into())),
    }
}

#[cfg(test)]
fn looks_like_captcha(body: &str) -> bool {
    let lower = body.to_ascii_lowercase();
    lower.contains("captcha") || lower.contains("bitte bestätigen sie")
}

#[cfg(test)]
fn urlencode(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    for ch in input.chars() {
        if ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_' | '.' | '~') {
            out.push(ch);
        } else if ch == ' ' {
            out.push('+');
        } else {
            for byte in ch.to_string().as_bytes() {
                out.push_str(&format!("%{:02X}", byte));
            }
        }
    }
    out
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
        assert_eq!(m.id(), "handelsregister.de");
        assert!(m.aliases().contains(&"handelsregister"));
        assert!(m.aliases().contains(&"hr"));
        assert_eq!(m.tier(), Tier::P);
        assert_eq!(m.countries(), &[Country::De]);
        assert!(m.requires_credential().is_none());
        for field in [
            FieldKey::FirmaName,
            FieldKey::FirmaAnschrift,
            FieldKey::FirmaPlz,
            FieldKey::FirmaOrt,
            FieldKey::PersonVorname,
            FieldKey::PersonNachname,
        ] {
            assert!(
                m.authoritative_for().contains(&field),
                "expected {field:?} in authoritative_for"
            );
        }
    }

    #[test]
    fn shape_query_de_pins_both_domains() {
        let shaped = module()
            .shape_query("WITTENSTEIN SE", &ctx_de())
            .expect("DE must shape");
        assert!(shaped.query.contains("WITTENSTEIN SE"));
        assert!(shaped.query.contains("site:handelsregister.de"));
        assert!(shaped.query.contains("site:unternehmensregister.de"));
        assert_eq!(
            shaped.domains,
            vec![
                "handelsregister.de".to_string(),
                "unternehmensregister.de".to_string()
            ]
        );
    }

    #[test]
    fn shape_query_non_de_returns_none() {
        assert!(module().shape_query("Red Bull GmbH", &ctx_at()).is_none());
        assert!(module().shape_query("Roche Holding AG", &ctx_none()).is_none());
    }

    #[test]
    fn shape_query_empty_returns_none() {
        assert!(module().shape_query("   ", &ctx_de()).is_none());
    }

    #[test]
    fn no_fetch_direct_override() {
        // Crawl-Pfad — der Trait-Default ist `None`, das ist der Vertrag.
        assert!(module().fetch_direct(&ctx_de(), "WITTENSTEIN SE").is_none());
    }

    #[test]
    fn split_address_typical() {
        let (street, zip, city) =
            super::split_address("Walter-Wittenstein-Straße 1, 97999 Igersheim");
        assert_eq!(street, "Walter-Wittenstein-Straße 1");
        assert_eq!(zip, "97999");
        assert_eq!(city, "Igersheim");
    }

    #[test]
    fn split_address_no_zip() {
        let (street, zip, city) = super::split_address("Hauptstr. 5, Berlin");
        assert_eq!(street, "Hauptstr. 5");
        assert_eq!(zip, "");
        assert_eq!(city, "Berlin");
    }

    #[test]
    fn split_address_only_street() {
        let (street, zip, city) = super::split_address("Königsallee 1");
        assert_eq!(street, "Königsallee 1");
        assert!(zip.is_empty());
        assert!(city.is_empty());
    }

    #[test]
    fn parse_person_block_single() {
        let row = super::parse_person_block(
            "Schult, Bertram, Igersheim, *17.05.1962, einzelvertretungsberechtigt",
            "test",
        )
        .expect("person parsed");
        assert_eq!(row.last_name.as_deref(), Some("Schult"));
        assert_eq!(row.first_name.as_deref(), Some("Bertram"));
    }

    #[test]
    fn parse_person_block_picks_first_of_many() {
        let row = super::parse_person_block(
            "Schult, Bertram, Igersheim, *17.05.1962; Brandstetter, Thomas, Igersheim, *03.09.1969",
            "test",
        )
        .expect("person parsed");
        assert_eq!(row.last_name.as_deref(), Some("Schult"));
        assert_eq!(row.first_name.as_deref(), Some("Bertram"));
    }

    #[test]
    fn parse_person_block_rejects_date_in_first_name_slot() {
        // Defensive: wenn die Quelle das Format kippt und nach dem Nachnamen
        // direkt das Datum kommt, weisen wir den Vornamen ab.
        let row = super::parse_person_block("Mustermann, *01.01.1970", "test")
            .expect("partial parsed");
        assert_eq!(row.last_name.as_deref(), Some("Mustermann"));
        assert!(row.first_name.is_none());
    }

    /// End-to-end extraction against a frozen WITTENSTEIN SE Treffer-Seite.
    ///
    /// Fixture-Ursprung: Markup einer öffentlich sichtbaren
    /// `erweiterte-suche.xhtml`-Trefferliste; auf das stabile RegPortErg-
    /// Schema reduziert. Captcha-Wall ist hier nicht abgebildet, weil die
    /// Trefferliste erst nach erfolgreichem Captcha gerendert wird.
    #[test]
    fn extract_search_wittenstein_fixture() {
        let html = include_str!(
            "../../fixtures/sources/handelsregister/search_wittenstein.html"
        );
        let page = SourceReadResult {
            url: "https://www.handelsregister.de/rp_web/erweiterte-suche.xhtml".to_string(),
            title: "Suchergebnis - Handelsregister".to_string(),
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
        assert_eq!(name.value, "WITTENSTEIN SE");
        assert_eq!(name.confidence, Confidence::High);

        let street = by_key
            .get(&FieldKey::FirmaAnschrift)
            .expect("firma_anschrift extracted");
        assert_eq!(street.value, "Walter-Wittenstein-Straße 1");
        assert_eq!(street.confidence, Confidence::High);

        let zip = by_key.get(&FieldKey::FirmaPlz).expect("firma_plz extracted");
        assert_eq!(zip.value, "97999");

        let city = by_key.get(&FieldKey::FirmaOrt).expect("firma_ort extracted");
        assert_eq!(city.value, "Igersheim");

        // Trefferliste kennt keine Personen → keine person_*-Felder.
        assert!(by_key.get(&FieldKey::PersonVorname).is_none());
        assert!(by_key.get(&FieldKey::PersonNachname).is_none());
    }

    /// End-to-end extraction against a frozen „Aktueller Abdruck" der
    /// WITTENSTEIN SE. Hier sollen Firma + Anschrift + Vorstand-Person
    /// extrahiert werden.
    #[test]
    fn extract_detail_wittenstein_fixture() {
        let html = include_str!(
            "../../fixtures/sources/handelsregister/detail_wittenstein.html"
        );
        let page = SourceReadResult {
            url: "https://www.handelsregister.de/rp_web/charge-info.xhtml".to_string(),
            title: "Aktueller Abdruck - WITTENSTEIN SE".to_string(),
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
        assert_eq!(name.value, "WITTENSTEIN SE");
        assert_eq!(name.confidence, Confidence::High);

        let street = by_key
            .get(&FieldKey::FirmaAnschrift)
            .expect("firma_anschrift extracted");
        assert_eq!(street.value, "Walter-Wittenstein-Straße 1");

        let zip = by_key.get(&FieldKey::FirmaPlz).expect("firma_plz extracted");
        assert_eq!(zip.value, "97999");

        let city = by_key.get(&FieldKey::FirmaOrt).expect("firma_ort extracted");
        assert_eq!(city.value, "Igersheim");

        // Vorstand: erster Eintrag ist „Schult, Bertram".
        let first = by_key
            .get(&FieldKey::PersonVorname)
            .expect("person_vorname extracted");
        assert_eq!(first.value, "Bertram");
        assert_eq!(first.confidence, Confidence::Medium);

        let last = by_key
            .get(&FieldKey::PersonNachname)
            .expect("person_nachname extracted");
        assert_eq!(last.value, "Schult");
        assert_eq!(last.confidence, Confidence::Medium);
    }

    #[test]
    fn extract_empty_on_unrelated_html() {
        let page = SourceReadResult {
            url: "https://example.com/".to_string(),
            title: "x".to_string(),
            summary: String::new(),
            text: "<html><body><p>nothing here</p></body></html>".to_string(),
            is_pdf: false,
            excerpts: Vec::new(),
            find_results: Vec::new(),
            raw_html: None,
        };
        let fields = module().extract_fields(&page);
        assert!(fields.is_empty(), "unexpected fields: {fields:?}");
    }

    #[test]
    fn extract_returns_empty_on_pdf_payload() {
        let page = SourceReadResult {
            url: "https://www.handelsregister.de/rp_web/foo.pdf".to_string(),
            title: "Auszug".to_string(),
            summary: String::new(),
            text: String::new(),
            is_pdf: true,
            excerpts: Vec::new(),
            find_results: Vec::new(),
            raw_html: None,
        };
        assert!(module().extract_fields(&page).is_empty());
    }

    /// Live-Smoke gegen handelsregister.de. Wegen Captcha/JSF-Wall in der
    /// Regel `SourceError::Blocked` — das ist das dokumentierte Erwartungs-
    /// Resultat, kein Test-Fehler. Echte Field-Extraction läuft hier nur,
    /// wenn ein Operator vor dem Test manuell den Browser-Pfad bedient hat.
    #[test]
    #[ignore = "live network; run with: cargo test -p ctox-web-stack -- --ignored sources::handelsregister"]
    fn live_smoke_blocked_is_ok() {
        match fetch_raw_search("WITTENSTEIN SE") {
            Ok(body) => {
                // Wenn der Crawl wider Erwarten durchkommt, prüfen wir, dass
                // wir wenigstens ein bekanntes Stück Markup sehen.
                assert!(
                    body.contains("handelsregister")
                        || body.to_ascii_lowercase().contains("registerportal"),
                    "unexpected body (live): {}",
                    &body[..body.len().min(200)]
                );
            }
            Err(super::super::SourceError::Blocked { reason }) => {
                eprintln!("handelsregister.de live blocked (expected): {reason}");
            }
            Err(super::super::SourceError::Network(err)) => {
                eprintln!("handelsregister.de live network error (skip): {err}");
            }
            Err(other) => {
                panic!("unexpected live error: {other}");
            }
        }
    }
}
