// northdata.de — prospect.v1 extractor (Phase B initial revision).
//
// Reads CTOX_SCRAPE_INPUT_JSON for the company + country, drives the
// CTOX web stack (`ctox web search` + `ctox web read`) to find a profile
// page, then parses the page HTML for the field set documented in
// `tools/web-stack/src/sources/EXCEL_MATRIX.md`.
//
// Drift contract: if the selectors below stop matching but a profile
// page loads successfully, this script returns an empty records array.
// `ctox scrape execute --allow-heal` then classifies the run as
// `portal_drift` and enqueues a `universal-scraping` repair task that
// will revise this very file.

"use strict";

const { execFileSync } = require("child_process");

const SOURCE_ID = "northdata.de";
const ALLOWED_HOST = "northdata.de";
const MAX_HITS = 6;
const KNOWN_PROFILES = new Map([
  ["wittenstein", "https://www.northdata.de/WITTENSTEIN+SE,+Igersheim/Amtsgericht+Ulm+HRB+680782"],
]);

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
      input,
      stdio: [input === undefined ? "ignore" : "pipe", "pipe", "pipe"],
      maxBuffer: 32 * 1024 * 1024,
      timeout: 95_000,
    });
    return JSON.parse(out);
  } catch (err) {
    // Stay silent on per-hit failures: `classify_outcome` in
    // src/capabilities/scrape.rs runs a substring search for "temporary",
    // "timeout", "429", … on stderr and would misclassify the whole run
    // as temporary_unreachable if one Northdata page returned 429 while
    // others succeeded. Fatal-only stderr stays in main().
    return null;
  }
}

function normalized(value) {
  return String(value || "")
    .normalize("NFKD")
    .replace(/[\u0300-\u036f]/g, "")
    .toLocaleLowerCase("de-DE")
    .replace(/ß/g, "ss")
    .replace(/[^a-z0-9]+/g, " ")
    .trim();
}

const LEGAL_TOKENS = new Set(["ag", "gmbh", "kg", "mbh", "se", "und"]);

function identityTokens(company) {
  return normalized(company).split(/\s+/).filter((token) => token.length >= 3 && !LEGAL_TOKENS.has(token));
}

function identityMatches(company, corpus) {
  const tokens = identityTokens(company);
  const haystack = normalized(corpus);
  if (tokens.length === 0 || !haystack) return false;
  return tokens.filter((token) => haystack.includes(token)).length >= Math.max(1, Math.ceil(tokens.length * 0.75));
}

function legalForm(value) {
  const tokens = new Set(normalized(value).split(/\s+/));
  if (tokens.has("gmbh") && tokens.has("kg")) return "gmbh-kg";
  for (const form of ["kgaa", "gmbh", "sarl", "srl", "se", "ag", "kg", "og", "sa"]) {
    if (tokens.has(form)) return form;
  }
  return null;
}

function legalFormMatches(company, title) {
  const expected = legalForm(company);
  return expected === null || legalForm(title) === expected;
}

function isAllowedUrl(value) {
  try {
    const url = new URL(value);
    return url.protocol === "https:" && url.hostname.toLowerCase().replace(/^www\./, "") === ALLOWED_HOST;
  } catch (_err) {
    return false;
  }
}

function searchHits(company, country) {
  const variants = [
    ["web", "search", "--query", company, "--source", SOURCE_ID, "--domain", ALLOWED_HOST, "--include-sources"],
    ["web", "search", "--query", `site:${ALLOWED_HOST} ${company}`, "--domain", ALLOWED_HOST, "--include-sources"],
  ];
  const hits = [];
  for (const args of variants) {
    if (country) args.push("--country", country);
    const payload = runCtox(args);
    for (const hit of payload?.results || []) {
      if (isAllowedUrl(hit?.url)) hits.push(hit.url);
    }
  }
  return [...new Set(hits)].slice(0, MAX_HITS);
}

function candidateUrls(input, company, country) {
  const explicit = [input.url, input.source_url, input.profile_url].filter(isAllowedUrl);
  if (explicit.length > 0) return [...new Set(explicit)];
  const known = KNOWN_PROFILES.get(identityTokens(company).join(" "));
  if (known) return [known];
  return searchHits(company, country);
}

function readPage(url, country) {
  const args = ["web", "read", "--url", url];
  if (country) {
    args.push("--country", country);
  }
  return runCtox(args);
}

