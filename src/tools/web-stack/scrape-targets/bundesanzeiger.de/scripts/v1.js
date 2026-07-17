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
const ALLOWED_HOST = "bundesanzeiger.de";
const BROWSER_TIMEOUT_MS = 45_000;

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

function runCtox(args, input) {
  try {
    const out = execFileSync(ctoxBin(), args, {
      encoding: "utf8",
      stdio: [input === undefined ? "ignore" : "pipe", "pipe", "pipe"],
      input,
      timeout: BROWSER_TIMEOUT_MS + 5_000,
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

function allowedSourceUrl(raw) {
  try {
    const url = new URL(raw);
    const host = url.hostname.toLowerCase().replace(/\.$/, "");
    if (url.protocol !== "https:" || url.username || url.password) return null;
    if (host !== ALLOWED_HOST && !host.endsWith(`.${ALLOWED_HOST}`)) return null;
    return url;
  } catch {
    return null;
  }
}

function browserRead(rawUrl) {
  const safeUrl = allowedSourceUrl(rawUrl);
  if (!safeUrl) return null;
  const source = `// ctox-browser: timeout_ms=${BROWSER_TIMEOUT_MS}
const targetUrl = ${JSON.stringify(safeUrl.href)};
const allowedHost = ${JSON.stringify(ALLOWED_HOST)};
await page.goto(targetUrl, { waitUntil: "domcontentloaded", timeout: 25000 });
await page.waitForTimeout(1800);
const finalUrl = page.url();
const parsed = new URL(finalUrl);
const host = parsed.hostname.toLowerCase().replace(/\\.$/, "");
const originOk = parsed.protocol === "https:"
  && !parsed.username && !parsed.password
  && (host === allowedHost || host.endsWith("." + allowedHost));
const title = await page.title();
const text = (await page.locator("body").innerText({ timeout: 5000 }).catch(() => "")).slice(0, 160000);
const html = (await page.content()).slice(0, 500000);
const corpus = (title + " " + text + " " + html.slice(0, 64000)).toLowerCase();
const blocked = /schutzma(?:ß|ss)nahme|to_nlp_start|captcha|verify (?:that )?you are human|access denied|request blocked|just a moment/.test(corpus);
return { url: finalUrl, title, text, html, origin_ok: originOk, blocked };
`;
  const payload = runCtox(
    ["web", "browser-automation", "--timeout-ms", String(BROWSER_TIMEOUT_MS)],
    source,
  );
  const result = payload && payload.ok === true ? payload.result : null;
  if (!result || result.origin_ok !== true || !allowedSourceUrl(result.url)) return null;
  return {
    ok: true,
    url: result.url,
    title: result.title,
    page_text_excerpt: result.text,
    raw_html: result.html,
    blocked: result.blocked === true || /(?:^|[-|:]\s*)(?:login|anmeldung|anmelden|portal)\b/i.test(String(result.title || "")),
    transport: "browser",
  };
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
    .filter((hit) => typeof hit.url === "string" && allowedSourceUrl(hit.url))
    .slice(0, MAX_HITS);
}

function readPage(url, country) {
  const args = ["web", "read", "--url", url];
  if (country) {
    args.push("--country", country);
  }
  return runCtox(args);
}

function pageMatchesCompany(company, page) {
  const title = String(page?.title || "").replace(/\s+/g, " ").trim();
  if (/\b(?:log[ -]?in|sign[ -]?in|anmeld(?:en|ung)|authentication|authentifizierung|kundenportal|customer portal)\b/i.test(title)
      || /^(?:portal|startseite|home|willkommen)(?:\s*[-|:]\s*.*)?$/i.test(title)) {
    return false;
  }
  const legalForms = new Set(["ag", "gmbh", "mbh", "se", "kg", "kgaa", "ohg", "ug", "sa", "sarl"]);
  const tokens = String(company || "").toLocaleLowerCase("de-DE").normalize("NFKD")
    .replace(/\p{M}/gu, "").replace(/[^a-z0-9äöüß]+/gi, " ").split(/\s+/)
    .filter((token) => token.length >= 3 && !legalForms.has(token));
  const corpus = [page?.title, page?.summary, page?.page_text_excerpt, page?.raw_html_excerpt, page?.raw_html]
    .filter(Boolean).join(" ").toLocaleLowerCase("de-DE").normalize("NFKD").replace(/\p{M}/gu, "");
  return tokens.length > 0 && tokens.every((token) => corpus.includes(token));
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
  return /schutzma(?:ß|ss)nahme|to_nlp_start|captcha|verify (?:that )?you are human|access denied|request blocked|just a moment/i.test(html);
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
  let blockedSeen = false;
  let matchingPageSeen = false;
  for (const hit of hits) {
    const safeHit = allowedSourceUrl(hit.url);
    if (!safeHit) continue;
    let page = readPage(safeHit.href, country);
    let html = page && page.ok
      ? page.raw_html_excerpt || page.raw_html || page.page_text_excerpt || ""
      : "";
    if (!page || !page.ok || !html || looksLikeAntiBotWall(html)) {
      if (html && looksLikeAntiBotWall(html)) blockedSeen = true;
      const browserPage = browserRead(safeHit.href);
      if (browserPage) {
        page = browserPage;
        html = page.raw_html || page.page_text_excerpt || "";
        blockedSeen = page.blocked || looksLikeAntiBotWall(html);
      }
    }
    if (!page || !page.ok || !html || page.blocked || looksLikeAntiBotWall(html)) continue;
    const evidenceUrl = allowedSourceUrl(page.url || safeHit.href);
    if (!evidenceUrl) continue;
    if (!pageMatchesCompany(company, page)) continue;
    // PDF Jahresabschluss is owned by the Rust adapter
    // (extract_from_pdf_text). Skip silently so we neither double-emit
    // records nor trigger a portal_drift classification.
    if (page.is_pdf === true) {
      continue;
    }
    matchingPageSeen = true;
    const records = extractRecords(evidenceUrl.href, html);
    const names = records.filter((record) => record.field === "firma_name");
    if (names.length > 0 && !names.some((record) => pageMatchesCompany(company, { raw_html: record.value }))) {
      continue;
    }
    aggregated.push(...records);
  }

  if (aggregated.length > 0) {
    process.stdout.write(JSON.stringify({ records: aggregated }));
    return;
  }
  process.stdout.write(JSON.stringify({
    records: [],
    failure_mode: blockedSeen ? "blocked" : matchingPageSeen ? "portal_drift" : "temporary_unreachable",
    detail: blockedSeen
      ? "bundesanzeiger.de remained blocked after CTOX browser fallback"
      : matchingPageSeen
        ? "company-matching bundesanzeiger.de evidence did not match known result selectors"
        : "bundesanzeiger.de returned no readable evidence for the requested company",
  }));
})();
