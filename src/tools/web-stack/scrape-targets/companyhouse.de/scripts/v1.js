// companyhouse.de — prospect.v1 extractor (Phase B initial revision).
//
// Reads CTOX_SCRAPE_INPUT_JSON for the company + country, drives the
// CTOX web stack (`ctox web search` + `ctox web read`) to find a profile
// page, then parses the page HTML for the field set documented in
// `tools/web-stack/src/sources/EXCEL_MATRIX.md`.
//
// Companyhouse exposes two profile types under the bare host root:
//   * Person profiles at `/person/<Vorname>-<Nachname>` — the `<h1>`
//     contains the academic title prefix plus first + last name as one
//     string, e.g. "Dr. Manfred Schneider" or
//     "Prof. Dr.-Ing. Anna Müller". This script peels off the title
//     tokens, splits the remaining name on DE conventions (first name
//     first, last name last, nobility particles attach to surname) and
//     emits `person_titel` / `person_vorname` / `person_nachname` at
//     Confidence::medium — the Excel matrix marks `person_titel` with an
//     asterisk because the title heuristic is regex-based.
//   * Company profiles at `/<Firmenname>-<Ort>` — the `<h1>` is the
//     canonical Handelsregister name. We emit `firma_name` at
//     Confidence::high.
//
// Cloudflare interstitials are detected and skipped per-hit so the
// run is classified `temporary_unreachable` (or `portal_drift` if no
// hit produced any record), not silently failed.
//
// Drift contract: if the selectors below stop matching but a profile
// page loads successfully, this script returns an empty records array.
// `ctox scrape execute --allow-heal` then classifies the run as
// `portal_drift` and enqueues a `universal-scraping` repair task that
// will revise this very file.

"use strict";

const { execFileSync } = require("child_process");

const MAX_HITS = 3;

function readInput() {
  const raw = process.env.CTOX_SCRAPE_INPUT_JSON;
  if (!raw) {
    return { company: "", country: "" };
  }
  try {
    return JSON.parse(raw);
  } catch (err) {
    process.stderr.write("invalid CTOX_SCRAPE_INPUT_JSON: " + err.message + "\n");
    return { company: "", country: "" };
  }
}

function ctoxBin() {
  return process.env.CTOX_BIN || "ctox";
}

function runCtox(args) {
  try {
    const out = execFileSync(ctoxBin(), args, {
      encoding: "utf8",
      stdio: ["ignore", "pipe", "pipe"],
      maxBuffer: 32 * 1024 * 1024,
    });
    return JSON.parse(out);
  } catch (err) {
    // Stay silent on per-hit failures: `classify_outcome` in
    // src/capabilities/scrape.rs runs a substring search for "temporary",
    // "timeout", "429", … on stderr and would misclassify the whole run
    // as temporary_unreachable if one Companyhouse page returned 429 while
    // others succeeded. Fatal-only stderr stays in main().
    return null;
  }
}

function searchHits(company, country) {
  const args = [
    "web",
    "search",
    "--query",
    company,
    "--source",
    "companyhouse.de",
    "--include-sources",
  ];
  if (country) {
    args.push("--country", country);
  }
  const payload = runCtox(args);
  if (!payload || !Array.isArray(payload.results)) {
    return [];
  }
  return payload.results
    .filter((hit) => typeof hit.url === "string" && hit.url.includes("companyhouse.de"))
    .slice(0, MAX_HITS);
}

function readPage(url, country) {
  const args = ["web", "read", "--url", url];
  if (country) {
    args.push("--country", country);
  }
  return runCtox(args);
}

// ---------------------------------------------------------------------------
// URL classification — mirrors src/sources/companyhouse.rs is_person_url /
// is_company_url. Person profiles live under `/person/`, company profiles
// live at the bare host root (`/<Firmenname>-<Ort>`); known non-profile
// segments (login, agb, suche, …) are skipped so we don't hallucinate a
// firma_name from a search-results page heading.
// ---------------------------------------------------------------------------

const NON_PROFILE_SEGMENTS = [
  "/login",
  "/register",
  "/suche",
  "/search",
  "/impressum",
  "/agb",
  "/datenschutz",
  "/faq",
  "/preise",
  "/kontakt",
];

function isPersonUrl(url) {
  const lower = (url || "").toLowerCase();
  return lower.includes("/person/") || lower.includes("/personen/");
}

function isCompanyUrl(url) {
  const lower = (url || "").toLowerCase();
  if (!lower.includes("companyhouse.de")) return false;
  if (isPersonUrl(lower)) return false;
  return !NON_PROFILE_SEGMENTS.some((seg) => lower.includes(seg));
}

// ---------------------------------------------------------------------------
// Cloudflare interstitial heuristic.
//
// Companyhouse fronts the site with Cloudflare and frequently returns either
// the classic "Just a moment…" challenge page or a localized block page
// ("Zugriff … gesperrt"). Both keep the response status at 200, so we can't
// rely on HTTP codes alone — we sniff the body. When matched, the page is
// skipped silently and the executor will classify the whole run as
// `temporary_unreachable` if no other hit produced records.
// ---------------------------------------------------------------------------

function isCloudflareBlock(html) {
  if (!html) return false;
  if (html.includes("Just a moment")) return true;
  if (html.includes("Cloudflare") && html.includes("gesperrt")) return true;
  return false;
}

