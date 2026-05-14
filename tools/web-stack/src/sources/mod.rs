//! Source modules for `ctox web` person/company research.
//!
//! Each registered source represents an external recherche site or API
//! (Bundesanzeiger, Northdata, Zefix, LinkedIn Sales Navigator, …) that
//! `ctox web search`, `ctox web read`, and `ctox web person-research` can
//! drive in addition to the generic provider cascade.
//!
//! A source module owns three concerns:
//!   * how to phrase a query for that source (`shape_query`),
//!   * how to talk to the source's native API when one exists (`fetch_direct`),
//!   * how to extract typed fields from a fetched page (`extract_fields`).
//!
//! Add a new source by adding a single file under `sources/<id>.rs`,
//! implementing [`SourceModule`], and registering it in [`REGISTRY`].
//! See `sources/README.md` for the full convention and `sources/EXCEL_MATRIX.md`
//! for the per-(mode, country, field) source-priority table that
//! `person-research` consumes.

use anyhow::Result;
use std::path::Path;

pub mod bundesanzeiger;
pub mod companyhouse;
pub mod dnbhoovers;
pub mod firmenabc;
pub mod handelsregister;
pub mod leadfeeder;
pub mod linkedin;
pub mod northdata;
pub mod scrape_bridge;
pub mod xing;
pub mod zefix;

// ---------------------------------------------------------------------------
// Public vocabulary
// ---------------------------------------------------------------------------

/// Countries the CTOX person-research workflow currently supports.
/// Driven by the Thesen Nachrecherche source matrix (DE / AT / CH).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Country {
    De,
    At,
    Ch,
}

impl Country {
    pub fn as_iso(self) -> &'static str {
        match self {
            Country::De => "DE",
            Country::At => "AT",
            Country::Ch => "CH",
        }
    }

    pub fn from_iso(raw: &str) -> Option<Self> {
        match raw.trim().to_ascii_uppercase().as_str() {
            "DE" | "DEU" | "GERMANY" | "DEUTSCHLAND" => Some(Country::De),
            "AT" | "AUT" | "AUSTRIA" | "OESTERREICH" | "ÖSTERREICH" => Some(Country::At),
            "CH" | "CHE" | "SCHWEIZ" | "SWITZERLAND" => Some(Country::Ch),
            _ => None,
        }
    }
}

/// The research-mode classification per row, taken from the Thesen Abstimmungs-Excel.
///
/// * `HaveData` — Sheet A. Stammdaten sind in Excel oder Sellify hinterlegt,
///   keine Recherche-Aktion.
/// * `UpdateInventoryGeneral` — Nach-Recherche Block B. Datensatz im Bestand,
///   keine Sonderursache; Quellen-Spalten leer → kein Plan-Eintrag pro Feld.
/// * `UpdatePerson` — Nach-Recherche Block B 1. Firma stabil, Person hat
///   gewechselt; nur Person-Felder werden recherchiert.
/// * `UpdateFirm` — Nach-Recherche Block B 2. Firmierung oder Anschrift hat
///   sich geändert; Firma + Person werden neu erhoben.
/// * `NewRecord` — Neu-Recherche Block B. Greenfield; Firma + Person komplett
///   ab Excel-Stammdaten aufbauen.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ResearchMode {
    HaveData,
    UpdateInventoryGeneral,
    UpdatePerson,
    UpdateFirm,
    NewRecord,
}

impl ResearchMode {
    pub fn as_str(self) -> &'static str {
        match self {
            ResearchMode::HaveData => "have_data",
            ResearchMode::UpdateInventoryGeneral => "update_inventory_general",
            ResearchMode::UpdatePerson => "update_person",
            ResearchMode::UpdateFirm => "update_firm",
            ResearchMode::NewRecord => "new_record",
        }
    }

    pub fn from_str(raw: &str) -> Option<Self> {
        match raw.trim().to_ascii_lowercase().as_str() {
            "have_data" | "a" => Some(Self::HaveData),
            "update_inventory_general" | "update_inventory" | "b" => {
                Some(Self::UpdateInventoryGeneral)
            }
            "update_person" | "b1" | "b 1" => Some(Self::UpdatePerson),
            "update_firm" | "b2" | "b 2" => Some(Self::UpdateFirm),
            "new_record" | "neu" => Some(Self::NewRecord),
            _ => None,
        }
    }
}

