//! `person-discovery` — Tier S, DACH-wide, no credential required.
//!
//! Public-snippet adapter that mines decision-maker profile URLs (LinkedIn
//! `/in/...` and XING `/profile/...`) by issuing site-restricted Google
//! queries through the existing web-search cascade.
//!
//! Sister adapter to the Tier-C [`linkedin`](super::linkedin) and
//! [`xing`](super::xing) modules — those rely on credentialed APIs and are
//! opt-in via `--include-private`. This one is always-on and exclusively
//! uses public search-engine snippets, so the underlying profile pages
//! never need to be fetched (they would be blocked by login walls
//! anyway). Evidence comes out of [`SourceModule::extract_from_hits`]
//! instead of the usual `web read` + `extract_fields` path.
//!
//! ## Extracted fields
//!
//! | Field            | Confidence | Source                                            |
//! | ---------------- | ---------- | ------------------------------------------------- |
//! | `person_linkedin`| High       | full LinkedIn `/in/<slug>` URL from a Google hit  |
//! | `person_xing`    | High       | full XING `/profile/<slug>` URL                   |
//! | `person_vorname` | Medium     | left of `"<First Last> - <Role> - <Company>"`     |
//! | `person_nachname`| Medium     | same                                              |
//! | `person_funktion`| Medium     | middle of `"<First Last> - <Role> - <Company>"`   |
//!
//! ## Social-media evaluation (sales-relevance hint)
//!
//! Each extracted person record carries a `note` field tagged
//! `seniority=<bucket>` to help downstream rankers. Buckets are derived
//! from common DE/EN management-role tokens in the role string:
//!
//! * `c_level`  — CEO, CFO, COO, CTO, CSO, CMO, CIO, CHRO, Vorstand,
//!                Geschäftsführer, Gründer, Founder, Managing Director
//! * `senior`   — VP, Direktor, Head of, Bereichsleiter, Leiter,
//!                Senior Manager, Partner
//! * `mid`      — Manager, Lead, Principal, Specialist
//! * `unknown`  — anything else
//!
//! The seniority bucket is purely advisory; it never overrides the
//! agent's own judgement. It just gives the prospect-research skill a
//! ready-to-rank signal alongside the raw role string.

use super::{
    Confidence, Country, FieldEvidence, FieldKey, ShapedQuery, SourceCtx, SourceHit,
    SourceModule, Tier,
};

const ID: &str = "person-discovery";

struct PersonDiscovery;

impl SourceModule for PersonDiscovery {
    fn id(&self) -> &'static str {
        ID
    }

    fn aliases(&self) -> &'static [&'static str] {
        &["google-people", "google-person", "li-xing-discovery", "person-google"]
    }

    fn tier(&self) -> Tier {
        Tier::S
    }

    fn countries(&self) -> &'static [Country] {
        &[Country::De, Country::At, Country::Ch]
    }

    fn authoritative_for(&self) -> &'static [FieldKey] {
        &[
            FieldKey::PersonVorname,
            FieldKey::PersonNachname,
            FieldKey::PersonFunktion,
            FieldKey::PersonLinkedin,
            FieldKey::PersonXing,
        ]
    }

    fn shape_query(&self, query: &str, _ctx: &SourceCtx<'_>) -> Option<ShapedQuery> {
        let trimmed = query.trim();
        if trimmed.is_empty() {
            return None;
        }
        // Inline `site:` path filters force Google to return profile pages
        // (`/in/<slug>`, `/profile/<slug>`) rather than the company /
        // jobs / pages roots that match the bare domain. The role hint
        // biases ranking toward decision-makers — without it Google
        // prefers the company root for a bare company-name query.
        let q = format!(
            "{trimmed} (CEO OR CFO OR COO OR CTO OR Vorstand OR \
             Geschäftsführer OR Gründer OR Founder OR Director OR \
             \"Head of\" OR Leiter OR Bereichsleiter) \
             (site:linkedin.com/in OR site:xing.com/profile)"
        );
        Some(ShapedQuery {
            query: q,
            domains: vec!["linkedin.com".to_string(), "xing.com".to_string()],
        })
    }

    fn extract_from_hits(
        &self,
        _ctx: &SourceCtx<'_>,
        company: &str,
        hits: &[SourceHit],
    ) -> Vec<(FieldKey, FieldEvidence)> {
        let mut out = Vec::new();
        let mut seen_profiles: Vec<String> = Vec::new();
        for hit in hits {
            let host = host_of(&hit.url);
            let (network, field_key) = match host.as_deref() {
                Some(h) if h.ends_with("linkedin.com") => ("linkedin", FieldKey::PersonLinkedin),
                Some(h) if h.ends_with("xing.com") => ("xing", FieldKey::PersonXing),
                _ => continue,
            };

            let profile_path = match network {
                "linkedin" => extract_path_token(&hit.url, "/in/"),
                "xing" => extract_path_token(&hit.url, "/profile/"),
                _ => None,
            };
            let Some(profile_slug) = profile_path else {
                continue;
            };
            // Skip duplicate profile URLs across multiple hits.
            let canonical = canonical_profile_url(network, &profile_slug);
            if seen_profiles.iter().any(|p| *p == canonical) {
                continue;
            }
            seen_profiles.push(canonical.clone());

            let (first, last, role) = parse_title(&hit.title, company);
            let seniority = classify_seniority(role.as_deref().unwrap_or(""));
            let note_base = format!("network={network};seniority={seniority}");

            push(
                &mut out,
                field_key,
                canonical.clone(),
                Confidence::High,
                &canonical,
                Some(&format!("{note_base};profile_url")),
            );
            if let Some(role) = role {
                push(
                    &mut out,
                    FieldKey::PersonFunktion,
                    role,
                    Confidence::Medium,
                    &canonical,
                    Some(&format!("{note_base};google_snippet_title")),
                );
            }
            if let Some(first) = first {
                push(
                    &mut out,
                    FieldKey::PersonVorname,
                    first,
                    Confidence::Medium,
                    &canonical,
                    Some(&format!("{note_base};google_snippet_title")),
                );
            }
            if let Some(last) = last {
                push(
                    &mut out,
                    FieldKey::PersonNachname,
                    last,
                    Confidence::Medium,
                    &canonical,
                    Some(&format!("{note_base};google_snippet_title")),
                );
            }
        }
        out
    }
}

