//! `xing.com` — Tier C, DACH (Schwerpunkt DE).
//!
//! Die produktive Recherche nutzt die authentifizierte XING-Weboberfläche über
//! das reparierbare Scrape-Target `xing-com`. Die aktuelle Mitgliedersuche lebt
//! unter `/search/members`, die Firmensuche unter `/search/companies`; der alte
//! Pfad `/search/people` liefert 404 und darf nicht mehr verwendet werden.
//!
//! Ein vorhandener XING-Partner-API-Token bleibt als optionaler Fast Path
//! nutzbar. Fehlt er, fällt die Quelle auf das Browser-Target zurück, statt eine
//! irreführende `credential_missing`-Meldung für die veraltete API-Integration
//! auszugeben. Die Quelle ist laut [`EXCEL_MATRIX`](./EXCEL_MATRIX.md) autoritativ für
//! `person_funktion` (aktuelle Funktionsbezeichnung aus dem Primärbeschäftiger)
//! und `person_xing` (Permalink auf das XING-Profil) — sowohl im Modus
//! `UpdatePerson` als auch in `NewRecord` für alle drei DACH-Länder.
//!
//! ## Browser-Endpunkte
//!
//! * `GET https://www.xing.com/search/members?keywords=<firma+name>`
//! * `GET https://www.xing.com/search/companies?keywords=<firma+name>`
//!
//! ## Optionaler API-Fallback
//!
//! XING-Partner-Programm (`https://dev.xing.com`), OAuth2 Bearer-Token:
//!
//! * `GET https://api.xing.com/v1/users/find?keywords=<firma+name>&limit=<n>`
//!   Antwort: `{"users": {"items": [User, ...], "total": <n>}}`.
//!   Jeder `User` enthält `id`, `display_name`, `permalink` und (für
//!   Such-Treffer mit Beschäftigungs-Kontext) `professional_experience.primary_company`
//!   mit Feldern `name`, `title`, `company_size`, `industry`.
//! * `GET https://api.xing.com/v1/users/<user_id>`
//!   Antwort: `{"users": [User]}` — ein-Element-Array mit dem vollen
//!   Profil-Body (gleiche Felder wie der Such-Treffer plus
//!   `business_address`, `non_primary_companies`, `wants`, `haves`, …).
//!
//! ## Extrahierte Felder
//!
//! * `person_vorname` / `person_nachname` = verified member-result identity,
//!   `Confidence::Medium` for the browser path.
//! * `person_funktion` = `professional_experience.primary_company.title`,
//!   `Confidence::High` (strukturiertes API-Feld; vom XING-User selbst gepflegt).
//! * `person_xing`     = `permalink`,
//!   `Confidence::High` (kanonische Profil-URL; OAuth-stabil).
//!
//! ## TOS / Browser-Assist
//!
//! Der Browser-Pfad ist operator-initiiert und einwilligungsbasiert
//! ([`browser_recipe`](Xing::browser_recipe), `capture_script =
//! `xing.authenticated_search.v2`): der Operator meldet sich mit **eigenen**
//! Zugangsdaten an. Dieser Pfad ist über denselben Tier-C-Opt-in
//! (`--include-private`) gegated, ist kein automatisches Scraping, trägt aber
//! ToS-/Rechts-Exposition und braucht eine gültige Rechtsgrundlage. Siehe
//! Hardening-Plan WS2-04.

use std::time::Duration;

use anyhow::anyhow;
use serde_json::Value;

use crate::runtime_config;

use super::{
    BrowserSourceRecipe, Confidence, Country, FieldEvidence, FieldKey, ShapedQuery, SourceCtx,
    SourceError, SourceHit, SourceModule, SourceReadResult, Tier,
};

const API_BASE: &str = "https://api.xing.com/v1";
const SECRET_NAME: &str = "XING_API_TOKEN";
const BROWSER_SECRET_NAME: &str = "XING_BROWSER_LOGIN";
const LOGIN_URL: &str = "https://login.xing.com/";
const VERIFY_SELECTOR: &str = "a[href*=\"/profile/\"], [data-testid=\"user-menu\"], a[href*=\"/jobs\"], a[href*=\"/network\"]";
const CREDENTIAL_SELECTOR: &str =
    "input[name=\"password\"], input#password, input[type=\"password\"]";
const CAPTURE_SCRIPT: &str = "xing.authenticated_search.v2";
const MEMBER_SEARCH_URL: &str = "https://www.xing.com/search/members";
const COMPANY_SEARCH_URL: &str = "https://www.xing.com/search/companies";
const MAX_HITS: usize = 8;
const TIMEOUT_MS: u64 = 12_000;
const USER_AGENT: &str = "ctox-web-stack/0.1 (+https://ctox.local)";

/// Builds the XING-specific continuation executed inside an authenticated
/// Browser-App session. Credentials stay inside the generic login boundary;
/// this continuation receives only the research query and country.
/// Member records retain their observed current-employer quote at the canonical
/// profile URL. The query itself is never company evidence; a rendered search
/// response does not establish an exhaustive no-match verdict.
pub fn build_authenticated_browser_capture(company: &str, country: &str) -> anyhow::Result<String> {
    let company = serde_json::to_string(company)?;
    let country = serde_json::to_string(country)?;
    let member_search_url = serde_json::to_string(MEMBER_SEARCH_URL)?;
    let company_search_url = serde_json::to_string(COMPANY_SEARCH_URL)?;

    Ok(XING_BROWSER_CAPTURE_TEMPLATE
        .replace("__COMPANY_JSON__", &company)
        .replace("__COUNTRY_JSON__", &country)
        .replace("__MEMBER_SEARCH_URL_JSON__", &member_search_url)
        .replace("__COMPANY_SEARCH_URL_JSON__", &company_search_url)
        .replace("__RECORD_PARSER__", XING_BROWSER_RECORD_PARSER))
}