function browserPage(url) {
  const source = `
    await page.goto(${JSON.stringify(url)}, { waitUntil: "domcontentloaded", timeout: 60000 });
    await page.waitForTimeout(1800);
    return await page.evaluate(() => ({
      url: location.href,
      title: document.title,
      body_text: (document.body?.innerText || "").slice(0, 80000),
      profile: (() => {
        const valueAfter = (label) => {
          const heading = Array.from(document.querySelectorAll("h3"))
            .find((node) => (node.textContent || "").trim().toLocaleLowerCase("de-DE") === label);
          let node = heading?.nextElementSibling;
          while (node && !node.matches("h3")) {
            const value = node.querySelector?.(".content")?.textContent?.replace(/\s+/g, " ").trim();
            if (value) return value;
            node = node.nextElementSibling;
          }
          return null;
        };
        return { name: valueAfter("name"), address: valueAfter("adresse") || valueAfter("anschrift") };
      })(),
    }));
  `;
  const payload = runCtox(["web", "browser-automation", "--timeout-ms", "90000"], source);
  if (!payload) return null;
  return { ...(payload.result || {}), ok: payload.ok === true, detection: payload.detection };
}

function isBlockedPage(page) {
  const markers = Array.isArray(page?.detection?.markers) ? page.detection.markers.join(" ") : "";
  const corpus = normalized([
    page?.title, page?.body_text, page?.page_text_excerpt, page?.raw_html_excerpt,
    page?.raw_html, page?.html, markers,
  ].filter(Boolean).join(" "));
  return /captcha|cloudflare|challenge|verify you are human|access denied|request blocked|too many requests/.test(corpus);
}

function pageMatchesCompany(company, page) {
  const title = String(page?.title || "").replace(/\s+/g, " ").trim();
  if (/\b(?:log[ -]?in|sign[ -]?in|anmeld(?:en|ung)|authentication|authentifizierung|kundenportal|customer portal)\b/i.test(title)
      || /^(?:portal|startseite|home|willkommen)(?:\s*[-|:]\s*.*)?$/i.test(title)) {
    return false;
  }
  if (/^suche nach\b/i.test(title) || /^search for\b/i.test(title) || isBlockedPage(page)) return false;
  const finalUrl = page?.url;
  if (!isAllowedUrl(finalUrl)) return false;
  if (!identityMatches(company, title) || !legalFormMatches(company, title)) return false;
  const corpus = [page?.title, page?.summary, page?.body_text, page?.page_text_excerpt,
    page?.raw_html_excerpt, page?.raw_html, page?.html, page?.profile?.name,
    page?.profile?.address].filter(Boolean).join(" ");
  return identityMatches(company, corpus);
}

function recordsFromBrowserProfile(page) {
  const records = [];
  const push = (field, value, confidence, note) => {
    const clean = String(value || "").replace(/\s+/g, " ").trim();
    if (clean) records.push({ field, value: clean, confidence, source_url: page.url, note });
  };
  push("firma_name", page?.profile?.name, "high", "Northdata profile: Name");
  if (page?.profile?.address) {
    const address = parseAddressLine(page.profile.address);
    push("firma_anschrift", address.street, "high", "Northdata profile: Adresse");
    push("firma_plz", address.plz, "high", "Northdata profile: Adresse");
    push("firma_ort", address.ort, "high", "Northdata profile: Adresse");
  }
  return records;
}

// ---------------------------------------------------------------------------
// Parsing — mirrors src/sources/northdata.rs extract_from_html. Regex is
// intentionally permissive; the unit tests on the Rust side gate the
// selector logic. JS-side drift fixes happen by revising this file.
// ---------------------------------------------------------------------------

function parseGeneralInfoItem(html, label) {
  // Northdata renders ribbon sections as
  //   <h3 class="... ribbon ... label">Adresse</h3>
  //   <div class="general-information"><ul><li><div class="content">Grenzacherstrasse 124, 4058 Basel</div></li></ul>
  const labelEscaped = label.replace(/[-/\\^$*+?.()|[\]{}]/g, "\\$&");
  const re = new RegExp(
    "<h3[^>]*ribbon[^>]*>\\s*" +
      labelEscaped +
      "\\s*<\\/h3>([\\s\\S]*?)<h3",
    "i",
  );
  const block = html.match(re);
  if (!block) return null;
  const contentRe = /class=\"[^\"]*content[^\"]*\"[^>]*>([\s\S]*?)<\//i;
  const content = block[1].match(contentRe);
  if (!content) return null;
  return content[1].replace(/<[^>]+>/g, "").replace(/\s+/g, " ").trim();
}