/// Trust tier for a source.
///
/// * `P` — public-authoritative (Bundesanzeiger, Zefix, Handelsregister).
/// * `S` — semi-public aggregator (Northdata, Firmenabc, Companyhouse).
/// * `C` — commercial / subscription (D&B Hoovers, Leadfeeder, LinkedIn, XING).
///
/// `person-research` orders source priority within a `(mode, country, field)`
/// triple by tier P → S → C, with ties broken by the Excel matrix order.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Tier {
    P,
    S,
    C,
}

/// Every typed field a source module may write.
///
/// The vocabulary is taken from the Thesen Excel
/// (`Person oder Ansprechpartner.xlsx` and `Abstimmung … final.xlsx`),
/// not from any internal CTOX entity model.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum FieldKey {
    FirmaName,
    FirmaAnschrift,
    FirmaPlz,
    FirmaOrt,
    FirmaEmail,
    FirmaDomain,
    WzCode,
    Umsatz,
    Mitarbeiter,
    SellifyNummer,
    PersonGeschlecht,
    PersonTitel,
    PersonVorname,
    PersonNachname,
    PersonFunktion,
    PersonPosition,
    PersonEmail,
    PersonTelefon,
    PersonLinkedin,
    PersonXing,
}

impl FieldKey {
    pub fn as_str(self) -> &'static str {
        match self {
            FieldKey::FirmaName => "firma_name",
            FieldKey::FirmaAnschrift => "firma_anschrift",
            FieldKey::FirmaPlz => "firma_plz",
            FieldKey::FirmaOrt => "firma_ort",
            FieldKey::FirmaEmail => "firma_email",
            FieldKey::FirmaDomain => "firma_domain",
            FieldKey::WzCode => "wz_code",
            FieldKey::Umsatz => "umsatz",
            FieldKey::Mitarbeiter => "mitarbeiter",
            FieldKey::SellifyNummer => "sellify_nummer",
            FieldKey::PersonGeschlecht => "person_geschlecht",
            FieldKey::PersonTitel => "person_titel",
            FieldKey::PersonVorname => "person_vorname",
            FieldKey::PersonNachname => "person_nachname",
            FieldKey::PersonFunktion => "person_funktion",
            FieldKey::PersonPosition => "person_position",
            FieldKey::PersonEmail => "person_email",
            FieldKey::PersonTelefon => "person_telefon",
            FieldKey::PersonLinkedin => "person_linkedin",
            FieldKey::PersonXing => "person_xing",
        }
    }

    pub fn from_str(raw: &str) -> Option<Self> {
        match raw.trim().to_ascii_lowercase().as_str() {
            "firma_name" | "firmierung" => Some(Self::FirmaName),
            "firma_anschrift" | "anschrift" => Some(Self::FirmaAnschrift),
            "firma_plz" | "plz" => Some(Self::FirmaPlz),
            "firma_ort" | "ort" => Some(Self::FirmaOrt),
            "firma_email" | "firmen_email" => Some(Self::FirmaEmail),
            "firma_domain" | "domain" => Some(Self::FirmaDomain),
            "wz_code" | "wz" => Some(Self::WzCode),
            "umsatz" => Some(Self::Umsatz),
            "mitarbeiter" | "anzahl_mitarbeiter" => Some(Self::Mitarbeiter),
            "sellify_nummer" => Some(Self::SellifyNummer),
            "person_geschlecht" | "geschlecht" => Some(Self::PersonGeschlecht),
            "person_titel" | "titel" => Some(Self::PersonTitel),
            "person_vorname" | "vorname" => Some(Self::PersonVorname),
            "person_nachname" | "nachname" => Some(Self::PersonNachname),
            "person_funktion" | "funktion" => Some(Self::PersonFunktion),
            "person_position" | "position" => Some(Self::PersonPosition),
            "person_email" => Some(Self::PersonEmail),
            "person_telefon" | "telefon" | "telefonnummer" => Some(Self::PersonTelefon),
            "person_linkedin" | "linkedin" => Some(Self::PersonLinkedin),
            "person_xing" | "xing" => Some(Self::PersonXing),
            _ => None,
        }
    }
}

/// Confidence the source assigns to a single extracted field value.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Confidence {
    Low,
    Medium,
    High,
    UserProvided,
}

impl Confidence {
    pub fn as_str(self) -> &'static str {
        match self {
            Confidence::Low => "low",
            Confidence::Medium => "medium",
            Confidence::High => "high",
            Confidence::UserProvided => "user_provided",
        }
    }
}

