//! Browser/search-backed research sources without a stable public API.
//!
//! Their parsers live in versioned scrape targets so portal drift can be
//! repaired without rebuilding CTOX. Registering them here makes
//! `ctox web search --source <id>` source-aware and prevents an unknown-source
//! request from silently degrading into unscoped web results.

use super::{Country, FieldKey, ShapedQuery, SourceCtx, SourceModule, Tier};

fn shaped(query: &str, domains: &[&str]) -> Option<ShapedQuery> {
    let query = query.trim();
    if query.is_empty() {
        return None;
    }
    Some(ShapedQuery {
        query: if domains.is_empty() {
            query.to_string()
        } else {
            format!(
                "{} {}",
                query,
                domains
                    .iter()
                    .map(|domain| format!("site:{domain}"))
                    .collect::<Vec<_>>()
                    .join(" OR ")
            )
        },
        domains: domains.iter().map(|domain| (*domain).to_string()).collect(),
    })
}

struct Google;

impl SourceModule for Google {
    fn id(&self) -> &'static str {
        "google.de"
    }
    fn aliases(&self) -> &'static [&'static str] {
        &["google"]
    }
    fn scrape_target_key(&self) -> Option<&'static str> {
        Some("google-de")
    }
    fn tier(&self) -> Tier {
        Tier::S
    }
    fn countries(&self) -> &'static [Country] {
        &[Country::De, Country::At, Country::Ch]
    }
    fn authoritative_for(&self) -> &'static [FieldKey] {
        &[FieldKey::FirmaDomain]
    }
    fn shape_query(&self, query: &str, _ctx: &SourceCtx<'_>) -> Option<ShapedQuery> {
        shaped(query, &[])
    }
}

struct GoogleMaps;

impl SourceModule for GoogleMaps {
    fn id(&self) -> &'static str {
        "maps.google.com"
    }
    fn aliases(&self) -> &'static [&'static str] {
        &["google-maps", "maps"]
    }
    fn host_suffixes(&self) -> &'static [&'static str] {
        &["google.de", "google.com"]
    }
    fn scrape_target_key(&self) -> Option<&'static str> {
        Some("maps-google-com")
    }
    fn tier(&self) -> Tier {
        Tier::S
    }
    fn countries(&self) -> &'static [Country] {
        &[Country::De, Country::At, Country::Ch]
    }
    fn authoritative_for(&self) -> &'static [FieldKey] {
        &[
            FieldKey::FirmaAnschrift,
            FieldKey::FirmaPlz,
            FieldKey::FirmaOrt,
            FieldKey::FirmaTelefon,
        ]
    }
    fn shape_query(&self, query: &str, _ctx: &SourceCtx<'_>) -> Option<ShapedQuery> {
        shaped(
            &format!("{query} Google Maps"),
            &["google.com", "google.de"],
        )
    }
}

struct Moneyhouse;

impl SourceModule for Moneyhouse {
    fn id(&self) -> &'static str {
        "moneyhouse.ch"
    }
    fn aliases(&self) -> &'static [&'static str] {
        &["moneyhouse"]
    }
    fn scrape_target_key(&self) -> Option<&'static str> {
        Some("moneyhouse-ch")
    }
    fn tier(&self) -> Tier {
        Tier::S
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
    fn shape_query(&self, query: &str, ctx: &SourceCtx<'_>) -> Option<ShapedQuery> {
        if ctx.country.is_some_and(|country| country != Country::Ch) {
            return None;
        }
        shaped(query, &["moneyhouse.ch"])
    }
}

struct RocketReach;

impl SourceModule for RocketReach {
    fn id(&self) -> &'static str {
        "rocketreach.com"
    }
    fn aliases(&self) -> &'static [&'static str] {
        &["rocketreach"]
    }
    fn scrape_target_key(&self) -> Option<&'static str> {
        Some("rocketreach-com")
    }
    fn tier(&self) -> Tier {
        Tier::C
    }
    fn countries(&self) -> &'static [Country] {
        &[Country::De, Country::At, Country::Ch]
    }
    fn authoritative_for(&self) -> &'static [FieldKey] {
        &[
            FieldKey::PersonVorname,
            FieldKey::PersonNachname,
            FieldKey::PersonPosition,
            FieldKey::PersonEmail,
            FieldKey::PersonTelefon,
        ]
    }
    fn shape_query(&self, query: &str, _ctx: &SourceCtx<'_>) -> Option<ShapedQuery> {
        shaped(query, &["rocketreach.com", "rocketreach.co"])
    }
}

struct Experte;

impl SourceModule for Experte {
    fn id(&self) -> &'static str {
        "experte.de"
    }
    fn aliases(&self) -> &'static [&'static str] {
        &["experte", "email-pruefen"]
    }
    fn scrape_target_key(&self) -> Option<&'static str> {
        Some("experte-de")
    }
    fn tier(&self) -> Tier {
        Tier::S
    }
    fn countries(&self) -> &'static [Country] {
        &[Country::De, Country::At, Country::Ch]
    }
    fn authoritative_for(&self) -> &'static [FieldKey] {
        &[FieldKey::PersonEmailValidation]
    }
    fn shape_query(&self, query: &str, _ctx: &SourceCtx<'_>) -> Option<ShapedQuery> {
        shaped(query, &["experte.de"])
    }
}

static GOOGLE: Google = Google;
static GOOGLE_MAPS: GoogleMaps = GoogleMaps;
static MONEYHOUSE: Moneyhouse = Moneyhouse;
static ROCKETREACH: RocketReach = RocketReach;
static EXPERTE: Experte = Experte;

pub fn google() -> &'static dyn SourceModule {
    &GOOGLE
}
pub fn google_maps() -> &'static dyn SourceModule {
    &GOOGLE_MAPS
}
pub fn moneyhouse() -> &'static dyn SourceModule {
    &MONEYHOUSE
}
pub fn rocketreach() -> &'static dyn SourceModule {
    &ROCKETREACH
}
pub fn experte() -> &'static dyn SourceModule {
    &EXPERTE
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    fn ctx(country: Country) -> SourceCtx<'static> {
        SourceCtx {
            country: Some(country),
            root: Path::new("."),
            mode: super::super::ResearchMode::NewRecord,
        }
    }

    #[test]
    fn directory_sources_pin_their_domains() {
        assert!(moneyhouse()
            .shape_query("Nests Sàrl", &ctx(Country::Ch))
            .unwrap()
            .domains
            .contains(&"moneyhouse.ch".to_string()));
        assert!(rocketreach()
            .shape_query("WITTENSTEIN SE", &ctx(Country::De))
            .unwrap()
            .domains
            .contains(&"rocketreach.com".to_string()));
        assert!(rocketreach()
            .shape_query("WITTENSTEIN SE", &ctx(Country::De))
            .unwrap()
            .domains
            .contains(&"rocketreach.co".to_string()));
        assert!(moneyhouse()
            .shape_query("Nests Sàrl", &ctx(Country::De))
            .is_none());
        assert!(google_maps()
            .authoritative_for()
            .contains(&FieldKey::FirmaTelefon));
        assert_eq!(
            experte().authoritative_for(),
            &[FieldKey::PersonEmailValidation]
        );
    }
}