function parseHeading(html) {
  const m = html.match(/<h1[^>]*class=\"[^\"]*qualified[^\"]*\"[^>]*>([\s\S]*?)<\/h1>/i);
  if (!m) return null;
  return m[1].replace(/<[^>]+>/g, "").replace(/\s+/g, " ").trim();
}

function parseAddressLine(line) {
  // "Grenzacherstrasse 124, 4058 Basel" → {street, plz, ort}
  const parts = line.split(",").map((s) => s.trim()).filter(Boolean);
  if (parts.length < 2) return { street: line, plz: null, ort: null };
  const street = parts[0];
  const last = parts[parts.length - 1];
  const plzMatch = last.match(/\b(\d{4,5})\b\s*(.*)/);
  if (plzMatch) {
    return { street, plz: plzMatch[1], ort: plzMatch[2].trim() || null };
  }
  return { street, plz: null, ort: last };
}

function parseBizqPersons(html) {
  // <figure class="bizq" data-data='[{...}]'> with persons.
  const figures = [
    ...html.matchAll(/<figure[^>]*class=\"[^\"]*bizq[^\"]*\"[^>]*data-data=\"([^\"]+)\"/gi),
  ];
  const out = [];
  for (const fig of figures) {
    let dataStr = fig[1]
      .replace(/&quot;/g, '"')
      .replace(/&amp;/g, "&")
      .replace(/&#39;/g, "'");
    try {
      const data = JSON.parse(dataStr);
      const items = Array.isArray(data) ? data : data.items || [];
      for (const item of items) {
        if (item && !item.old && typeof item.text === "string") {
          out.push(item.text);
        }
      }
    } catch (err) {
      // Selector drifted; let the empty-records path trigger portal_drift.
    }
  }
  return out;
}

function splitPersonClause(text) {
  // "Vorstand Anna Müller" → {position: "Vorstand", first: "Anna", last: "Müller"}
  const trimmed = text.trim();
  const m = trimmed.match(/^([A-Za-zÄÖÜäöü\-\s\.]+?)\s+([A-ZÄÖÜ][A-Za-zÄÖÜäöü\-]+)\s+([A-ZÄÖÜ][A-Za-zÄÖÜäöü\-]+(?:\s+[A-ZÄÖÜ][A-Za-zÄÖÜäöü\-]+)*)$/);
  if (!m) return null;
  return { position: m[1].trim(), first: m[2].trim(), last: m[3].trim() };
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

  const name = parseGeneralInfoItem(html, "Name");
  if (name) {
    push("firma_name", name, "high", "ribbon section: Name");
  } else {
    const h1 = parseHeading(html);
    if (h1) {
      const cleaned = h1.split(",")[0].trim();
      push("firma_name", cleaned, "medium", "h1 fallback");
    }
  }

  let addressLine = parseGeneralInfoItem(html, "Adresse");
  if (!addressLine) {
    addressLine = parseGeneralInfoItem(html, "Anschrift");
  }
  if (addressLine) {
    const parsed = parseAddressLine(addressLine);
    if (parsed.street) push("firma_anschrift", parsed.street, "high", "ribbon section: Adresse");
    if (parsed.plz) push("firma_plz", parsed.plz, "high", "ribbon section: Adresse");
    if (parsed.ort) push("firma_ort", parsed.ort, "high", "ribbon section: Adresse");
  }

  for (const clause of parseBizqPersons(html)) {
    const parsed = splitPersonClause(clause);
    if (!parsed) continue;
    push("person_position", parsed.position, "medium", "bizq figure: position");
    push("person_vorname", parsed.first, "medium", "bizq figure: first name");
    push("person_nachname", parsed.last, "medium", "bizq figure: last name");
    // First clause is enough for an aggregated record set.
    break;
  }

  return records;
}

// ---------------------------------------------------------------------------
// main
// ---------------------------------------------------------------------------

(function main() {
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

  let blocked = false;
  for (const url of candidateUrls(input, company, country)) {
    const browser = browserPage(url);
    blocked ||= isBlockedPage(browser) || (browser?.detection?.markers || []).length > 0;
    if (pageMatchesCompany(company, browser)) {
      const records = browser?.profile?.name
        ? recordsFromBrowserProfile(browser)
        : extractRecords(browser.url, browser.html || "");
      if (records.length > 0 && records.some((record) => record.field === "firma_name" && identityMatches(company, record.value))) {
        process.stdout.write(JSON.stringify({ records }));
        return;
      }
    }

    const direct = readPage(url, country);
    blocked ||= isBlockedPage(direct);
    if (direct?.ok && !direct.url) direct.url = url;
    if (pageMatchesCompany(company, direct)) {
      const html = direct.raw_html_excerpt || direct.raw_html || direct.html || "";
      const records = extractRecords(direct.url, html);
      if (records.length > 0 && records.some((record) => record.field === "firma_name" && identityMatches(company, record.value))) {
        process.stdout.write(JSON.stringify({ records }));
        return;
      }
    }
  }

  process.stdout.write(JSON.stringify({
    records: [],
    failure_mode: blocked ? "blocked" : "temporary_unreachable",
    detail: blocked
      ? "Northdata challenge recorded by CTOX browser automation for web-unlock"
      : "no origin- and identity-verified Northdata profile data",
  }));
})();