const XING_BROWSER_RECORD_PARSER: &str = r#"const parseXingRecords = (companyName, companySearch, memberSearch) => {
  const clean = (value, max = 240) => String(value || "").replace(/\s+/g, " ").trim().slice(0, max);
  const normalize = (value) => clean(value, 2000)
    .toLocaleLowerCase("de-DE")
    .normalize("NFKD")
    .replace(/[\u0300-\u036f]/g, "")
    .replace(/ß/g, "ss")
    .replace(/[^a-z0-9äöüß]+/g, " ")
    .trim();
  const ignoredCompanyTokens = new Set([
    "ag", "se", "gmbh", "kg", "ohg", "gbr", "mbh", "inc", "ltd", "llc",
    "gesellschaft", "aktiengesellschaft", "holding", "group", "gruppe",
  ]);
  const companyTokens = normalize(companyName).split(/\s+/).filter((token) =>
    token.length >= 3 && !ignoredCompanyTokens.has(token)
  );
  const relevantCompanyText = (value) => {
    const haystack = normalize(value);
    const required = companyTokens.slice(0, Math.min(2, companyTokens.length));
    return required.length > 0 && required.every((token) => haystack.includes(token));
  };
  const providerUrl = (raw) => {
    try {
      const url = new URL(raw);
      const host = url.hostname.toLowerCase();
      return host === "xing.com" || host.endsWith(".xing.com") ? url : null;
    } catch { return null; }
  };
  const canonicalProfile = (raw) => {
    const url = providerUrl(raw);
    if (!url) return null;
    const match = url.pathname.match(/^\/profile\/([^/]+)\/?$/i);
    if (!match) return null;
    let slug;
    try { slug = decodeURIComponent(match[1]); } catch { return null; }
    if (!slug || /^(?:contacts?|network)$/i.test(slug)) return null;
    return {
      slug,
      url: `https://www.xing.com/profile/${encodeURIComponent(slug).replace(/%5F/gi, "_").replace(/%2D/gi, "-")}`,
    };
  };
  const validNameToken = (value) => /\p{L}/u.test(value)
    && !/\d/u.test(value)
    && /^[\p{L}][\p{L}.'’\-]*$/u.test(value);
  const nameFromSlug = (slug) => {
    const parts = slug.split("_").map((part) => clean(part)).filter(Boolean);
    if (parts.length < 2 || parts.length > 6) return "";
    parts[parts.length - 1] = parts[parts.length - 1].replace(/\d+$/, "");
    return parts.every(validNameToken) ? parts.join(" ") : "";
  };
  const nameFromVisibleResult = (link) => {
    const candidates = [link.text, ...(link.contextLines || []).slice(0, 4)];
    for (const candidate of candidates) {
      const value = clean(candidate)
        .replace(/^(?:profil von|profile of)\s+/i, "")
        .replace(/\s*[|·]\s*XING\s*$/i, "")
        .replace(/^(?:(?:dr|prof)\.?\s+)+/i, "");
      const parts = value.split(/\s+/).filter(Boolean);
      if (parts.length >= 2 && parts.length <= 5
          && parts.every(validNameToken)
          && !relevantCompanyText(value)
          && !/^(?:profil|profile|kontakt|contacts?|network|xing)\b/i.test(value)) {
        return value;
      }
    }
    return "";
  };
  const formerEmployment = /\b(?:ehemalige[snr]?\s+unternehmen|ehemalig|former\s+(?:company|employer)|previous\s+(?:company|employer))\b/i;
  const currentCompanyIndex = (lines) => {
    const matches = lines.map((line, index) => relevantCompanyText(line) ? index : -1).filter((index) => index >= 0);
    return matches.find((index) => {
      const employmentLabel = lines.slice(Math.max(0, index - 2), index + 1).join(" ");
      return !formerEmployment.test(employmentLabel);
    }) ?? -1;
  };
  const locationLine = (value) => /(?:^|[,\s])(?:deutschland|germany|österreich|austria|schweiz|switzerland)\s*$/i.test(clean(value));
  const ignoredFunctionLine = /^(?:profil|profil ansehen|ganzes profil ansehen|kontakt|vernetzen|nachricht|folgen|basis|premium|xing|aktuelles? unternehmen|derzeitiges unternehmen|current company)\s*:?\s*$/i;
  const contactMetadataLine = (value) => {
    const text = clean(value);
    const contactTerm = /\b(?:kontakte|contacts?|connections?|followers?|gemeinsame[rsn]?\s+kontakt|mutual\s+contacts?)\b/i;
    return contactTerm.test(text) && /\d/.test(text);
  };
  const labelOnlyLine = (value) => /^[^:]{2,80}:\s*$/.test(clean(value));
  const roleSignal = /\b(?:geschäftsführ(?:er|erin|ung)|managing\s+director|chief\s+(?:executive|operating|financial|technology|commercial)\s+officer|ceo|coo|cfo|cto|cmo|vorstand|inhaber(?:in)?|eigentümer(?:in)?|gründer(?:in)?|founder|prokurist(?:in)?|leiter(?:in)?|leitung|head\s+of|director|manager|vertrieb|sales|einkauf|procurement|marketing|produktion|operations|business\s+development|partner)\b/i;
  const personNameKey = (value) => normalize(value)
    .replace(/ae/g, "a")
    .replace(/oe/g, "o")
    .replace(/ue/g, "u");
  const plausibleFunctionLine = (value, personName) => {
    const text = clean(value);
    return text.length >= 2 && text.length <= 160
      && /\p{L}/u.test(text)
      && roleSignal.test(text)
      && personNameKey(text) !== personNameKey(personName)
      && !relevantCompanyText(text)
      && !formerEmployment.test(text)
      && !locationLine(text)
      && !ignoredFunctionLine.test(text)
      && !contactMetadataLine(text)
      && !labelOnlyLine(text);
  };
  const records = [];
  const push = (field, value, confidence, note, url) => {
    const cleanValue = clean(value, 500);
    if (!cleanValue || !providerUrl(url)) return;
    if (field === "person_vorname" && (!validNameToken(cleanValue) || /^\d+$/.test(cleanValue))) return;
    if (records.some((record) => record.field === field && record.value === cleanValue && record.source_url === url)) return;
    records.push({ field, value: cleanValue, confidence, source_url: url, note });
  };
  const providerLinks = (snapshot) => (snapshot?.links || []).filter((link) => providerUrl(link.url));
  const memberNames = new Map(providerLinks(memberSearch).map((link) => {
    const profile = canonicalProfile(link.url);
    return [profile?.url, profile ? nameFromSlug(profile.slug) || nameFromVisibleResult(link) : ""];
  }).filter(([url, name]) => url && name));
  const companyHit = providerLinks(companySearch).find((link) => {
    const url = providerUrl(link.url);
    return url && /^\/(?:pages|companies)\/[^/]+(?:\/.*)?$/i.test(url.pathname)
      && relevantCompanyText(`${link.text || ""} ${(link.contextLines || []).join(" ")}`);
  });
  if (companyHit) {
    const visibleName = (companyHit.contextLines || []).find((line) => relevantCompanyText(line))
      || companyHit.text;
    push("firma_name", visibleName, "high", "XING company search result", companyHit.url);
  }

  const profiles = [];
  for (const link of providerLinks(memberSearch)) {
    const profile = canonicalProfile(link.url);
    if (!profile || profiles.some((entry) => entry.profile.url === profile.url)) continue;
    const name = memberNames.get(profile.url) || "";
    const nameParts = name.split(/\s+/).filter(Boolean);
    if (nameParts.length < 2 || !nameParts.every(validNameToken)) continue;
    const contextLines = (link.contextLines || []).map((line) => clean(line)).filter(Boolean);
    // Never borrow employer/role lines from the next member card.
    const neighborIndex = contextLines.findIndex((line) => Array.from(memberNames.entries())
      .some(([url, otherName]) => url !== profile.url && personNameKey(line) === personNameKey(otherName)));
    const lines = neighborIndex >= 0 ? contextLines.slice(0, neighborIndex) : contextLines;
    const companyIndex = currentCompanyIndex(lines);
    if (companyIndex < 0) continue;
    const employer = lines[companyIndex];
    if (personNameKey(name) === personNameKey(companyName.replace(/\b(?:gmbh|ag|kg|ohg|gbr|ltd)\b/gi, ""))
        && !/\b(?:gmbh|ag|kg|ohg|gbr|ltd|sarl|sàrl)\b/i.test(employer)) continue;
    profiles.push({ link, lines, companyIndex, profile, name, nameParts });
    if (profiles.length >= 8) break;
  }
  // Namen ALLER gefundenen Profile: das Zeilenfenster unten kann in die
  // benachbarte Trefferkarte hineinragen, und deren Personenname darf nie als
  // Funktion durchgehen. Gemessen am 19.08.2026 (ANGUS Chemie): person_funktion
  // von Thorsten Bruns wurde "Dr. Sandra Junghänel" — der Name der naechsten
  // Karte, vom Rollensignal "Dr." als Funktion akzeptiert.
  const alleProfilNamen = new Set(profiles.map((entry) => personNameKey(entry.name)).filter(Boolean));
  const sieht_aus_wie_personenname = (text) => {
    if (alleProfilNamen.has(personNameKey(text))) return true;
    // Titel + zwei grossgeschriebene Woerter ohne Rollenwort dahinter:
    // "Dr. Sandra Junghänel", "Prof. Max Muster".
    return /^(Dr\.|Prof\.|Dipl\.[-\w.]*)\s+\p{Lu}[\p{L}-]+\s+\p{Lu}[\p{L}-]+$/u.test(clean(text));
  };
  for (const { lines, companyIndex, profile, name, nameParts } of profiles) {
    const employerNote = `XING member result current employer: "${lines[companyIndex]}"`;
    push("person_vorname", nameParts[0], "medium", employerNote, profile.url);
    push("person_nachname", nameParts.slice(1).join(" "), "medium", employerNote, profile.url);
    push("person_xing", profile.url, "high", employerNote, profile.url);

    const candidateIndexes = [companyIndex - 1, companyIndex - 2, companyIndex + 1, companyIndex + 2]
      .filter((index) => index >= 0 && index < lines.length);
    const functionLine = candidateIndexes.map((index) => lines[index])
      .find((line) => plausibleFunctionLine(line, name) && !sieht_aus_wie_personenname(line));
    if (functionLine) {
      push("person_funktion", functionLine, "medium", employerNote, profile.url);
    }
  }
  return records;
};"#;