// ---------------------------------------------------------------------------
// Helpers
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

fn host_of(raw: &str) -> Option<String> {
    let parsed = url::Url::parse(raw).ok()?;
    let host = parsed.host_str()?.to_ascii_lowercase();
    let bare = host.strip_prefix("www.").unwrap_or(&host).to_string();
    Some(bare)
}

/// Pull `<token>/<slug>` out of a URL path. Handles trailing slashes,
/// query strings, and locale prefixes like `/de/in/`.
fn extract_path_token(raw: &str, token: &str) -> Option<String> {
    let parsed = url::Url::parse(raw).ok()?;
    let path = parsed.path();
    let idx = path.find(token)?;
    let after = &path[idx + token.len()..];
    let slug = after.split('/').next()?.split('?').next()?.trim();
    if slug.is_empty() {
        return None;
    }
    Some(slug.to_string())
}

fn canonical_profile_url(network: &str, slug: &str) -> String {
    match network {
        "linkedin" => format!("https://www.linkedin.com/in/{slug}/"),
        "xing" => format!("https://www.xing.com/profile/{slug}/"),
        _ => format!("https://{network}/{slug}/"),
    }
}

/// Parse a Google search-result title into (first_name, last_name, role).
///
/// Typical LinkedIn titles:
///   `Attila Dogudan - Founder & CEO - DO & CO AG | LinkedIn`
///   `Edith Hahn-Olsson - Head of Schaden – DONAU Versicherung AG ...`
///   `Anna Maier – CFO bei Beispiel GmbH - LinkedIn`
///
/// Typical XING titles:
///   `Anna Maier - Geschäftsführerin bei Beispiel GmbH - XING`
///   `Anna Maier | XING`
///
/// We tolerate `-`, `–`, `—`, and `|` as separators, normalize them, and
/// keep tokens between the network suffix and the first separator as the
/// person name. The role is the middle slice up to (but not including)
/// the company segment.
fn parse_title(raw: &str, company: &str) -> (Option<String>, Option<String>, Option<String>) {
    let normalized = raw
        .replace('\u{2013}', "-") // en dash
        .replace('\u{2014}', "-") // em dash
        .replace('|', "-");
    let mut parts: Vec<String> = normalized
        .split(" - ")
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect();
    if parts.is_empty() {
        return (None, None, None);
    }
    // Drop trailing network suffix.
    while let Some(last) = parts.last() {
        let lower = last.to_ascii_lowercase();
        if lower == "linkedin" || lower == "xing" || lower.ends_with(" linkedin") || lower.ends_with(" xing") {
            parts.pop();
            continue;
        }
        break;
    }
    // Drop trailing company segment if it matches.
    if let Some(last) = parts.last() {
        if name_matches_company(last, company) {
            parts.pop();
        }
    }

    let name_segment = parts.first().cloned().unwrap_or_default();
    let role_segment = if parts.len() >= 2 {
        // Join everything between name and the (already-stripped) company /
        // network suffix. For 2-part titles this is just parts[1]; for
        // 3+ parts (rare middle-mgmt with sub-division) it preserves the
        // hierarchical role.
        Some(parts[1..].join(" - "))
    } else {
        // 1-part: try to detect `<First Last> at <Company>` or
        // `<First Last> bei <Company>` inside name_segment.
        split_role_from_inline(&name_segment, company).map(|s| s.1.to_string())
    };

    let role_segment = role_segment.map(|r| strip_company_inline(&r, company));

    let (first, last) = split_name(&name_segment);
    let role = role_segment
        .map(|r| strip_trailing_bei_company(&r, company))
        .filter(|r| !r.trim().is_empty());

    (first, last, role)
}

