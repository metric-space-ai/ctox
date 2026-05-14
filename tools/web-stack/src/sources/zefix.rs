//! `zefix.ch` — Tier P, CH only.
//!
//! Offizielles Schweizer Handelsregister mit dokumentierter REST-API. Die
//! Quelle dient als Referenz-Implementation für API-Pfad-Module (D&B Hoovers,
//! Leadfeeder, LinkedIn, XING). `fetch_direct` schickt einen direkten
//! POST-Request an `firm/search`, parst das JSON und gibt die obersten
//! Treffer als [`SourceHit`]s zurück; `extract_fields` läuft entweder über
//! die JSON-Detail-Antwort (`firm/{ehraid}.json`) oder fällt auf den
//! HTML-Profil-Text der SPA zurück.
//!
//! ## Endpoints
//!
//! Die SPA unter `https://www.zefix.admin.ch` ruft folgende offene Endpoints
//! ohne Authentifizierung auf (separat vom dokumentierten `ZefixPublicREST`,
//! das HTTP-Basic-Auth verlangt):
//!
//! * `POST  https://www.zefix.admin.ch/ZefixREST/api/v1/firm/search`
//!   Body: `{"name": "<query>", "languageKey": "DE", "activeOnly": true}`.
//!   Antwort: `{"list": [CompanyShort], ...}`.
//! * `GET   https://www.zefix.admin.ch/ZefixREST/api/v1/firm/<ehraid>.json`
//!   Antwort: `CompanyFull` mit `address`, `purpose`, `shabPub[]`.
//!
//! ## Personen (Geschäftsführer / Verwaltungsrat)
//!
//! Der öffentliche Pfad liefert Personen NICHT als strukturierte
//! `personRelations` (das ist die `ZefixPublicREST`-Auth-API), sondern
//! eingebettet in `shabPub[*].message` als deutscher Fließtext mit
//! `<FT TYPE="P">` / `<FT TYPE="O">`-Markern und Funktions-Phrasen
//! ("Mitglied des Verwaltungsrates", "Präsident", "mit Einzelunterschrift",
//! …). [`extract_fields`] zieht diese Personen mit `Confidence::Medium`
//! heraus; strukturierte Adress-Felder bekommen `Confidence::High`.

use std::time::Duration;

use anyhow::anyhow;
use serde_json::{json, Value};

use super::{
    Confidence, Country, FieldEvidence, FieldKey, ShapedQuery, SourceCtx, SourceError,
    SourceHit, SourceModule, SourceReadResult, Tier,
};

const API_BASE: &str = "https://www.zefix.admin.ch/ZefixREST/api/v1";
const PROFILE_BASE: &str = "https://www.zefix.admin.ch/de/search/entity/list/firm";
const MAX_HITS: usize = 8;
const TIMEOUT_MS: u64 = 12_000;
const USER_AGENT: &str = "ctox-web-stack/0.1 (+https://ctox.local)";

struct Zefix;

