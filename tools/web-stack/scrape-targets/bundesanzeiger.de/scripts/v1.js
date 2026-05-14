// bundesanzeiger.de — prospect.v1 extractor (Phase B HTML-path migration).
//
// Hybrid migration: only the HTML search-results path of bundesanzeiger.de
// is owned by this scrape target. The PDF Jahresabschluss path stays in
// Rust (`tools/web-stack/src/sources/bundesanzeiger.rs::extract_from_pdf_text`)
// because the PDF layout is stable and `ctox_pdf_parse` does the heavy
// lifting up-front. This script intentionally skips PDF hits (the Rust
// adapter still runs in parallel via person-research and picks them up).
//
// Reads CTOX_SCRAPE_INPUT_JSON for the company + country, drives the CTOX
// web stack (`ctox web search` + `ctox web read`) to find search-results
// pages on bundesanzeiger.de, then parses the result-row HTML for the
// `firma_name` field. Confidence is Medium because the search hit is the
// summary view; the high-confidence row (Umsatzerlöse, Mitarbeiter, the
// authoritative HGB-§325 company name) lives inside the PDF detail
// document and is owned by the Rust path.
//
// Drift contract: if the selectors below stop matching but a hit page
// loads successfully, this script returns an empty records array.
// `ctox scrape execute --allow-heal` then classifies the run as
// `portal_drift` and enqueues a `universal-scraping` repair task that
// will revise this very file.
//
// Anti-bot: Bundesanzeiger throttles aggressively and serves a JS-only
// Schutzmaßnahme / `to_nlp_start` landing page when it detects a non-
// browser fetch. We detect that explicitly and treat such pages as
// skip (no records emitted) so the executor doesn't mis-classify the
// run as portal_drift — the page loaded, it just wasn't a real result.

"use strict";

const { execFileSync } = require("child_process");

const MAX_HITS = 5;

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
    // as temporary_unreachable if a single bundesanzeiger.de page was
    // throttled while others succeeded. Bundesanzeiger 429s often; only
    // fatal failures bubble to stderr from main().
    return null;
  }
}

function searchHits(query, country) {
  const args = [
    "web",
    "search",
    "--query",
    query,
    "--source",
    "bundesanzeiger.de",
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
    .filter((hit) => typeof hit.url === "string" && hit.url.includes("bundesanzeiger.de"))
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
// Parsing — mirrors the HTML half of src/sources/bundesanzeiger.rs
// (`extract_from_html`). The Rust unit tests gate the row-selector logic
// on a frozen fixture; JS-side drift fixes happen by revising this file
// (universal-scraping repair loop).
// ---------------------------------------------------------------------------

function looksLikeAntiBotWall(html) {
  // The Bundesanzeiger anti-bot interstitial ships either of these tokens
  // in the body (the JS challenge bootstraps via `to_nlp_start(…)`; the
  // user-facing label is "Schutzmaßnahme"). Either presence means we
  // never reached a real results page.
  return html.includes("Schutzmaßnahme") || html.includes("to_nlp_start");
}

function parseFirstHitName(html) {
  // Result rows carry `data-id="…"`; inside each row the company name is
  // rendered in a `.title` block (newer layout) or `.company` block
  // (legacy layout). We do not parse the whole DOM — a permissive regex
  // is enough for the single-field extract this script owns.
  const rowRe = /<([a-z0-9]+)\b[^>]*\bdata-id=\"[^\"]+\"[^>]*>([\s\S]*?)<\/\1>/gi;
  let match;
  while ((match = rowRe.exec(html)) !== null) {
    const inner = match[2];
    const titleMatch = inner.match(
      /<[a-z0-9]+\b[^>]*class=\"[^\"]*\b(?:title|company)\b[^\"]*\"[^>]*>([\s\S]*?)<\//i,
    );
    if (!titleMatch) continue;
    const text = titleMatch[1]
      .replace(/<[^>]+>/g, "")
      .replace(/&nbsp;/g, " ")
      .replace(/&amp;/g, "&")
      .replace(/&#39;/g, "'")
      .replace(/&quot;/g, '"')
      .replace(/\s+/g, " ")
      .trim();
    if (text) {
      return text;
    }
  }
  return null;
}

function extractRecords(url, html) {
  const records = [];
  const name = parseFirstHitName(html);
  if (name) {
    records.push({
      field: "firma_name",
      value: name,
      // Search-hit listing is the summary view, not the authoritative
      // PDF row. Confidence::High would require the Jahresabschluss
      // PDF table — that is owned by the Rust adapter
      // (`extract_from_pdf_text`).
      confidence: "medium",
      source_url: url,
      note: "bundesanzeiger search hit (data-id + .title/.company)",
    });
  }
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

  // Two-stage search:
  //   (1) Prefer PDF Jahresabschluss hits so the Rust PDF adapter has
  //       canonical material to chew on (we won't extract from them
  //       ourselves; the Rust path picks them up in parallel).
  //   (2) Fall back to plain bundesanzeiger.de hits so we at least
  //       get the firma_name from the HTML hit listing.
  const primaryQuery = `${company} Jahresabschluss site:bundesanzeiger.de filetype:pdf`;
  const fallbackQuery = `${company} site:bundesanzeiger.de`;

  let hits = searchHits(primaryQuery, country);
  if (hits.length === 0) {
    hits = searchHits(fallbackQuery, country);
  }
  if (hits.length === 0) {
    process.stdout.write(
      JSON.stringify({
        records: [],
        failure_mode: "temporary_unreachable",
        detail: "ctox web search returned no bundesanzeiger.de hits",
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
    // PDF Jahresabschluss is owned by the Rust adapter
    // (extract_from_pdf_text). Skip silently so we neither double-emit
    // records nor trigger a portal_drift classification.
    if (page.is_pdf === true) {
      continue;
    }
    const html =
      page.raw_html_excerpt || page.raw_html || page.page_text_excerpt || "";
    if (!html) continue;
    if (looksLikeAntiBotWall(html)) {
      // Reached the Schutzmaßnahme / to_nlp_start interstitial — page
      // technically loaded, but it isn't a real results page. Skip.
      continue;
    }
    aggregated.push(...extractRecords(page.url || hit.url, html));
  }

  process.stdout.write(JSON.stringify({ records: aggregated }));
})();