fn split_name(raw: &str) -> (Option<String>, Option<String>) {
    let clean = raw
        .trim()
        .trim_end_matches(|c: char| c == ':' || c == ',' || c == ';');
    if clean.is_empty() {
        return (None, None);
    }
    let mut tokens: Vec<&str> = clean.split_whitespace().collect();
    // Drop honorific / academic prefixes.
    while let Some(first) = tokens.first().copied() {
        let lower = first.to_ascii_lowercase();
        if matches!(
            lower.as_str(),
            "dr."
                | "dr"
                | "mag."
                | "mag"
                | "dipl.-ing."
                | "dipl."
                | "ing."
                | "prof."
                | "prof"
                | "ddr."
                | "mag.a"
                | "dr.in"
        ) {
            tokens.remove(0);
        } else {
            break;
        }
    }
    match tokens.len() {
        0 => (None, None),
        1 => (None, Some(tokens[0].to_string())),
        _ => {
            let last = tokens.last().copied().unwrap().to_string();
            let first = tokens[..tokens.len() - 1].join(" ");
            (Some(first), Some(last))
        }
    }
}

fn name_matches_company(candidate: &str, company: &str) -> bool {
    let a = simplify(candidate);
    let b = simplify(company);
    if a.is_empty() || b.is_empty() {
        return false;
    }
    // Near-exact match only. We deliberately do NOT do substring matching
    // here, because titles like "Geschäftsführerin bei Beispiel GmbH"
    // contain the company name but are role+company composites that
    // should be parsed apart later via `strip_company_inline`, not
    // stripped wholesale.
    if a == b {
        return true;
    }
    // Tolerate trivial legal-form drift ("AG" vs "AG & Co. KG"). We accept
    // a match when the candidate equals the company minus / plus a short
    // trailing legal-form suffix.
    const LEGAL_SUFFIXES: &[&str] = &["ag", "gmbh", "se", "kg", "ohg", "ug", "ev"];
    for suffix in LEGAL_SUFFIXES {
        if let Some(stripped) = a.strip_suffix(suffix) {
            if stripped == b {
                return true;
            }
        }
        if let Some(stripped) = b.strip_suffix(suffix) {
            if stripped == a {
                return true;
            }
        }
    }
    false
}

fn simplify(raw: &str) -> String {
    raw.chars()
        .filter(|c| c.is_ascii_alphanumeric())
        .map(|c| c.to_ascii_lowercase())
        .collect()
}

fn split_role_from_inline(raw: &str, _company: &str) -> Option<(String, String)> {
    // Look for " bei " / " at " / " @ " inside a single segment.
    for sep in [" bei ", " at ", " @ "] {
        if let Some(idx) = raw.find(sep) {
            let name = raw[..idx].trim().to_string();
            let role = raw[idx + sep.len()..].trim().to_string();
            if !name.is_empty() && !role.is_empty() {
                return Some((name, role));
            }
        }
    }
    None
}

fn strip_company_inline(role: &str, company: &str) -> String {
    for sep in [" bei ", " at ", " @ "] {
        if let Some(idx) = role.find(sep) {
            let prefix = role[..idx].trim();
            let suffix = role[idx + sep.len()..].trim();
            if name_matches_company(suffix, company) {
                return prefix.to_string();
            }
        }
    }
    role.to_string()
}

fn strip_trailing_bei_company(role: &str, company: &str) -> String {
    let mut out = role.to_string();
    if let Some(idx) = out.to_ascii_lowercase().rfind(" bei ") {
        let suffix = out[idx + " bei ".len()..].trim().to_string();
        if name_matches_company(&suffix, company) {
            out = out[..idx].trim().to_string();
        }
    }
    out
}