impl SourceModule for Zefix {
    fn id(&self) -> &'static str {
        "zefix.ch"
    }

    fn aliases(&self) -> &'static [&'static str] {
        &["zefix"]
    }

    fn tier(&self) -> Tier {
        Tier::P
    }

    fn countries(&self) -> &'static [Country] {
        &[Country::Ch]
    }

    fn authoritative_for(&self) -> &'static [FieldKey] {
        &[
            FieldKey::FirmaName,
            FieldKey::FirmaAnschrift,
            FieldKey::FirmaPlz,
            FieldKey::FirmaOrt,
            FieldKey::PersonVorname,
            FieldKey::PersonNachname,
            FieldKey::PersonFunktion,
        ]
    }

    fn requires_credential(&self) -> Option<&'static str> {
        // Der `firm/search`-Pfad ist offen; der dokumentierte
        // `ZefixPublicREST`-Pfad würde Basic-Auth verlangen, ist hier aber
        // nicht aktiviert. Bleibt `None`.
        None
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
        // CH-only. Nicht-CH-Tenants signalisieren wir mit `None` zurück,
        // damit der Orchestrator die Quelle still überspringt.
        if matches!(ctx.country, Some(country) if country != Country::Ch) {
            return None;
        }

        let trimmed = company.trim();
        if trimmed.is_empty() {
            return Some(Err(SourceError::NoMatch));
        }
        // Zefix verlangt mindestens 3 Zeichen Suchbegriff (siehe OpenAPI:
        // `CompanySearchQuery.name.minLength=3`). Sub-3-Char-Anfragen sind
        // strukturell `no_match`, nicht parse_failed.
        if trimmed.chars().count() < 3 {
            return Some(Err(SourceError::NoMatch));
        }

        let agent = build_agent();
        let body = json!({
            "name": trimmed,
            "languageKey": "DE",
            "activeOnly": true,
        });
        Some(perform_search(&agent, &body))
    }

    fn extract_fields(&self, page: &SourceReadResult) -> Vec<(FieldKey, FieldEvidence)> {
        // JSON-Detail-Antwort? Dann strukturiert extrahieren (High).
        if let Some(json) = parse_detail_json(&page.text) {
            return extract_from_json(&json, &page.url);
        }
        // Sonst HTML-Profil-Fallback (Medium).
        extract_from_html(&page.text, &page.url)
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

fn perform_search(agent: &ureq::Agent, body: &Value) -> Result<Vec<SourceHit>, SourceError> {
    let url = format!("{API_BASE}/firm/search");
    let response = agent
        .post(&url)
        .set("accept", "application/json")
        .set("content-type", "application/json")
        .send_json(body.clone());

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
    let value: Value = serde_json::from_str(&text)
        .map_err(|err| SourceError::ParseFailed { detail: err.to_string() })?;

    parse_search_hits(&value)
}

fn classify_status(status: u16, resp: ureq::Response) -> SourceError {
    match status {
        429 => {
            let retry = resp
                .header("retry-after")
                .and_then(|v| v.parse::<u64>().ok())
                .map(|secs| secs.saturating_mul(1_000));
            SourceError::RateLimited { retry_after_ms: retry }
        }
        401 | 403 => SourceError::Blocked {
            reason: format!("http {status}"),
        },
        400 | 404 => SourceError::NoMatch,
        _ => {
            let detail = resp
                .into_string()
                .unwrap_or_else(|_| format!("http {status}"));
            SourceError::Other(anyhow!("zefix http {status}: {detail}"))
        }
    }
}

// ---------------------------------------------------------------------------
// Parsing: Search
// ---------------------------------------------------------------------------

fn parse_search_hits(value: &Value) -> Result<Vec<SourceHit>, SourceError> {
    let list = value
        .get("list")
        .and_then(Value::as_array)
        .ok_or_else(|| SourceError::ParseFailed {
            detail: "missing `list` array".to_string(),
        })?;
    if list.is_empty() {
        return Err(SourceError::NoMatch);
    }

    let mut hits = Vec::with_capacity(list.len().min(MAX_HITS));
    for entry in list.iter().take(MAX_HITS) {
        if let Some(hit) = company_short_to_hit(entry) {
            hits.push(hit);
        }
    }
    if hits.is_empty() {
        return Err(SourceError::NoMatch);
    }
    Ok(hits)
}

fn company_short_to_hit(entry: &Value) -> Option<SourceHit> {
    let name = entry.get("name").and_then(Value::as_str)?.trim();
    if name.is_empty() {
        return None;
    }
    // The profile-page URL the SPA uses (klickbar; HTML wird später vom
    // generischen Read-Pfad geladen). `ehraid` ist robust; fällt back auf
    // `chid` für legacy IDs.
    let url = if let Some(ehraid) = entry.get("ehraid").and_then(Value::as_i64) {
        format!("{PROFILE_BASE}/{ehraid}")
    } else if let Some(chid) = entry.get("chid").and_then(Value::as_str) {
        format!("{PROFILE_BASE}/{}", chid.trim())
    } else {
        return None;
    };

    let uid = entry
        .get("uidFormatted")
        .and_then(Value::as_str)
        .or_else(|| entry.get("uid").and_then(Value::as_str))
        .unwrap_or("");
    let seat = entry.get("legalSeat").and_then(Value::as_str).unwrap_or("");
    let status = entry.get("status").and_then(Value::as_str).unwrap_or("");
    let snippet = format!("{uid} · {seat} · {status}")
        .trim_matches(|c: char| c == ' ' || c == '·')
        .to_string();

    Some(SourceHit {
        title: name.to_string(),
        url,
        snippet,
    })
}

// ---------------------------------------------------------------------------
// Parsing: Detail (JSON)
// ---------------------------------------------------------------------------

fn parse_detail_json(raw: &str) -> Option<Value> {
    let trimmed = raw.trim_start();
    if !trimmed.starts_with('{') {
        return None;
    }
    let value: Value = serde_json::from_str(trimmed).ok()?;
    // Heuristik: ein Detail-Body hat `name` und mindestens eines von
    // `ehraid`, `uid`, `address`, `purpose`. Bewahrt uns davor, beliebige
    // JSON-Bodies versehentlich als Zefix-Detail zu interpretieren.
    let has_name = value.get("name").and_then(Value::as_str).is_some();
    let has_marker = ["ehraid", "uid", "address", "purpose"]
        .iter()
        .any(|key| value.get(*key).is_some());
    if has_name && has_marker {
        Some(value)
    } else {
        None
    }
}

fn extract_from_json(value: &Value, source_url: &str) -> Vec<(FieldKey, FieldEvidence)> {
    let mut out = Vec::new();
    let url = source_url.to_string();

    if let Some(name) = value.get("name").and_then(Value::as_str) {
        push_high(&mut out, FieldKey::FirmaName, name, &url);
    }

    if let Some(address) = value.get("address") {
        if let Some(line) = compose_street(address) {
            push_high(&mut out, FieldKey::FirmaAnschrift, &line, &url);
        }
        if let Some(zip) = address.get("swissZipCode").and_then(Value::as_str) {
            let zip = zip.trim();
            if !zip.is_empty() {
                push_high(&mut out, FieldKey::FirmaPlz, zip, &url);
            }
        }
        // Schema-Drift abfangen: offizielles Schema sagt `city`, die
        // ungeschützte SPA-API liefert `town`. Beide akzeptieren.
        if let Some(town) = address
            .get("town")
            .and_then(Value::as_str)
            .or_else(|| address.get("city").and_then(Value::as_str))
        {
            let town = town.trim();
            if !town.is_empty() {
                push_high(&mut out, FieldKey::FirmaOrt, town, &url);
            }
        }
    }

    // Personen aus `shabPub[*].message` ziehen (Medium, weil Freitext).
    if let Some(shab_pub) = value.get("shabPub").and_then(Value::as_array) {
        let mut seen: Vec<(String, String, String)> = Vec::new();
        for entry in shab_pub {
            if let Some(message) = entry.get("message").and_then(Value::as_str) {
                for person in parse_persons_from_shab_message(message) {
                    let key = (
                        person.vorname.to_lowercase(),
                        person.nachname.to_lowercase(),
                        person.funktion.to_lowercase(),
                    );
                    if seen.iter().any(|prev| prev == &key) {
                        continue;
                    }
                    seen.push(key);
                    push_medium(&mut out, FieldKey::PersonVorname, &person.vorname, &url);
                    push_medium(&mut out, FieldKey::PersonNachname, &person.nachname, &url);
                    if !person.funktion.is_empty() {
                        push_medium(&mut out, FieldKey::PersonFunktion, &person.funktion, &url);
                    }
                }
            }
        }
    }

    out
}

fn compose_street(address: &Value) -> Option<String> {
    let street = address.get("street").and_then(Value::as_str).unwrap_or("").trim();
    let house = address
        .get("houseNumber")
        .and_then(Value::as_str)
        .unwrap_or("")
        .trim();
    let addon = address.get("addon").and_then(Value::as_str).unwrap_or("").trim();
    let mut parts: Vec<&str> = Vec::new();
    if !street.is_empty() {
        parts.push(street);
    }
    if !house.is_empty() {
        parts.push(house);
    }
    if parts.is_empty() {
        return None;
    }
    let mut joined = parts.join(" ");
    if !addon.is_empty() {
        joined.push_str(", ");
        joined.push_str(addon);
    }
    Some(joined)
}

// ---------------------------------------------------------------------------
// Parsing: Persons in SHAB messages
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
struct Person {
    vorname: String,
    nachname: String,
    funktion: String,
}

/// Splittet eine SHAB-Mitteilung am Abschnittsmarker ("Eingetragene Personen
/// neu oder mutierend:") und parsed alle Personen-Klauseln. Ignoriert
/// "Ausgeschiedene Personen"-Sektionen — die beschreiben Abgänge.
fn parse_persons_from_shab_message(message: &str) -> Vec<Person> {
    let cleaned = strip_ft_tags(message);
    let mut out = Vec::new();
    for section in person_sections(&cleaned) {
        for clause in split_person_clauses(section) {
            if let Some(person) = parse_person_clause(clause) {
                out.push(person);
            }
        }
    }
    out
}

fn strip_ft_tags(input: &str) -> String {
    // Entfernt `<FT TYPE="...">`/`</FT>` und decodiert `&apos;`, `&quot;`,
    // `&amp;`. Andere XML-Entities sind in SHAB-Texten extrem selten.
    let mut buf = String::with_capacity(input.len());
    let mut chars = input.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '<' {
            // Skip until matching '>'
            for nc in chars.by_ref() {
                if nc == '>' {
                    break;
                }
            }
        } else {
            buf.push(c);
        }
    }
    buf.replace("&apos;", "'")
        .replace("&quot;", "\"")
        .replace("&amp;", "&")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
}