const XING_BROWSER_CAPTURE_TEMPLATE: &str = r#"const sourceId = "xing.com";
const company = __COMPANY_JSON__;
const country = __COUNTRY_JSON__;
const memberSearchUrl = __MEMBER_SEARCH_URL_JSON__;
const companySearchUrl = __COMPANY_SEARCH_URL_JSON__;
__RECORD_PARSER__
const allowedHosts = ["xing.com", "www.xing.com"];
const hostAllowed = (raw) => {
  try {
    const host = new URL(raw).hostname.toLowerCase();
    return allowedHosts.some((allowed) => host === allowed || host.endsWith(`.${allowed}`));
  } catch { return false; }
};
const normalized = (value) => String(value || "")
  .toLocaleLowerCase("de-DE")
  .normalize("NFKD")
  .replace(/[\u0300-\u036f]/g, "")
  .replace(/[^a-z0-9äöüß]+/g, " ")
  .trim();
const companyTokens = normalized(company).split(/\s+/).filter((token) =>
  token.length >= 3 && ![
    "ag", "se", "gmbh", "kg", "ohg", "gbr", "mbh", "inc", "ltd", "llc",
    "gesellschaft", "aktiengesellschaft", "holding", "group", "gruppe",
  ].includes(token)
);
const relevantCompanyText = (value) => {
  const haystack = normalized(value);
  const required = companyTokens.slice(0, Math.min(2, companyTokens.length));
  return required.length > 0 && required.every((token) => haystack.includes(token));
};
const authRequired = async () => {
  if (/login\.xing\.com|\/login(?:[/?#]|$)|\/signin(?:[/?#]|$)/i.test(page.url())) return true;
  return (await page.locator('input[type="password"], form[action*="login" i]').count().catch(() => 0)) > 0;
};
const captureSearch = async (kind, baseUrl) => {
  const targetUrl = `${baseUrl}?keywords=${encodeURIComponent(company)}`;
  let response = null;
  try {
    response = await page.goto(targetUrl, { waitUntil: "domcontentloaded", timeout: 30000 });
  } catch (error) {
    return { kind, status: "navigation_failed", sourceUrl: page.url(), title: "", text: "", links: [] };
  }
  if (await authRequired()) {
    return { kind, status: "auth_required", sourceUrl: page.url(), title: "", text: "", links: [] };
  }
  await page.waitForLoadState("networkidle", { timeout: 12000 }).catch(() => null);
  await page.locator('a[href*="/profile/"], a[href*="/pages/"], a[href*="/companies/"]')
    .first().waitFor({ state: "visible", timeout: 8000 }).catch(() => null);
  await page.waitForTimeout(500);
  const snapshot = await page.evaluate(({ companyName, searchKind }) => {
    const clean = (value, max = 2000) => String(value || "").replace(/\s+/g, " ").trim().slice(0, max);
    const lines = (value) => String(value || "").split(/\n+/).map((line) => clean(line, 240)).filter(Boolean);
    const contextFor = (link) => {
      const identityPath = searchKind === "members" ? /^\/profile\/[^/]+\/?$/i : /^\/(?:pages|companies)\/[^/]+(?:\/.*)?$/i;
      const identityOf = (raw) => {
        try {
          const url = new URL(raw);
          return /^(?:www\.)?xing\.com$/i.test(url.hostname) && identityPath.test(url.pathname)
            ? url.pathname.replace(/\/$/, "").toLowerCase() : null;
        } catch { return null; }
      };
      const ownIdentity = identityOf(link.href);
      if (!ownIdentity) return [];
      let contextLines = [];
      for (let node = link.parentElement, depth = 0; node && depth < 6; node = node.parentElement, depth += 1) {
        const identities = new Set(Array.from(node.querySelectorAll("a[href]"))
          .map((anchor) => identityOf(anchor.href)).filter(Boolean));
        if (identities.size !== 1 || !identities.has(ownIdentity)) break;
        const candidate = lines(node.innerText || node.textContent || "");
        if (candidate.join(" ").length > 2000) break;
        contextLines = candidate;
        if (node.matches('li, article, [role="listitem"], [data-testid*="result" i], [data-testid*="card" i]')) break;
      }
      return contextLines;
    };
    const bodyText = String(document.body?.innerText || "").slice(0, 100000);
    const links = Array.from(document.querySelectorAll("a[href]"))
      .map((link) => ({
        url: link.href,
        text: clean(link.innerText || link.textContent || "", 240),
        contextLines: contextFor(link),
      }))
      .filter((link) => link.url)
      .slice(0, 600);
    return { title: document.title, text: bodyText, links, companyName, searchKind };
  }, { companyName: company, searchKind: kind });
  const httpStatus = response?.status?.() || 0;
  const pageSignal = normalized(`${snapshot.title} ${snapshot.text.slice(0, 5000)}`);
  const blocked = [401, 403, 429].includes(httpStatus)
    || /captcha|ungewohnliche aktivitat|unusual activity|zugriff verweigert|access denied/.test(pageSignal);
  const notFound = httpStatus === 404
    || /seite nicht gefunden|page not found|fehler 404|error 404/.test(pageSignal);
  return {
    ...snapshot,
    kind,
    sourceUrl: page.url(),
    responseUrl: response?.url?.() || "",
    httpStatus,
    requestedUrl: targetUrl,
    status: blocked ? "blocked" : (notFound ? "not_found" : "completed"),
  };
};

if (await authRequired()) {
  return { status: "auth_required", source_url: page.url(), country, records: [] };
}
const companySearch = await captureSearch("companies", companySearchUrl);
if (companySearch.status === "auth_required") {
  return { status: "auth_required", source_url: companySearch.sourceUrl, country, records: [] };
}
const memberSearch = await captureSearch("members", memberSearchUrl);
if (memberSearch.status === "auth_required") {
  return { status: "auth_required", source_url: memberSearch.sourceUrl, country, records: [] };
}
if ([companySearch.status, memberSearch.status].every((status) => status === "blocked")) {
  return { status: "blocked", source_url: memberSearch.sourceUrl, country, records: [] };
}

const queryCompletion = (snapshot) => {
  const matchesQuery = (raw) => {
    try {
      const url = new URL(raw);
      const expected = new URL(snapshot.requestedUrl);
      return hostAllowed(url.href) && url.pathname === expected.pathname
        && url.searchParams.get("keywords") === company;
    } catch { return false; }
  };
  return {
    kind: snapshot.kind,
    query: company,
    requested_url: snapshot.requestedUrl || "",
    response_url: snapshot.responseUrl || "",
    source_url: snapshot.sourceUrl,
    http_status: snapshot.httpStatus || 0,
    response_bound: snapshot.status === "completed" && snapshot.httpStatus === 200
      && matchesQuery(snapshot.responseUrl) && matchesQuery(snapshot.sourceUrl),
    result_completion: "unconfirmed",
  };
};
const query_completion = [companySearch, memberSearch].map(queryCompletion);
const records = parseXingRecords(company,
  query_completion[0].response_bound ? companySearch : null,
  query_completion[1].response_bound ? memberSearch : null);
return {
  // A rendered search response does not prove exhaustive person-query completion.
  status: records.length > 0 ? "succeeded"
    : (query_completion.every((item) => item.response_bound) ? "no_extractable_fields" : "capture_incomplete"),
  source_url: memberSearch.status === "completed" ? memberSearch.sourceUrl : companySearch.sourceUrl,
  country,
  records,
  query_completion,
};"#;

struct Xing;

impl SourceModule for Xing {
    fn id(&self) -> &'static str {
        "xing.com"
    }

    fn aliases(&self) -> &'static [&'static str] {
        &["xing"]
    }

    fn host_suffixes(&self) -> &'static [&'static str] {
        &["api.xing.com"]
    }

    fn scrape_target_key(&self) -> Option<&'static str> {
        Some("xing-com")
    }

    fn tier(&self) -> Tier {
        Tier::C
    }

    fn countries(&self) -> &'static [Country] {
        &[Country::De, Country::At, Country::Ch]
    }

    fn authoritative_for(&self) -> &'static [FieldKey] {
        &[
            // Gender may only be emitted when the profile contains an
            // explicit value; the source must never infer it from names.
            FieldKey::PersonGeschlecht,
            FieldKey::PersonTitel,
            FieldKey::PersonVorname,
            FieldKey::PersonNachname,
            FieldKey::PersonFunktion,
            FieldKey::PersonPosition,
            FieldKey::PersonXing,
        ]
    }

    fn requires_credential(&self) -> Option<&'static str> {
        Some(BROWSER_SECRET_NAME)
    }

    fn browser_recipe(&self) -> Option<BrowserSourceRecipe> {
        Some(BrowserSourceRecipe {
            source_id: self.id(),
            login_url: LOGIN_URL.to_string(),
            allowed_domains: vec![
                "xing.com".to_string(),
                "www.xing.com".to_string(),
                "login.xing.com".to_string(),
                "api.xing.com".to_string(),
            ],
            required_secret_name: Some(BROWSER_SECRET_NAME),
            verify_selector: Some(VERIFY_SELECTOR),
            credential_selector: Some(CREDENTIAL_SELECTOR),
            capture_script: Some(CAPTURE_SCRIPT),
        })
    }

    fn shape_query(&self, query: &str, _ctx: &SourceCtx<'_>) -> Option<ShapedQuery> {
        let query = query.trim();
        if query.is_empty() {
            return None;
        }
        Some(ShapedQuery {
            query: format!("{query} site:xing.com/profile OR site:xing.com/pages"),
            domains: vec!["xing.com".to_string()],
        })
    }

    fn fetch_direct(
        &self,
        ctx: &SourceCtx<'_>,
        company: &str,
    ) -> Option<Result<Vec<SourceHit>, SourceError>> {
        // DACH-only. Andere Länder werden still übersprungen, damit der
        // Orchestrator die Quelle nicht in seine Priority-Liste aufnimmt.
        if matches!(ctx.country, Some(country) if !matches!(country, Country::De | Country::At | Country::Ch))
        {
            return None;
        }

        let trimmed = company.trim();
        if trimmed.is_empty() {
            return Some(Err(SourceError::NoMatch));
        }

        // The member-search API is optional. Browser authentication is the
        // supported default and is executed by the `xing-com` scrape target.
        let token = runtime_config::get(ctx.root, SECRET_NAME)?;

        let agent = build_agent();
        Some(perform_search(&agent, &token, trimmed))
    }

    fn extract_fields(&self, page: &SourceReadResult) -> Vec<(FieldKey, FieldEvidence)> {
        let value = match serde_json::from_str::<Value>(page.text.trim_start()) {
            Ok(v) => v,
            Err(_) => return Vec::new(),
        };
        let user = match pick_user(&value) {
            Some(u) => u,
            None => return Vec::new(),
        };
        extract_from_user(user, &page.url)
    }
}