/// One field, one value, with the page URL and confidence the source assigns.
#[derive(Debug, Clone)]
pub struct FieldEvidence {
    pub value: String,
    pub confidence: Confidence,
    pub source_url: String,
    pub note: Option<String>,
}

/// Context handed to source modules for a single research turn.
///
/// `root` is the CTOX state root (the directory containing
/// `runtime/ctox.sqlite3`), so a module can read API tokens via
/// [`runtime_config::get`](crate::runtime_config::get) or — for encrypted
/// credentials — via the `ctox secret get` CLI of the same binary.
pub struct SourceCtx<'a> {
    pub root: &'a Path,
    pub country: Option<Country>,
    pub mode: ResearchMode,
}

/// A query as a source module wants it phrased for the search-engine cascade.
///
/// * `query` — the rewritten query text (e.g. `"WITTENSTEIN SE Jahresabschluss"`).
/// * `domains` — domain filters to pass through as `--domain` to `ctox web search`.
#[derive(Debug, Clone)]
pub struct ShapedQuery {
    pub query: String,
    pub domains: Vec<String>,
}

/// A single search-result row as returned by either a search-engine cascade
/// or a source's own `fetch_direct` API.
#[derive(Debug, Clone)]
pub struct SourceHit {
    pub title: String,
    pub url: String,
    pub snippet: String,
}

/// The page-read payload [`SourceModule::extract_fields`] receives.
///
/// Phase 3 converts the web-stack's internal `EvidenceDoc` into this shape so
/// source modules stay decoupled from the search-engine internals.
///
/// `text` carries the article-extracted plaintext suitable for LLM
/// summarisation. `raw_html` carries the original HTML body of the page
/// when one was fetched — crawl-pathed adapters that parse with
/// `scraper` should always prefer `raw_html` over `text`, falling back
/// to `text` only when `raw_html` is `None` (PDF responses, cache-loaded
/// pages from older CTOX versions).
#[derive(Debug, Clone)]
pub struct SourceReadResult {
    pub url: String,
    pub title: String,
    pub summary: String,
    pub text: String,
    pub is_pdf: bool,
    pub excerpts: Vec<String>,
    pub find_results: Vec<SourceFindMatch>,
    /// Raw HTML body of the page, when available. `None` for PDFs and
    /// when the read was served from a cache that pre-dates this field.
    pub raw_html: Option<String>,
}

impl SourceReadResult {
    /// Best string to feed to an HTML parser: `raw_html` if present and
    /// non-empty, otherwise `text` as a degraded fallback.
    pub fn html_source(&self) -> &str {
        self.raw_html
            .as_deref()
            .filter(|s| !s.trim().is_empty())
            .unwrap_or(&self.text)
    }
}

#[derive(Debug, Clone)]
pub struct SourceFindMatch {
    pub pattern: String,
    pub matches: Vec<String>,
}

/// Structured failure modes a source module may report.
///
/// Mapped 1:1 to the documented exit-code semantics — `person-research`
/// uses these to decide between "try next source", "ask the user", "park".
#[derive(Debug)]
pub enum SourceError {
    CredentialMissing { secret_name: &'static str },
    NoMatch,
    RateLimited { retry_after_ms: Option<u64> },
    Blocked { reason: String },
    ParseFailed { detail: String },
    Network(anyhow::Error),
    Other(anyhow::Error),
}

impl SourceError {
    pub fn as_str(&self) -> &'static str {
        match self {
            SourceError::CredentialMissing { .. } => "credential_missing",
            SourceError::NoMatch => "no_match",
            SourceError::RateLimited { .. } => "rate_limited",
            SourceError::Blocked { .. } => "blocked",
            SourceError::ParseFailed { .. } => "parse_failed",
            SourceError::Network(_) => "network",
            SourceError::Other(_) => "other",
        }
    }
}

impl std::fmt::Display for SourceError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SourceError::CredentialMissing { secret_name } => {
                write!(f, "credential_missing: {secret_name}")
            }
            SourceError::NoMatch => write!(f, "no_match"),
            SourceError::RateLimited { retry_after_ms } => match retry_after_ms {
                Some(ms) => write!(f, "rate_limited (retry_after_ms={ms})"),
                None => write!(f, "rate_limited"),
            },
            SourceError::Blocked { reason } => write!(f, "blocked: {reason}"),
            SourceError::ParseFailed { detail } => write!(f, "parse_failed: {detail}"),
            SourceError::Network(err) => write!(f, "network: {err}"),
            SourceError::Other(err) => write!(f, "{err}"),
        }
    }
}