fn person_sections(message: &str) -> Vec<&str> {
    // Zefix nutzt ein paar fixe Header-Phrasen, die einen "neue/mutierende
    // Personen"-Block einleiten. Jede solche Phrase wird zu einem Eintrag.
    // Der Block läuft bis zum nächsten bekannten Sektions-Header oder bis
    // zum String-Ende. Wir können *nicht* einfach an "<Satz>. <Großbuchstabe>"
    // brechen, weil SHAB-Texte voller Titel-Abkürzungen sind ("Dr. Bruno",
    // "Prof. Müller"), die wir sonst mitten in einer Personen-Klausel
    // abschneiden würden.
    const PERSON_HEADERS: &[&str] = &[
        "Eingetragene Personen neu oder mutierend:",
        "Eingetragene Personen:",
        "Neue eingetragene Personen:",
        "Personen neu oder mutierend:",
    ];
    const TERMINATOR_HEADERS: &[&str] = &[
        "Ausgeschiedene Personen",
        "Statutenänderung",
        "Sitzverlegung",
        "Bonität",
        "Zweck:",
        "Domizil:",
    ];
    let mut sections = Vec::new();
    for header in PERSON_HEADERS {
        let mut search_from = 0;
        while let Some(rel) = message[search_from..].find(header) {
            let start = search_from + rel + header.len();
            let end = TERMINATOR_HEADERS
                .iter()
                .filter_map(|term| message[start..].find(term).map(|p| start + p))
                .min()
                .unwrap_or(message.len());
            sections.push(message[start..end].trim());
            search_from = end;
        }
    }
    sections
}