// ---------------------------------------------------------------------------
// HTTP
// ---------------------------------------------------------------------------

fn build_agent() -> ureq::Agent {
    ureq::AgentBuilder::new()
        .user_agent(USER_AGENT)
        .timeout(Duration::from_millis(TIMEOUT_MS))
        // SSRF guard: a resolved or redirected host must never reach an
        // internal/loopback/metadata address.
        .resolver(crate::egress::SsrfResolver::new(Vec::new()))
        .build()
}

fn perform_search(
    agent: &ureq::Agent,
    token: &str,
    keywords: &str,
) -> Result<Vec<SourceHit>, SourceError> {
    let url = format!("{API_BASE}/users/find");
    let response = agent
        .get(&url)
        .query("keywords", keywords)
        .query("limit", &MAX_HITS.to_string())
        .set("accept", "application/json")
        .set("authorization", &format!("Bearer {token}"))
        .call();

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
    let value: Value = serde_json::from_str(&text).map_err(|err| SourceError::ParseFailed {
        detail: err.to_string(),
    })?;

    parse_search_hits(&value)
}

fn classify_status(status: u16, resp: ureq::Response) -> SourceError {
    match status {
        429 => {
            let retry = resp
                .header("retry-after")
                .and_then(|v| v.parse::<u64>().ok())
                .map(|secs| secs.saturating_mul(1_000));
            SourceError::RateLimited {
                retry_after_ms: retry,
            }
        }
        401 | 403 => SourceError::Blocked {
            reason: format!("http {status}"),
        },
        400 | 404 => SourceError::NoMatch,
        _ => {
            let detail = resp
                .into_string()
                .unwrap_or_else(|_| format!("http {status}"));
            SourceError::Other(anyhow!("xing http {status}: {detail}"))
        }
    }
}