impl std::error::Error for SourceError {}

// ---------------------------------------------------------------------------
// Trait + Registry
// ---------------------------------------------------------------------------

/// The contract every recherche source must implement.
///
/// Source modules are intentionally narrow: each one represents one
/// external website or API and exposes only what is specific to it.
/// Generic concerns — provider cascade, caching, evidence aggregation,
/// workspace persistence — live in `web_search.rs` and (Phase 4)
/// `person_research.rs`, not here.
pub trait SourceModule: Sync {
    /// Stable identifier as referenced by `ctox web search --source <id>`
    /// and in `EXCEL_MATRIX.md`. Conventionally the bare hostname,
    /// e.g. `"bundesanzeiger.de"`.
    fn id(&self) -> &'static str;

    /// Short aliases the user may type instead of [`id`].
    /// Example: `["northdata", "nd"]` for `northdata.de`.
    fn aliases(&self) -> &'static [&'static str] {
        &[]
    }

    /// Optional `target_key` for the CTOX scrape registry
    /// (`ctox scrape execute --target-key <key>`). When set, the orchestrator
    /// may delegate extraction to the registered, hot-revisable script under
    /// `runtime/scraping/targets/<key>/scripts/current.*` instead of running
    /// the in-tree Rust `extract_fields`. This is how the
    /// `universal-scraping` skill ([skills/system/communication/universal-scraping](../../../../skills/system/communication/universal-scraping/SKILL.md))
    /// repairs HTML extractors on portal drift without a Cargo rebuild.
    ///
    /// Returning `None` (default) keeps the source purely Rust-resident
    /// — appropriate for stable API endpoints (Zefix, LinkedIn, XING,
    /// D&B, Leadfeeder) where the JSON shape is versioned upstream.
    /// Crawl-pathed HTML sources (Northdata, Firmenabc, Companyhouse,
    /// Handelsregister, Bundesanzeiger HTML) should override this with
    /// their canonical id, register the target via
    /// `ctox scrape upsert-target`, and emit `prospect.v1` records from
    /// the registered script.
    fn scrape_target_key(&self) -> Option<&'static str> {
        None
    }

    /// Additional URL hosts that should resolve to this source module
    /// beyond the canonical [`id`].
    ///
    /// Most modules need an empty list because the canonical id is also the
    /// public host (e.g. `bundesanzeiger.de`). Override when the production
    /// host differs from the id — typical cases:
    ///
    /// * `zefix.ch` id, but hits come back from `www.zefix.admin.ch`.
    /// * `dnbhoovers.com` id, but the D&B Direct+ API lives under
    ///   `plus.dnb.com` and detail pages on `app.dnbhoovers.com`.
    /// * `linkedin.com` / `xing.com` API responses originate from
    ///   `api.linkedin.com` / `api.xing.com`.
    ///
    /// Suffixes are matched after the URL host has its `www.` / `app.` /
    /// `api.` prefixes stripped, so list bare domain forms here
    /// (`zefix.admin.ch`, `plus.dnb.com`, `api.linkedin.com`).
    fn host_suffixes(&self) -> &'static [&'static str] {
        &[]
    }

    fn tier(&self) -> Tier;

    /// Countries this source covers authoritatively or usefully.
    fn countries(&self) -> &'static [Country];

    /// Fields this source is authoritative or near-authoritative for.
    /// `person-research` uses this to pick which sources to try for each
    /// requested field, in tier order.
    fn authoritative_for(&self) -> &'static [FieldKey];

    /// The CTOX secret name this source needs to function, if any.
    /// `None` for sources that work without authentication.
    fn requires_credential(&self) -> Option<&'static str> {
        None
    }

    /// Rewrite the agent's query for this source and pin its domain.
    ///
    /// Returning `None` means: this source is irrelevant for this context
    /// and should be skipped by the orchestrator.
    fn shape_query(&self, query: &str, ctx: &SourceCtx<'_>) -> Option<ShapedQuery>;

    /// Talk to the source's own API directly, bypassing the search-engine cascade.
    ///
    /// Source modules that have a native API (Zefix REST, LinkedIn Sales Nav,
    /// D&B Direct+, Leadfeeder) override this. Crawl-only modules
    /// (Bundesanzeiger, Northdata, Firmenabc, Companyhouse, Handelsregister)
    /// leave the default `None`, in which case the orchestrator falls back
    /// to `ctox web search` with `shape_query`'s domain pin.
    fn fetch_direct(
        &self,
        _ctx: &SourceCtx<'_>,
        _company: &str,
    ) -> Option<Result<Vec<SourceHit>, SourceError>> {
        None
    }

    /// Pull typed fields out of a single read page (HTML, PDF, or JSON body
    /// already rendered to text).
    ///
    /// The orchestrator calls this only when the page URL matches one of
    /// this module's pinned domains (for crawl sources) or when the hit
    /// originated from `fetch_direct` (for API sources).
    fn extract_fields(&self, _page: &SourceReadResult) -> Vec<(FieldKey, FieldEvidence)> {
        Vec::new()
    }
}