// ---------------------------------------------------------------------------
// Parsing — mirrors src/sources/companyhouse.rs extract_from_html. Regex is
// intentionally permissive; the unit tests on the Rust side gate the
// selector logic. JS-side drift fixes happen by revising this file.
// ---------------------------------------------------------------------------

function parseHeading(html) {
  const m = html.match(/<h1[^>]*>([\s\S]*?)<\/h1>/i);
  if (!m) return null;
  return m[1].replace(/<[^>]+>/g, "").replace(/\s+/g, " ").trim();
}

// Conservative whitelist of common DE academic / professional titles plus
// the generic "ends in '.'" rule for combos like `Dr.-Ing.` or `Dipl.-Kfm.`
// that the whitelist may not cover.
const TITLE_WHITELIST = new Set([
  "Prof",
  "Dr",
  "Mag",
  "Dipl",
  "Ing",
  "MBA",
  "MSc",
  "MA",
  "BA",
  "LL.M",
  "PhD",
  "DDr",
]);

function isTitleToken(token) {
  if (!token) return false;
  if (token.endsWith(".")) return true;
  return TITLE_WHITELIST.has(token);
}

const NOBILITY_PARTICLES = new Set([
  "von",
  "vom",
  "zu",
  "zur",
  "der",
  "den",
  "de",
  "del",
  "di",
  "van",
  "ten",
  "ter",
]);

function isNobilityParticle(token) {
  return NOBILITY_PARTICLES.has((token || "").toLowerCase());
}

// Walk the remaining name tokens from the right to find where the surname
// starts. The surname is the last token, plus any preceding nobility
// particles ("von", "von der", "zu", "de", "van", "van der", "del", "di").
function surnameStart(tokens) {
  if (tokens.length <= 1) return 0;
  let start = tokens.length - 1;
  while (start > 0 && isNobilityParticle(tokens[start - 1])) {
    start -= 1;
  }
  if (start === 0) {
    // Particles consumed everything — fall back to "last token is surname".
    return tokens.length - 1;
  }
  return start;
}

function parsePersonHeading(heading) {
  const tokens = (heading || "").split(/\s+/).filter(Boolean);
  if (tokens.length === 0) return null;

  let idx = 0;
  const titleParts = [];
  while (idx < tokens.length && isTitleToken(tokens[idx])) {
    titleParts.push(tokens[idx]);
    idx += 1;
  }
  const remaining = tokens.slice(idx);
  if (remaining.length === 0) {
    // Heading is *only* titles — defensive, almost never happens.
    return null;
  }

  const result = { title: null, first: null, last: null };
  if (titleParts.length > 0) {
    result.title = titleParts.join(" ");
  }

  if (remaining.length === 1) {
    result.last = remaining[0];
  } else {
    const split = surnameStart(remaining);
    const firstTokens = remaining.slice(0, split);
    const lastTokens = remaining.slice(split);
    if (firstTokens.length > 0) result.first = firstTokens.join(" ");
    if (lastTokens.length > 0) result.last = lastTokens.join(" ");
  }
  return result;
}

function extractRecords(url, html) {
  const records = [];
  const push = (field, value, confidence, note) => {
    const v = (value || "").trim();
    if (!v) return;
    records.push({
      field,
      value: v,
      confidence,
      source_url: url,
      note,
    });
  };

  if (isCloudflareBlock(html)) {
    // Skip — the executor will pick this up as temporary_unreachable
    // if no other hit succeeds.
    return records;
  }

  const heading = parseHeading(html);
  if (!heading) return records;

  if (isPersonUrl(url)) {
    const parsed = parsePersonHeading(heading);
    if (parsed) {
      if (parsed.title) {
        push(
          "person_titel",
          parsed.title,
          "medium",
          "companyhouse person <h1> title prefix",
        );
      }
      if (parsed.first) {
        push(
          "person_vorname",
          parsed.first,
          "medium",
          "companyhouse person <h1> first name",
        );
      }
      if (parsed.last) {
        push(
          "person_nachname",
          parsed.last,
          "medium",
          "companyhouse person <h1> last name",
        );
      }
    }
  } else if (isCompanyUrl(url)) {
    push("firma_name", heading, "high", "companyhouse company <h1>");
  }
  // Else: neither person nor company path (search hit, status page, …).
  // Empty records → drift loop will classify accordingly.

  return records;
}

// ---------------------------------------------------------------------------
// main
// ---------------------------------------------------------------------------

(async function main() {
  const input = readInput();
  const company = (input.company || "").trim();
  const country = (input.country || "").trim();
  if (!company) {
    process.stdout.write(
      JSON.stringify({
        records: [],
        failure_mode: "portal_drift",
        detail: "CTOX_SCRAPE_INPUT_JSON.company missing",
      }),
    );
    return;
  }

  const hits = searchHits(company, country);
  if (hits.length === 0) {
    process.stdout.write(
      JSON.stringify({
        records: [],
        failure_mode: "temporary_unreachable",
        detail: "ctox web search returned no companyhouse.de hits",
      }),
    );
    return;
  }

  const aggregated = [];
  for (const hit of hits) {
    const page = readPage(hit.url, country);
    if (!page || !page.ok) {
      continue;
    }
    const html =
      page.raw_html_excerpt || page.raw_html || page.page_text_excerpt || "";
    if (!html) continue;
    aggregated.push(...extractRecords(page.url || hit.url, html));
  }

  process.stdout.write(JSON.stringify({ records: aggregated }));
})();