fn split_person_clauses(section: &str) -> Vec<&str> {
    // Personen sind mit "; " separiert. Innerhalb einer Klausel können
    // ", " als Feldtrenner stehen (Nachname, Vorname, Herkunft, in Wohnort,
    // Funktion, Zeichnungsberechtigung).
    section
        .split(';')
        .map(str::trim)
        .filter(|c| !c.is_empty())
        .collect()
}

fn parse_person_clause(clause: &str) -> Option<Person> {
    // Erwartetes Schema: "Nachname, Vorname[, Titel], <Herkunft>, in <Ort>,
    // <Funktion>[, <Zeichnungsbefugnis>]". Wir nehmen den zweiten Eintrag
    // nicht naiv als Vorname, weil "Dr. Bruno" usw. vorkommt. Strategie:
    //   * Wort 1 = Nachname (alles vor erstem Komma).
    //   * Wort 2 = Vorname (alles zwischen 1. und 2. Komma; Titel-Präfixe
    //     wie "Dr.", "Prof.", "Dr.-Ing." werden abgeschnitten).
    //   * Funktion = erste Phrase, die ein bekanntes Funktions-Stichwort
    //     enthält.
    let parts: Vec<&str> = clause.split(',').map(str::trim).collect();
    if parts.len() < 2 {
        return None;
    }
    let nachname = parts[0].trim();
    if nachname.is_empty() || nachname.split_whitespace().count() > 4 {
        // Sehr lange erste Token sind fast immer kein Personenname,
        // sondern Konzern-Boilerplate.
        return None;
    }
    let vorname_raw = parts[1].trim();
    let vorname = strip_title_prefix(vorname_raw);
    if vorname.is_empty() || !starts_with_uppercase(&vorname) {
        return None;
    }
    if !starts_with_uppercase(nachname) {
        return None;
    }
    let funktion = parts
        .iter()
        .skip(2)
        .map(|p| p.trim())
        .find(|p| is_funktion_token(p))
        .map(|p| normalize_funktion(p))
        .unwrap_or_default();

    Some(Person {
        vorname,
        nachname: nachname.to_string(),
        funktion,
    })
}