// ---------------------------------------------------------------------------
// Parsing: Search results
// ---------------------------------------------------------------------------

fn parse_search_hits(value: &Value) -> Result<Vec<SourceHit>, SourceError> {
    let items = value
        .get("users")
        .and_then(|users| users.get("items"))
        .and_then(Value::as_array)
        .ok_or_else(|| SourceError::ParseFailed {
            detail: "missing `users.items` array".to_string(),
        })?;
    if items.is_empty() {
        return Err(SourceError::NoMatch);
    }

    let mut hits = Vec::with_capacity(items.len().min(MAX_HITS));
    for entry in items.iter().take(MAX_HITS) {
        if let Some(hit) = user_to_hit(entry) {
            hits.push(hit);
        }
    }
    if hits.is_empty() {
        return Err(SourceError::NoMatch);
    }
    Ok(hits)
}

fn user_to_hit(entry: &Value) -> Option<SourceHit> {
    let permalink = entry.get("permalink").and_then(Value::as_str)?.trim();
    if permalink.is_empty() {
        return None;
    }
    let title = entry
        .get("display_name")
        .and_then(Value::as_str)
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| permalink.to_string());

    let primary = entry
        .get("professional_experience")
        .and_then(|exp| exp.get("primary_company"));
    let company = primary
        .and_then(|c| c.get("name"))
        .and_then(Value::as_str)
        .unwrap_or("")
        .trim();
    let funktion = primary
        .and_then(|c| c.get("title"))
        .and_then(Value::as_str)
        .unwrap_or("")
        .trim();
    let snippet = match (company.is_empty(), funktion.is_empty()) {
        (false, false) => format!("{funktion} · {company}"),
        (false, true) => company.to_string(),
        (true, false) => funktion.to_string(),
        (true, true) => String::new(),
    };

    Some(SourceHit {
        title,
        url: permalink.to_string(),
        snippet,
    })
}

// ---------------------------------------------------------------------------
// Parsing: Detail (User object)
// ---------------------------------------------------------------------------

/// Wählt das User-Objekt aus einem rohen XING-API-Body.
///
/// Akzeptiert sowohl die Detail-Form (`{"users": [user]}`, ein Element)
/// als auch die Such-Form (`{"users": {"items": [user, ...]}}`, dann
/// erstes Item) und schließlich einen "nackten" User-Body.
fn pick_user(value: &Value) -> Option<&Value> {
    if let Some(arr) = value.get("users").and_then(Value::as_array) {
        return arr.first();
    }
    if let Some(items) = value
        .get("users")
        .and_then(|u| u.get("items"))
        .and_then(Value::as_array)
    {
        return items.first();
    }
    // Bare user body fallback: must have permalink to be identifiable.
    if value.get("permalink").and_then(Value::as_str).is_some() {
        return Some(value);
    }
    None
}

fn extract_from_user(user: &Value, source_url: &str) -> Vec<(FieldKey, FieldEvidence)> {
    let mut out = Vec::new();
    // `permalink` ist die kanonische XING-Profil-URL und das einzige
    // Feld, das wir als `person_xing` schreiben dürfen. Fehlt der
    // Permalink, taggen wir die Person-Evidence stattdessen mit der
    // Load-URL — aber emittieren KEIN `person_xing` (das Feld wäre sonst
    // ein API-Endpoint, kein Profil-Link).
    let permalink = user
        .get("permalink")
        .and_then(Value::as_str)
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty());
    let evidence_url = permalink.clone().unwrap_or_else(|| source_url.to_string());

    if let Some(ref link) = permalink {
        push_high(&mut out, FieldKey::PersonXing, link, &evidence_url);
    }

    if let Some(title) = user
        .get("professional_experience")
        .and_then(|exp| exp.get("primary_company"))
        .and_then(|c| c.get("title"))
        .and_then(Value::as_str)
    {
        push_high(&mut out, FieldKey::PersonFunktion, title, &evidence_url);
    }

    out
}

// ---------------------------------------------------------------------------
// Field-evidence helpers
// ---------------------------------------------------------------------------