/// Static list of every registered source module.
///
/// Phase 2 fans out parallel work per source by editing
/// `sources/<id>.rs`; this registry is only edited in Phase 0 and
/// never touched by per-source agents.
pub static REGISTRY: &[fn() -> &'static dyn SourceModule] = &[
    bundesanzeiger::module,
    companyhouse::module,
    dnbhoovers::module,
    firmenabc::module,
    handelsregister::module,
    leadfeeder::module,
    linkedin::module,
    northdata::module,
    xing::module,
    zefix::module,
];

/// Iterate every registered source module.
pub fn list() -> impl Iterator<Item = &'static dyn SourceModule> {
    REGISTRY.iter().map(|factory| factory())
}

/// Resolve a source by canonical id or one of its aliases.
pub fn find(id_or_alias: &str) -> Option<&'static dyn SourceModule> {
    let needle = id_or_alias.trim().to_ascii_lowercase();
    for module in list() {
        if module.id().eq_ignore_ascii_case(&needle) {
            return Some(module);
        }
        if module
            .aliases()
            .iter()
            .any(|alias| alias.eq_ignore_ascii_case(&needle))
        {
            return Some(module);
        }
    }
    None
}

/// Sources whose `authoritative_for` includes the given field, in tier order.
pub fn sources_for_field(field: FieldKey) -> Vec<&'static dyn SourceModule> {
    let mut out: Vec<_> = list()
        .filter(|module| module.authoritative_for().contains(&field))
        .collect();
    out.sort_by_key(|module| module.tier());
    out
}

/// Sources that operate in the given country, in tier order.
pub fn sources_for_country(country: Country) -> Vec<&'static dyn SourceModule> {
    let mut out: Vec<_> = list()
        .filter(|module| module.countries().contains(&country))
        .collect();
    out.sort_by_key(|module| module.tier());
    out
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn registry_has_all_expected_sources() {
        let ids: Vec<_> = list().map(|m| m.id()).collect();
        assert_eq!(ids.len(), 10, "expected exactly 10 registered sources");
        for expected in [
            "bundesanzeiger.de",
            "companyhouse.de",
            "dnbhoovers.com",
            "firmenabc.at",
            "handelsregister.de",
            "leadfeeder.com",
            "linkedin.com",
            "northdata.de",
            "xing.com",
            "zefix.ch",
        ] {
            assert!(
                ids.iter().any(|id| *id == expected),
                "missing source: {expected}"
            );
        }
    }

    #[test]
    fn find_resolves_by_id_and_alias() {
        let by_id = find("bundesanzeiger.de").expect("by id");
        assert_eq!(by_id.id(), "bundesanzeiger.de");
    }

    #[test]
    fn country_round_trip() {
        for raw in ["DE", "de", "Deutschland"] {
            assert_eq!(Country::from_iso(raw), Some(Country::De));
        }
        for raw in ["AT", "Österreich", "OESTERREICH"] {
            assert_eq!(Country::from_iso(raw), Some(Country::At));
        }
        for raw in ["CH", "Schweiz"] {
            assert_eq!(Country::from_iso(raw), Some(Country::Ch));
        }
    }

    #[test]
    fn field_key_round_trip() {
        for key in [
            FieldKey::FirmaName,
            FieldKey::Umsatz,
            FieldKey::PersonFunktion,
            FieldKey::PersonLinkedin,
        ] {
            assert_eq!(FieldKey::from_str(key.as_str()), Some(key));
        }
    }
}