fn strip_title_prefix(raw: &str) -> String {
    const PREFIXES: &[&str] = &["Dr.", "Dr.-Ing.", "Prof.", "Prof. Dr.", "Dipl.", "Ing."];
    let mut s = raw.trim().to_string();
    loop {
        let mut stripped = false;
        for p in PREFIXES {
            if let Some(rest) = s.strip_prefix(p) {
                s = rest.trim_start().to_string();
                stripped = true;
                break;
            }
        }
        if !stripped {
            break;
        }
    }
    s
}

fn starts_with_uppercase(s: &str) -> bool {
    s.chars().next().map(|c| c.is_uppercase()).unwrap_or(false)
}

fn is_funktion_token(phrase: &str) -> bool {
    const KEYWORDS: &[&str] = &[
        "Mitglied des Verwaltungsrates",
        "Verwaltungsrat",
        "Präsident",
        "Vizepräsident",
        "Geschäftsführer",
        "Geschäftsführerin",
        "Gesellschafter",
        "Vorsitzender",
        "Direktor",
        "Direktorin",
    ];
    KEYWORDS.iter().any(|kw| phrase.contains(kw))
}

fn normalize_funktion(phrase: &str) -> String {
    // Nimm das spezifischste bekannte Funktions-Token aus der Phrase
    // (z. B. "Mitglied des Verwaltungsrates" statt "des Verwaltungsrates").
    const KEYWORDS: &[&str] = &[
        "Mitglied des Verwaltungsrates",
        "Präsident des Verwaltungsrates",
        "Vizepräsident des Verwaltungsrates",
        "Vorsitzender der Geschäftsleitung",
        "Geschäftsführerin",
        "Geschäftsführer",
        "Gesellschafterin",
        "Gesellschafter",
        "Verwaltungsrat",
        "Vizepräsident",
        "Präsident",
        "Direktorin",
        "Direktor",
    ];
    for kw in KEYWORDS {
        if phrase.contains(kw) {
            return (*kw).to_string();
        }
    }
    phrase.to_string()
}

// ---------------------------------------------------------------------------
// Parsing: HTML profile fallback
// ---------------------------------------------------------------------------

fn extract_from_html(text: &str, source_url: &str) -> Vec<(FieldKey, FieldEvidence)> {
    // Die Zefix-SPA rendert clientseitig — der initiale HTML-Body enthält
    // keine Stammdaten. Wenn jemand uns dennoch HTML-Text reicht (z. B.
    // ein archivierter Profil-Snapshot), versuchen wir, PLZ+Ort aus einer
    // Adress-Zeile und den Firmennamen aus dem `<title>` zu lesen.
    let mut out = Vec::new();
    let url = source_url.to_string();

    if let Some(title) = extract_title(text) {
        let cleaned = title
            .trim()
            .trim_end_matches("- Zefix")
            .trim_end_matches("| Zefix")
            .trim()
            .to_string();
        if !cleaned.is_empty() && !cleaned.eq_ignore_ascii_case("Zefix") {
            push_medium(&mut out, FieldKey::FirmaName, &cleaned, &url);
        }
    }

    // Suche nach einem Schweizer PLZ/Ort-Paar im Klartext: 4 Ziffern + Ort.
    if let Some((plz, ort)) = find_swiss_plz_ort(text) {
        push_medium(&mut out, FieldKey::FirmaPlz, &plz, &url);
        push_medium(&mut out, FieldKey::FirmaOrt, &ort, &url);
    }

    out
}