fn classify_seniority(role: &str) -> &'static str {
    let lower = role.to_ascii_lowercase();
    const C_LEVEL: &[&str] = &[
        "ceo", "cfo", "coo", "cto", "cso", "cmo", "cio", "chro", "vorstand",
        "geschäftsführer", "geschaeftsfuehrer", "geschäftsführerin",
        "geschaeftsfuehrerin", "gründer", "gruender", "founder", "owner",
        "managing director", "präsident", "praesident", "general manager",
        "managing partner",
    ];
    const SENIOR: &[&str] = &[
        "vp ", "vp,", "vp.", "vice president", "direktor", "director",
        "head of", "bereichsleiter", "bereichsleiterin", "leiter",
        "leiterin", "senior manager", "partner", "prokurist",
        "abteilungsleiter",
    ];
    const MID: &[&str] = &[
        "manager", "lead", "principal", "specialist", "consultant",
        "engineer", "architect",
    ];
    for needle in C_LEVEL {
        if lower.contains(needle) {
            return "c_level";
        }
    }
    for needle in SENIOR {
        if lower.contains(needle) {
            return "senior";
        }
    }
    for needle in MID {
        if lower.contains(needle) {
            return "mid";
        }
    }
    "unknown"
}

// ---------------------------------------------------------------------------
// Registry hook
// ---------------------------------------------------------------------------

static MODULE: PersonDiscovery = PersonDiscovery;

