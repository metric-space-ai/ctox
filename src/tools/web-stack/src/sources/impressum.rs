//! `impressum` — Tier P, DACH first-party legal notices.
//!
//! Unlike portal-backed sources, this source has no fixed website of its own.
//! Its input-driven scrape target discovers the researched company's official
//! domain from the company name (or uses an input URL/domain when available),
//! then extracts evidence from that company's own Impressum.

use super::{Country, FieldKey, ShapedQuery, SourceCtx, SourceModule, Tier};

const ID: &str = "impressum";

struct Impressum;

impl SourceModule for Impressum {
    fn id(&self) -> &'static str {
        ID
    }

    fn scrape_target_key(&self) -> Option<&'static str> {
        Some("impressum")
    }

    fn first_party_evidence(&self) -> bool {
        true
    }

    fn tier(&self) -> Tier {
        Tier::P
    }

    fn countries(&self) -> &'static [Country] {
        &[Country::De, Country::At, Country::Ch]
    }

    fn authoritative_for(&self) -> &'static [FieldKey] {
        &[
            FieldKey::FirmaAnschrift,
            FieldKey::FirmaBesucheranschrift,
            FieldKey::FirmaPostanschrift,
            FieldKey::FirmaPostfach,
            FieldKey::FirmaPlz,
            FieldKey::FirmaOrt,
            FieldKey::FirmaLand,
            FieldKey::FirmaEmail,
            FieldKey::FirmaTelefon,
            FieldKey::FirmaFax,
            FieldKey::FirmaDomain,
            FieldKey::FirmaGeschaeftstaetigkeit,
            FieldKey::FirmaHomepageFactSheet,
            FieldKey::FirmaGeschaeftsfuehrung,
            FieldKey::FirmaProkura,
            FieldKey::PersonVorname,
            FieldKey::PersonNachname,
            FieldKey::PersonTitel,
            FieldKey::PersonFunktion,
        ]
    }

    fn shape_query(&self, query: &str, _ctx: &SourceCtx<'_>) -> Option<ShapedQuery> {
        let query = query.trim();
        if query.is_empty() {
            return None;
        }
        Some(ShapedQuery {
            query: format!("{query} Impressum"),
            domains: Vec::new(),
        })
    }
}

static MODULE: Impressum = Impressum;

pub fn module() -> &'static dyn SourceModule {
    &MODULE
}

#[cfg(test)]
mod tests {
    use super::*;
}