fn extract_title(html: &str) -> Option<String> {
    let lower = html.to_ascii_lowercase();
    let start = lower.find("<title")?;
    let after = &html[start..];
    let gt = after.find('>')?;
    let rest = &after[gt + 1..];
    let end = rest.to_ascii_lowercase().find("</title>")?;
    Some(rest[..end].trim().to_string())
}

fn find_swiss_plz_ort(text: &str) -> Option<(String, String)> {
    // Sehr konservativ: erste Sequenz `\b\d{4}\s+<Ort>` mit Ort beginnend
    // mit Großbuchstabe und max. 3 Worten. Vermeidet Umsatz-Zahlen.
    let bytes = text.as_bytes();
    let mut i = 0;
    while i + 5 < bytes.len() {
        let window = &bytes[i..i + 4];
        if window.iter().all(|b| b.is_ascii_digit()) && bytes[i + 4] == b' ' {
            // CH-PLZ liegt zwischen 1000 und 9999.
            let leading = bytes[i];
            if leading >= b'1' && leading <= b'9' {
                // Boundary: davor sollte kein anderes Digit/Letter stehen.
                let prev_ok = i == 0 || !bytes[i - 1].is_ascii_alphanumeric();
                if prev_ok {
                    let plz = std::str::from_utf8(&bytes[i..i + 4]).ok()?.to_string();
                    let rest = &text[i + 5..];
                    let ort = take_ort_words(rest, 3);
                    if !ort.is_empty() {
                        return Some((plz, ort));
                    }
                }
            }
        }
        i += 1;
    }
    None
}

fn take_ort_words(rest: &str, max_words: usize) -> String {
    let mut words = Vec::new();
    for word in rest.split_whitespace() {
        // Truncate at the first non-name character (`<`, `,`, etc.) and
        // strip surrounding punctuation. Keeps `Zürich` clean even if the
        // raw token is `Zürich</p></body></html>`.
        let clean: String = word
            .chars()
            .take_while(|c| c.is_alphabetic() || *c == '-' || *c == '.' || *c == '\'')
            .collect();
        let clean = clean.trim_matches(|c: char| c.is_ascii_punctuation());
        if clean.is_empty() {
            break;
        }
        if !starts_with_uppercase(clean) {
            break;
        }
        words.push(clean.to_string());
        if words.len() >= max_words {
            break;
        }
    }
    words.join(" ")
}

// ---------------------------------------------------------------------------
// Field-evidence helpers
// ---------------------------------------------------------------------------

fn push_high(out: &mut Vec<(FieldKey, FieldEvidence)>, key: FieldKey, value: &str, url: &str) {
    push(out, key, value, url, Confidence::High);
}

