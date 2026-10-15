//! Runtime-installed scrape targets that participate in typed person research.
//!
//! These targets already have versioned Playwright implementations in the
//! universal-scraping runtime. Registering them here makes the native planner
//! select them for their assigned fields instead of treating them as unrelated
//! adapter-only records.

use super::{Country, FieldKey, ShapedQuery, SourceCtx, SourceModule, Tier};

struct RuntimeTarget {
    id: &'static str,
    aliases: &'static [&'static str],
    target_key: &'static str,
    countries: &'static [Country],
    fields: &'static [FieldKey],
    tier: Tier,
}

impl SourceModule for RuntimeTarget {
    fn id(&self) -> &'static str {
        self.id
    }

    fn aliases(&self) -> &'static [&'static str] {
        self.aliases
    }

    fn scrape_target_key(&self) -> Option<&'static str> {
        Some(self.target_key)
    }

    fn tier(&self) -> Tier {
        self.tier
    }

    fn countries(&self) -> &'static [Country] {
        self.countries
    }

    fn authoritative_for(&self) -> &'static [FieldKey] {
        self.fields
    }

    fn shape_query(&self, query: &str, _ctx: &SourceCtx<'_>) -> Option<ShapedQuery> {
        let query = query.trim();
        if query.is_empty() {
            return None;
        }
        Some(ShapedQuery {
            query: format!("{query} site:{}", self.id),
            domains: vec![self.id.to_string()],
        })
    }
}

static EVI: RuntimeTarget = RuntimeTarget {
    id: "evi.gv.at",
    aliases: &["evi", "evi-at"],
    target_key: "evi-gv-at",
    countries: &[Country::At],
    fields: &[
        FieldKey::FirmaName,
        FieldKey::FirmaFruehereNamen,
        FieldKey::FirmaAktivitaetsstatus,
        FieldKey::FirmaAnschrift,
        FieldKey::FirmaPlz,
        FieldKey::FirmaOrt,
        FieldKey::FirmaLand,
        FieldKey::FirmaGeschaeftstaetigkeit,
        FieldKey::FirmaGeschaeftsfuehrung,
        FieldKey::FirmaProkura,
    ],
    tier: Tier::P,
};

static JUSTIZONLINE: RuntimeTarget = RuntimeTarget {
    id: "justizonline.gv.at",
    aliases: &["justizonline", "justiz-online"],
    target_key: "justizonline-gv-at",
    countries: &[Country::At],
    fields: &[
        FieldKey::FirmaName,
        FieldKey::FirmaFruehereNamen,
        FieldKey::FirmaAktivitaetsstatus,
        FieldKey::FirmaAnschrift,
        FieldKey::FirmaPlz,
        FieldKey::FirmaOrt,
        FieldKey::FirmaLand,
        FieldKey::FirmaGeschaeftstaetigkeit,
        FieldKey::FirmaGeschaeftsfuehrung,
        FieldKey::FirmaProkura,
    ],
    tier: Tier::P,
};

static MAILTESTER: RuntimeTarget = RuntimeTarget {
    id: "mailtester.com",
    aliases: &["mailtester", "email-check"],
    target_key: "mailtester-com",
    countries: &[Country::De, Country::At, Country::Ch],
    fields: &[FieldKey::PersonEmailValidation],
    tier: Tier::S,
};

static SHAB: RuntimeTarget = RuntimeTarget {
    id: "shab.ch",
    aliases: &["shab", "schweizerisches-handelsamtsblatt"],
    target_key: "shab-ch",
    countries: &[Country::Ch],
    fields: &[
        FieldKey::FirmaName,
        FieldKey::FirmaFruehereNamen,
        FieldKey::FirmaAktivitaetsstatus,
        FieldKey::FirmaAnschrift,
        FieldKey::FirmaPlz,
        FieldKey::FirmaOrt,
        FieldKey::FirmaLand,
        FieldKey::FirmaGeschaeftstaetigkeit,
        FieldKey::FirmaGeschaeftsfuehrung,
        FieldKey::FirmaProkura,
    ],
    tier: Tier::P,
};

pub fn evi() -> &'static dyn SourceModule {
    &EVI
}

pub fn justizonline() -> &'static dyn SourceModule {
    &JUSTIZONLINE
}

pub fn mailtester() -> &'static dyn SourceModule {
    &MAILTESTER
}

pub fn shab() -> &'static dyn SourceModule {
    &SHAB
}

#[cfg(test)]
mod tests {
    use super::*;

}