fn push_high(out: &mut Vec<(FieldKey, FieldEvidence)>, key: FieldKey, value: &str, url: &str) {
    push(out, key, value, url, Confidence::High);
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

static MODULE: Xing = Xing;

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
    use std::process::Command;

    const SEARCH_FIXTURE: &str =
        include_str!("../../fixtures/sources/xing/users_find_example_industrial.json");
    const DETAIL_FIXTURE: &str =
        include_str!("../../fixtures/sources/xing/users_detail_10368_abcdef.json");

    fn dummy_page(text: &str, url: &str) -> SourceReadResult {
        SourceReadResult {
            url: url.to_string(),
            title: String::new(),
            summary: String::new(),
            text: text.to_string(),
            is_pdf: false,
            excerpts: Vec::new(),
            find_results: Vec::new(),
            raw_html: None,
        }
    }

    fn parse_browser_records(member_links: Value) -> Value {
        let company_search = serde_json::json!({
            "sourceUrl": "https://www.xing.com/search/companies?keywords=Example%20Industrial%20GmbH",
            "links": []
        });
        let member_search = serde_json::json!({
            "sourceUrl": "https://www.xing.com/search/members?keywords=Example%20Industrial%20GmbH",
            "links": member_links
        });
        let company = serde_json::to_string("Example Industrial GmbH").expect("company json");
        let script = format!(
            "{}\nconst result = parseXingRecords({}, {}, {});\nprocess.stdout.write(JSON.stringify(result));",
            XING_BROWSER_RECORD_PARSER, company, company_search, member_search
        );
        let output = Command::new("node")
            .args(["--input-type=module", "--eval", &script])
            .output()
            .expect("Node.js is required for authenticated-browser parser tests");
        assert!(
            output.status.success(),
            "XING browser parser failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        serde_json::from_slice(&output.stdout).expect("browser parser result json")
    }

    fn values_for_field<'a>(records: &'a Value, field: &str) -> Vec<&'a str> {
        records
            .as_array()
            .expect("records array")
            .iter()
            .filter(|record| record.get("field").and_then(Value::as_str) == Some(field))
            .filter_map(|record| record.get("value").and_then(Value::as_str))
            .collect()
    }

    #[test]
    fn browser_capture_keeps_profile_bound_evidence_and_response_completion() {
        let test = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/xing-browser-evidence.mjs");
        let output = Command::new("node")
            .arg(test)
            .output()
            .expect("Node.js is required for XING evidence tests");
        assert!(
            output.status.success(),
            "XING evidence regressions failed: {}\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
    }

    #[test]
    fn module_metadata() {
        let m = module();
        assert_eq!(m.id(), "xing.com");
        assert_eq!(m.aliases(), &["xing"]);
        assert!(matches!(m.tier(), Tier::C));
        assert_eq!(m.countries(), &[Country::De, Country::At, Country::Ch]);
        assert_eq!(m.requires_credential(), Some("XING_BROWSER_LOGIN"));
        assert_eq!(m.scrape_target_key(), Some("xing-com"));
        let recipe = m.browser_recipe().expect("browser recipe");
        assert_eq!(recipe.required_secret_name, Some("XING_BROWSER_LOGIN"));
        assert!(m
            .authoritative_for()
            .iter()
            .any(|k| matches!(k, FieldKey::PersonFunktion)));
        assert!(m
            .authoritative_for()
            .iter()
            .any(|k| matches!(k, FieldKey::PersonVorname)));
        assert!(m
            .authoritative_for()
            .iter()
            .any(|k| matches!(k, FieldKey::PersonNachname)));
        assert!(m
            .authoritative_for()
            .iter()
            .any(|k| matches!(k, FieldKey::PersonXing)));
    }

    #[test]
    fn shape_query_falls_back_to_provider_scoped_profile_search() {
        let ctx = SourceCtx {
            root: Path::new("/tmp/ctox-test"),
            country: Some(Country::De),
            mode: ResearchMode::NewRecord,
        };
        let shaped = module()
            .shape_query("Example Industrial GmbH", &ctx)
            .expect("browser-backed source should expose a provider-scoped fallback");
        assert!(shaped.query.contains("site:xing.com/profile"));
        assert_eq!(shaped.domains, vec!["xing.com"]);
    }

    #[test]
    fn browser_capture_uses_current_company_and_member_searches() -> anyhow::Result<()> {
        let source = build_authenticated_browser_capture("Example Industrial GmbH", "DE")?;

        assert!(source.contains("https://www.xing.com/search/companies"));
        assert!(source.contains("https://www.xing.com/search/members"));
        assert!(!source.contains("https://www.xing.com/search/people"));
        assert!(source.contains("companySearch"));
        assert!(source.contains("memberSearch"));
        assert!(source.contains("(?:pages|companies)"));
        assert!(source.contains("^\\/profile\\/([^/]+)\\/?$"));
        assert!(source.contains("auth_required"));
        assert!(source.contains("not_found"));
        assert!(source.contains("blocked"));

        Ok(())
    }

    #[test]
    fn browser_capture_json_quotes_query_and_contains_no_secret_logging() -> anyhow::Result<()> {
        let source = build_authenticated_browser_capture(
            "Example \"Quoted\" AG; return globalThis.secret",
            "DE",
        )?;

        assert!(source
            .contains(r#"const company = "Example \"Quoted\" AG; return globalThis.secret";"#));
        assert!(!source.contains("console."));
        assert!(!source.contains("credentialValue"));
        assert!(!source.contains("localStorage"));
        assert!(!source.contains("sessionStorage"));

        Ok(())
    }

    #[test]
    fn browser_capture_rejects_numeric_names_and_profile_contacts_links() {
        let records = parse_browser_records(serde_json::json!([
            {
                "url": "https://www.xing.com/profile/1/contacts",
                "text": "1",
                "contextLines": ["1", "Example Industrial GmbH", "Harthausen, Deutschland"]
            },
            {
                "url": "https://www.xing.com/profile/2",
                "text": "2",
                "contextLines": ["2", "Example Industrial GmbH", "Harthausen, Deutschland"]
            }
        ]));

        assert!(values_for_field(&records, "person_vorname").is_empty());
        assert!(values_for_field(&records, "person_nachname").is_empty());
        assert!(values_for_field(&records, "person_xing").is_empty());
    }

    #[test]
    fn browser_capture_uses_canonical_profile_slug_and_rejects_location_as_function() {
        let records = parse_browser_records(serde_json::json!([
            {
                "url": "https://www.xing.com/profile/Anna_Schmidt10?sc_o=search_result",
                "text": "1",
                "contextLines": [
                    "1",
                    "Leiterin Einkauf",
                    "Example Industrial GmbH",
                    "Harthausen, Deutschland"
                ]
            },
            {
                "url": "https://www.xing.com/profile/Bernd_Mueller7",
                "text": "2",
                "contextLines": ["2", "Example Industrial GmbH", "Harthausen, Deutschland"]
            }
        ]));

        assert_eq!(
            values_for_field(&records, "person_vorname"),
            ["Anna", "Bernd"]
        );
        assert_eq!(
            values_for_field(&records, "person_nachname"),
            ["Schmidt", "Mueller"]
        );
        assert_eq!(
            values_for_field(&records, "person_xing"),
            [
                "https://www.xing.com/profile/Anna_Schmidt10",
                "https://www.xing.com/profile/Bernd_Mueller7"
            ]
        );
        assert_eq!(
            values_for_field(&records, "person_funktion"),
            ["Leiterin Einkauf"]
        );
        assert!(!values_for_field(&records, "person_funktion").contains(&"Harthausen, Deutschland"));
    }

    #[test]
    fn browser_capture_rejects_former_company_as_current_person_evidence() {
        let records = parse_browser_records(serde_json::json!([
            {
                "url": "https://www.xing.com/profile/Claudia_Hofer3",
                "text": "Claudia Hofer",
                "contextLines": [
                    "Claudia Hofer",
                    "Ehemaliges Unternehmen",
                    "Example Industrial GmbH",
                    "Harthausen, Deutschland"
                ]
            }
        ]));

        for field in [
            "person_vorname",
            "person_nachname",
            "person_funktion",
            "person_xing",
        ] {
            assert!(
                values_for_field(&records, field).is_empty(),
                "former-company result emitted {field}: {records}"
            );
        }
    }

    #[test]
    fn browser_capture_rejects_ui_metadata_and_person_name_as_function() {
        let records = parse_browser_records(serde_json::json!([
            {
                "url": "https://www.xing.com/profile/Nadine_Hehn2",
                "text": "Nadine Hehn",
                "contextLines": [
                    "Nadine Hehn",
                    "Kontakte: 493 davon 1 gemeinsamer Kontakt",
                    "Example Industrial GmbH",
                    "Igersheim, Deutschland"
                ]
            },
            {
                "url": "https://www.xing.com/profile/Malte_Schober2",
                "text": "Malte Schober",
                "contextLines": [
                    "Malte Schober",
                    "Kontakte: 613",
                    "Example Industrial GmbH",
                    "Igersheim, Deutschland"
                ]
            },
            {
                "url": "https://www.xing.com/profile/Ronald_Gleich2",
                "text": "Ronald Gleich",
                "contextLines": [
                    "Ronald Gleich",
                    "Derzeitiges Unternehmen:",
                    "Example Industrial GmbH",
                    "Igersheim, Deutschland"
                ]
            },
            {
                "url": "https://www.xing.com/profile/Michael_Roessler2",
                "text": "Michael Rößler",
                "contextLines": [
                    "Michael Rößler",
                    "Example Industrial GmbH",
                    "Michael Rößler",
                    "Igersheim, Deutschland"
                ]
            }
        ]));

        assert!(
            values_for_field(&records, "person_funktion").is_empty(),
            "UI metadata was emitted as a function: {records}"
        );
        assert_eq!(values_for_field(&records, "person_xing").len(), 4);
    }

    #[test]
    fn browser_capture_skips_metadata_and_keeps_next_plausible_function() {
        let records = parse_browser_records(serde_json::json!([
            {
                "url": "https://www.xing.com/profile/Anna_Schmidt10",
                "text": "Anna Schmidt",
                "contextLines": [
                    "Anna Schmidt",
                    "Leiterin Einkauf",
                    "Kontakte: 284",
                    "Example Industrial GmbH",
                    "Derzeitiges Unternehmen:"
                ]
            }
        ]));

        assert_eq!(
            values_for_field(&records, "person_funktion"),
            ["Leiterin Einkauf"]
        );
    }

    #[test]
    fn browser_capture_rejects_person_names_locations_and_articles_as_functions() {
        let records = parse_browser_records(serde_json::json!([
            {
                "url": "https://www.xing.com/profile/Thorsten_Bruns",
                "text": "Thorsten Bruns",
                "contextLines": [
                    "Thorsten Bruns",
                    "Dr. Sandra Junghänel",
                    "Example Industrial GmbH",
                    "Larnarca, Zypern"
                ]
            },
            {
                "url": "https://www.xing.com/profile/Gunther_Weiss",
                "text": "Gunther Weiß",
                "contextLines": [
                    "Gunther Weiß",
                    "Artikel zur Arbeit von Dr. Gunther K. Weiß",
                    "Example Industrial GmbH",
                    "Nikosia, Zypern"
                ]
            }
        ]));

        assert!(
            values_for_field(&records, "person_funktion").is_empty(),
            "non-role context was emitted as a function: {records}"
        );
        assert_eq!(values_for_field(&records, "person_xing").len(), 2);
    }

    #[test]
    fn fetch_direct_without_api_token_falls_back_to_browser_for_de() {
        let ctx = SourceCtx {
            root: Path::new("/tmp/ctox-test-xing-no-secret"),
            country: Some(Country::De),
            mode: ResearchMode::NewRecord,
        };
        assert!(module()
            .fetch_direct(&ctx, "Example Industrial GmbH")
            .is_none());
    }

    #[test]
    fn fetch_direct_empty_company_short_circuits_no_match() {
        // Even without a token: empty company string must map to
        // `NoMatch` before we ever look at credentials, so the test stays
        // hermetic.
        let ctx = SourceCtx {
            root: Path::new("/tmp/ctox-test-xing-empty"),
            country: Some(Country::De),
            mode: ResearchMode::NewRecord,
        };
        let r = module()
            .fetch_direct(&ctx, "   ")
            .expect("DACH context must engage");
        assert!(matches!(r, Err(SourceError::NoMatch)));
    }

    #[test]
    fn fetch_direct_without_api_token_falls_back_to_browser_for_at_and_ch() {
        for country in [Country::At, Country::Ch] {
            let ctx = SourceCtx {
                root: Path::new("/tmp/ctox-test-xing-no-secret"),
                country: Some(country),
                mode: ResearchMode::NewRecord,
            };
            assert!(module()
                .fetch_direct(&ctx, "Example Industrial GmbH")
                .is_none());
        }
    }

    #[test]
    fn fetch_direct_with_unknown_country_falls_back_to_browser() {
        let ctx = SourceCtx {
            root: Path::new("/tmp/ctox-test-xing-no-secret"),
            country: None,
            mode: ResearchMode::NewRecord,
        };
        assert!(module()
            .fetch_direct(&ctx, "Example Industrial GmbH")
            .is_none());
    }

    #[test]
    fn parses_search_fixture_into_hits() {
        let value: Value = serde_json::from_str(SEARCH_FIXTURE).expect("fixture json");
        let hits = parse_search_hits(&value).expect("fixture has hits");
        assert_eq!(hits.len(), 3, "fixture has three users");
        let anna = hits
            .iter()
            .find(|h| h.url == "https://www.xing.com/profile/Anna_Schmidt10")
            .expect("Anna Schmidt hit");
        assert_eq!(anna.title, "Anna Schmidt");
        assert!(
            anna.snippet.contains("Leiterin Forschung & Entwicklung"),
            "expected funktion in snippet, got: {}",
            anna.snippet
        );
        assert!(
            anna.snippet.contains("Example Industrial GmbH"),
            "expected company in snippet, got: {}",
            anna.snippet
        );
    }

    #[test]
    fn parses_search_fixture_caps_hits_at_max() {
        let value: Value = serde_json::from_str(SEARCH_FIXTURE).expect("fixture json");
        let hits = parse_search_hits(&value).expect("hits");
        assert!(hits.len() <= MAX_HITS);
    }

    #[test]
    fn empty_items_list_maps_to_no_match() {
        let value: Value = serde_json::from_str(r#"{"users": {"items": [], "total": 0}}"#).unwrap();
        let err = parse_search_hits(&value).unwrap_err();
        assert!(matches!(err, SourceError::NoMatch));
    }

    #[test]
    fn missing_items_field_maps_to_parse_failed() {
        let value: Value = serde_json::from_str(r#"{"error": "boom"}"#).unwrap();
        let err = parse_search_hits(&value).unwrap_err();
        assert!(matches!(err, SourceError::ParseFailed { .. }));
    }

    #[test]
    fn extracts_funktion_and_xing_from_detail_json_with_high_confidence() {
        let page = dummy_page(DETAIL_FIXTURE, "https://api.xing.com/v1/users/10368_abcdef");
        let fields = module().extract_fields(&page);

        let funktion = fields
            .iter()
            .find(|(k, _)| matches!(k, FieldKey::PersonFunktion))
            .expect("person_funktion");
        assert_eq!(funktion.1.value, "Leiterin Forschung & Entwicklung");
        assert!(matches!(funktion.1.confidence, Confidence::High));

        let xing = fields
            .iter()
            .find(|(k, _)| matches!(k, FieldKey::PersonXing))
            .expect("person_xing");
        assert_eq!(xing.1.value, "https://www.xing.com/profile/Anna_Schmidt10");
        assert!(matches!(xing.1.confidence, Confidence::High));

        // Both evidence rows must point at the canonical XING permalink,
        // not at the raw API URL — that's what downstream consumers expect.
        for (_, ev) in &fields {
            assert_eq!(
                ev.source_url, "https://www.xing.com/profile/Anna_Schmidt10",
                "evidence source_url must be the permalink, got {}",
                ev.source_url
            );
        }
    }

    #[test]
    fn extracts_from_search_fixture_top_item_as_user() {
        // `pick_user` also accepts the search-result shape and returns the
        // first item. Useful when the orchestrator passes a `users/find`
        // body through `extract_fields` after a fetch_direct hit.
        let page = dummy_page(SEARCH_FIXTURE, "https://api.xing.com/v1/users/find");
        let fields = module().extract_fields(&page);
        let funktion = fields
            .iter()
            .find(|(k, _)| matches!(k, FieldKey::PersonFunktion))
            .expect("person_funktion");
        assert_eq!(funktion.1.value, "Leiterin Forschung & Entwicklung");
        let xing = fields
            .iter()
            .find(|(k, _)| matches!(k, FieldKey::PersonXing))
            .expect("person_xing");
        assert_eq!(xing.1.value, "https://www.xing.com/profile/Anna_Schmidt10");
    }

    #[test]
    fn extract_fields_returns_empty_for_non_json_body() {
        let html = "<html><head><title>XING – Anna Schmidt</title></head><body></body></html>";
        let page = dummy_page(html, "https://www.xing.com/profile/Anna_Schmidt10");
        let fields = module().extract_fields(&page);
        assert!(
            fields.is_empty(),
            "HTML pages are not the API contract; should yield no evidence"
        );
    }

    #[test]
    fn extract_fields_ignores_user_object_without_permalink_or_funktion() {
        // A user body that has neither a permalink nor a primary_company.title
        // must not emit any evidence at all.
        let raw = r#"{"users": [{"id": "x", "display_name": "Anon"}]}"#;
        let page = dummy_page(raw, "https://api.xing.com/v1/users/x");
        let fields = module().extract_fields(&page);
        assert!(fields.is_empty(), "got unexpected fields: {fields:?}");
    }

    #[test]
    fn pick_user_accepts_bare_user_body() {
        let raw = r#"{"id": "bare", "display_name": "Bare User",
                       "permalink": "https://www.xing.com/profile/Bare_User",
                       "professional_experience": {
                         "primary_company": {"name": "X AG", "title": "CEO"}
                       }}"#;
        let value: Value = serde_json::from_str(raw).unwrap();
        let user = pick_user(&value).expect("bare body must be accepted");
        let fields = extract_from_user(user, "https://api.xing.com/v1/users/bare");
        let funktion = fields
            .iter()
            .find(|(k, _)| matches!(k, FieldKey::PersonFunktion))
            .expect("person_funktion");
        assert_eq!(funktion.1.value, "CEO");
    }

    #[test]
    #[ignore = "live network; run with: cargo test -p ctox-web-stack -- --ignored sources::xing"]
    fn live_search_credential_missing_smoke() {
        // Phase 2 live-test: without a configured `XING_API_TOKEN` in the
        // test-only runtime root, the adapter MUST report
        // `CredentialMissing` rather than touching the network or
        // surfacing a generic error. This is the contract person-research
        // relies on to skip the source on tenants that have not yet
        // onboarded the XING partner program.
        let tmp = std::env::temp_dir().join("ctox-web-stack-xing-live-smoke");
        // Ensure the runtime DB at `<root>/runtime/ctox.sqlite3` does NOT
        // exist (or contains no XING token) — using a non-existent dir is
        // the cleanest way: `runtime_config::get` returns `None`.
        let ctx = SourceCtx {
            root: tmp.as_path(),
            country: Some(Country::De),
            mode: ResearchMode::NewRecord,
        };
        let result = module()
            .fetch_direct(&ctx, "Example Industrial GmbH")
            .expect("DACH context must engage");
        assert!(
            matches!(
                result,
                Err(SourceError::CredentialMissing {
                    secret_name: "XING_API_TOKEN"
                })
            ),
            "expected credential_missing, got: {result:?}"
        );
    }
}