pub fn module() -> &'static dyn SourceModule {
    &MODULE
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sources::ResearchMode;
    use std::path::Path;

    fn ctx_de() -> SourceCtx<'static> {
        SourceCtx {
            root: Path::new(""),
            country: Some(Country::De),
            mode: ResearchMode::NewRecord,
        }
    }

    fn hit(title: &str, url: &str, snippet: &str) -> SourceHit {
        SourceHit {
            title: title.to_string(),
            url: url.to_string(),
            snippet: snippet.to_string(),
        }
    }

    #[test]
    fn module_metadata() {
        let m = module();
        assert_eq!(m.id(), "person-discovery");
        assert_eq!(m.tier(), Tier::S);
        assert!(m.requires_credential().is_none());
        assert_eq!(
            m.countries(),
            &[Country::De, Country::At, Country::Ch]
        );
        let af = m.authoritative_for();
        assert!(af.contains(&FieldKey::PersonLinkedin));
        assert!(af.contains(&FieldKey::PersonXing));
        assert!(af.contains(&FieldKey::PersonFunktion));
        assert!(af.contains(&FieldKey::PersonVorname));
        assert!(af.contains(&FieldKey::PersonNachname));
    }

    #[test]
    fn shape_query_pins_both_networks() {
        let shaped = module()
            .shape_query("DO & Co. AG", &ctx_de())
            .expect("shape");
        assert!(shaped.query.contains("DO & Co. AG"));
        assert!(shaped.query.contains("site:linkedin.com/in"));
        assert!(shaped.query.contains("site:xing.com/profile"));
        // Role-bias keywords are present so Google ranks executive profiles
        // higher than the company root.
        assert!(shaped.query.contains("CEO"));
        assert!(shaped.query.contains("Vorstand"));
        assert!(shaped.domains.iter().any(|d| d == "linkedin.com"));
        assert!(shaped.domains.iter().any(|d| d == "xing.com"));
    }

    #[test]
    fn shape_query_empty_returns_none() {
        assert!(module().shape_query("   ", &ctx_de()).is_none());
    }

    #[test]
    fn extracts_linkedin_profile_from_classic_title() {
        let h = hit(
            "Attila Dogudan - Founder & CEO - DO & CO AG | LinkedIn",
            "https://www.linkedin.com/in/attila-dogudan-1a2b3c/",
            "Wien, Österreich. ...",
        );
        let fields = module().extract_from_hits(&ctx_de(), "DO & Co. AG", &[h]);
        let by_key: std::collections::HashMap<FieldKey, &FieldEvidence> =
            fields.iter().map(|(k, v)| (*k, v)).collect();

        let linkedin = by_key.get(&FieldKey::PersonLinkedin).expect("linkedin");
        assert_eq!(
            linkedin.value,
            "https://www.linkedin.com/in/attila-dogudan-1a2b3c/"
        );
        assert_eq!(linkedin.confidence, Confidence::High);

        let funktion = by_key.get(&FieldKey::PersonFunktion).expect("funktion");
        assert_eq!(funktion.value, "Founder & CEO");
        assert!(funktion
            .note
            .as_deref()
            .unwrap()
            .contains("seniority=c_level"));

        let vorname = by_key.get(&FieldKey::PersonVorname).expect("vorname");
        assert_eq!(vorname.value, "Attila");
        let nachname = by_key.get(&FieldKey::PersonNachname).expect("nachname");
        assert_eq!(nachname.value, "Dogudan");
    }

    #[test]
    fn extracts_xing_profile_with_bei_separator() {
        let h = hit(
            "Anna Maier - Geschäftsführerin bei Beispiel GmbH - XING",
            "https://www.xing.com/profile/Anna_Maier",
            "Profil von Anna Maier auf XING ...",
        );
        let fields = module().extract_from_hits(&ctx_de(), "Beispiel GmbH", &[h]);
        let by_key: std::collections::HashMap<FieldKey, &FieldEvidence> =
            fields.iter().map(|(k, v)| (*k, v)).collect();

        let xing = by_key.get(&FieldKey::PersonXing).expect("xing");
        assert_eq!(xing.value, "https://www.xing.com/profile/Anna_Maier/");
        assert_eq!(xing.confidence, Confidence::High);

        let funktion = by_key.get(&FieldKey::PersonFunktion).expect("funktion");
        // Note: the trailing "- XING" is stripped, "- Beispiel GmbH" is
        // stripped, leaving "Geschäftsführerin bei Beispiel GmbH" which
        // we then trim to drop the "bei <company>" tail.
        assert_eq!(funktion.value, "Geschäftsführerin");
        assert!(funktion
            .note
            .as_deref()
            .unwrap()
            .contains("seniority=c_level"));
    }

    #[test]
    fn dedupes_repeated_profile_urls() {
        let h1 = hit(
            "Anna Maier - CFO - Beispiel GmbH",
            "https://www.linkedin.com/in/anna-maier/",
            "",
        );
        let h2 = hit(
            "Anna Maier - CFO - Beispiel GmbH",
            "https://www.linkedin.com/in/anna-maier/?foo=1",
            "",
        );
        let fields = module().extract_from_hits(&ctx_de(), "Beispiel GmbH", &[h1, h2]);
        // Only one set of person fields emitted.
        let linkedin_count = fields
            .iter()
            .filter(|(k, _)| *k == FieldKey::PersonLinkedin)
            .count();
        assert_eq!(linkedin_count, 1);
    }

    #[test]
    fn skips_non_profile_urls() {
        let h = hit(
            "DO & Co. AG | LinkedIn",
            "https://www.linkedin.com/company/do-co-ag/",
            "",
        );
        let fields = module().extract_from_hits(&ctx_de(), "DO & Co. AG", &[h]);
        assert!(fields.is_empty());
    }

    #[test]
    fn seniority_classification_covers_main_buckets() {
        assert_eq!(classify_seniority("Vorstand Schaden"), "c_level");
        assert_eq!(classify_seniority("Geschäftsführer"), "c_level");
        assert_eq!(classify_seniority("CFO"), "c_level");
        assert_eq!(classify_seniority("Head of Sales"), "senior");
        assert_eq!(classify_seniority("Bereichsleiter Vertrieb"), "senior");
        assert_eq!(classify_seniority("Senior Manager"), "senior");
        assert_eq!(classify_seniority("Project Manager"), "mid");
        assert_eq!(classify_seniority("Werkstudent"), "unknown");
    }

    #[test]
    fn parse_title_handles_em_dash_and_pipe() {
        let (first, last, role) =
            parse_title("Edith Hahn-Olsson – Head of Schaden – DONAU AG | LinkedIn", "DONAU AG");
        assert_eq!(first.as_deref(), Some("Edith"));
        assert_eq!(last.as_deref(), Some("Hahn-Olsson"));
        assert_eq!(role.as_deref(), Some("Head of Schaden"));
    }

    #[test]
    fn extract_path_token_handles_trailing_slash_and_query() {
        assert_eq!(
            extract_path_token("https://www.linkedin.com/in/anna/", "/in/").as_deref(),
            Some("anna")
        );
        assert_eq!(
            extract_path_token(
                "https://www.linkedin.com/in/anna-mueller?utm_source=share",
                "/in/"
            )
            .as_deref(),
            Some("anna-mueller")
        );
        assert_eq!(
            extract_path_token("https://www.linkedin.com/company/do-co", "/in/"),
            None
        );
    }
}