fn push_medium(out: &mut Vec<(FieldKey, FieldEvidence)>, key: FieldKey, value: &str, url: &str) {
    push(out, key, value, url, Confidence::Medium);
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

static MODULE: Zefix = Zefix;

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
        include_str!("../../fixtures/sources/zefix/search_roche.json");
    const DETAIL_FIXTURE: &str =
        include_str!("../../fixtures/sources/zefix/detail_roche.json");

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
        assert_eq!(m.id(), "zefix.ch");
        assert_eq!(m.aliases(), &["zefix"]);
        assert!(matches!(m.tier(), Tier::P));
        assert_eq!(m.countries(), &[Country::Ch]);
        assert!(m.requires_credential().is_none());
        assert!(m
            .authoritative_for()
            .iter()
            .any(|k| matches!(k, FieldKey::FirmaName)));
    }

    #[test]
    fn shape_query_is_none_for_api_source() {
        let ctx = SourceCtx {
            root: Path::new("/tmp/ctox-test"),
            country: Some(Country::Ch),
            mode: ResearchMode::NewRecord,
        };
        assert!(module().shape_query("Roche", &ctx).is_none());
    }

    #[test]
    fn fetch_direct_skips_non_ch_countries() {
        let ctx = SourceCtx {
            root: Path::new("/tmp/ctox-test"),
            country: Some(Country::De),
            mode: ResearchMode::NewRecord,
        };
        let r = module().fetch_direct(&ctx, "Roche Holding AG");
        assert!(r.is_none(), "non-CH must short-circuit to None");
    }

    #[test]
    fn fetch_direct_accepts_unknown_country() {
        // Mode where the orchestrator did not yet bind a country: we WILL
        // attempt the call. We do not actually issue HTTP in this unit
        // test; we only check that the early-return None branch is not
        // taken, by inspecting the shape: `Some(Err(_))` would imply we
        // tried (or were about to). Network errors are tolerated.
        // To stay hermetic, we use a < 3 char company name, which short-
        // circuits to `Some(Err(SourceError::NoMatch))`.
        let ctx = SourceCtx {
            root: Path::new("/tmp/ctox-test"),
            country: None,
            mode: ResearchMode::NewRecord,
        };
        let result = module().fetch_direct(&ctx, "Ro").expect("country=None must engage");
        assert!(matches!(result, Err(SourceError::NoMatch)));
    }

    #[test]
    fn parses_search_fixture_into_hits() {
        let value: Value = serde_json::from_str(SEARCH_FIXTURE).expect("fixture json");
        let hits = parse_search_hits(&value).expect("fixture has hits");
        assert!(!hits.is_empty(), "expected at least one hit");
        // Roche Holding AG should be among the top hits.
        let roche = hits
            .iter()
            .find(|h| h.title == "Roche Holding AG")
            .expect("Roche Holding AG hit");
        assert_eq!(
            roche.url,
            "https://www.zefix.admin.ch/de/search/entity/list/firm/154673"
        );
        assert!(roche.snippet.contains("CHE-101.602.521"));
        assert!(roche.snippet.contains("Basel"));
    }

    #[test]
    fn parses_search_fixture_caps_hits_at_max() {
        let value: Value = serde_json::from_str(SEARCH_FIXTURE).expect("fixture json");
        let hits = parse_search_hits(&value).expect("hits");
        assert!(hits.len() <= MAX_HITS);
    }

    #[test]
    fn empty_search_list_maps_to_no_match() {
        let value: Value = serde_json::from_str(r#"{"list": []}"#).unwrap();
        let err = parse_search_hits(&value).unwrap_err();
        assert!(matches!(err, SourceError::NoMatch));
    }

    #[test]
    fn missing_list_field_maps_to_parse_failed() {
        let value: Value = serde_json::from_str(r#"{"error": "boom"}"#).unwrap();
        let err = parse_search_hits(&value).unwrap_err();
        assert!(matches!(err, SourceError::ParseFailed { .. }));
    }

    #[test]
    fn extracts_address_fields_from_detail_json_with_high_confidence() {
        let page = dummy_page(
            DETAIL_FIXTURE,
            "https://www.zefix.admin.ch/de/search/entity/list/firm/154673",
        );
        let fields = module().extract_fields(&page);
        let name = fields
            .iter()
            .find(|(k, _)| matches!(k, FieldKey::FirmaName))
            .expect("firma_name");
        assert_eq!(name.1.value, "Roche Holding AG");
        assert!(matches!(name.1.confidence, Confidence::High));

        let anschrift = fields
            .iter()
            .find(|(k, _)| matches!(k, FieldKey::FirmaAnschrift))
            .expect("firma_anschrift");
        assert_eq!(anschrift.1.value, "Grenzacherstr. 124");

        let plz = fields
            .iter()
            .find(|(k, _)| matches!(k, FieldKey::FirmaPlz))
            .expect("firma_plz");
        assert_eq!(plz.1.value, "4058");
        assert!(matches!(plz.1.confidence, Confidence::High));

        let ort = fields
            .iter()
            .find(|(k, _)| matches!(k, FieldKey::FirmaOrt))
            .expect("firma_ort");
        assert_eq!(ort.1.value, "Basel");
    }

    #[test]
    fn extracts_persons_from_shab_pub_messages_as_medium() {
        let page = dummy_page(
            DETAIL_FIXTURE,
            "https://www.zefix.admin.ch/de/search/entity/list/firm/154673",
        );
        let fields = module().extract_fields(&page);
        let vornames: Vec<_> = fields
            .iter()
            .filter(|(k, _)| matches!(k, FieldKey::PersonVorname))
            .map(|(_, ev)| ev.value.clone())
            .collect();
        let nachnames: Vec<_> = fields
            .iter()
            .filter(|(k, _)| matches!(k, FieldKey::PersonNachname))
            .map(|(_, ev)| ev.value.clone())
            .collect();
        let funktionen: Vec<_> = fields
            .iter()
            .filter(|(k, _)| matches!(k, FieldKey::PersonFunktion))
            .map(|(_, ev)| ev.value.clone())
            .collect();

        // Persons in "neu oder mutierend" sections only.
        assert!(
            nachnames.iter().any(|n| n == "Rochet"),
            "expected Rochet (new VR), got: {nachnames:?}"
        );
        assert!(
            vornames.iter().any(|v| v == "Lubomira"),
            "expected Lubomira, got: {vornames:?}"
        );
        assert!(
            nachnames.iter().any(|n| n == "Eschli"),
            "expected Eschli, got: {nachnames:?}"
        );
        // Eschli's first name "Dr. Bruno" must lose the title prefix.
        assert!(
            vornames.iter().any(|v| v == "Bruno"),
            "title prefix not stripped: {vornames:?}"
        );

        // Funktion text should be normalized to the canonical phrase.
        assert!(
            funktionen.iter().any(|f| f == "Mitglied des Verwaltungsrates"),
            "expected VR funktion, got: {funktionen:?}"
        );

        // "Süssmuth-Dyckerhoff" appears only in the "Ausgeschiedene
        // Personen" section, which we deliberately skip.
        assert!(
            !nachnames.iter().any(|n| n == "Süssmuth-Dyckerhoff"),
            "must skip ausgeschiedene Personen, got: {nachnames:?}"
        );

        // All person evidence must carry Medium confidence.
        for (key, ev) in &fields {
            if matches!(
                key,
                FieldKey::PersonVorname | FieldKey::PersonNachname | FieldKey::PersonFunktion
            ) {
                assert!(matches!(ev.confidence, Confidence::Medium));
            }
        }
    }

    #[test]
    fn extract_fields_falls_back_to_html_when_text_is_not_json() {
        let html = "<html><head><title>Beispiel AG - Zefix</title></head>\
                    <body><p>Musterstrasse 1, 8001 Zürich</p></body></html>";
        let page = dummy_page(html, "https://www.zefix.admin.ch/de/firm/1");
        let fields = module().extract_fields(&page);
        let name = fields
            .iter()
            .find(|(k, _)| matches!(k, FieldKey::FirmaName))
            .expect("firma_name from <title>");
        assert_eq!(name.1.value, "Beispiel AG");
        assert!(matches!(name.1.confidence, Confidence::Medium));
        let plz = fields
            .iter()
            .find(|(k, _)| matches!(k, FieldKey::FirmaPlz))
            .expect("firma_plz");
        assert_eq!(plz.1.value, "8001");
        let ort = fields
            .iter()
            .find(|(k, _)| matches!(k, FieldKey::FirmaOrt))
            .expect("firma_ort");
        assert_eq!(ort.1.value, "Zürich");
    }

    #[test]
    fn strip_ft_tags_removes_zefix_markup_and_decodes_apos() {
        let input = "<FT TYPE=\"F\">Roche &apos;Holding&apos; AG</FT>";
        let out = strip_ft_tags(input);
        assert_eq!(out, "Roche 'Holding' AG");
    }

    #[test]
    fn parse_person_clause_rejects_non_person_text() {
        assert!(parse_person_clause("Aktiengesellschaft").is_none());
        assert!(parse_person_clause("Statutenänderung: 10.03.2026").is_none());
    }

    #[test]
    #[ignore = "live network; run with: cargo test -p ctox-web-stack -- --ignored sources::zefix"]
    fn live_search_smoke() {
        let ctx = SourceCtx {
            root: Path::new("/tmp/ctox-test"),
            country: Some(Country::Ch),
            mode: ResearchMode::NewRecord,
        };
        let result = module()
            .fetch_direct(&ctx, "Roche Holding AG")
            .expect("CH context must engage")
            .expect("live search ok");
        assert!(!result.is_empty(), "expected at least one hit");
        let roche = result
            .iter()
            .find(|h| h.title == "Roche Holding AG")
            .expect("Roche Holding AG present");
        assert!(roche.url.starts_with(PROFILE_BASE));
        assert!(roche.snippet.contains("CHE-"));
    }
}
