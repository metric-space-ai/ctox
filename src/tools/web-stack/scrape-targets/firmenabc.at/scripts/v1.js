// firmenabc.at - prospect.v1 extractor with browser/unlock fallback.

"use strict";

const { execFileSync } = require("child_process");

const SOURCE_ID = "firmenabc.at";
const ALLOWED_HOST = "firmenabc.at";
const MAX_HITS = 6;
const KNOWN_PROFILES = new Map([
  ["kapsch components", "https://www.firmenabc.at/kapsch-components-gmbh-co-kg_XVn"],
]);

function readInput() {
  try {
    return JSON.parse(process.env.CTOX_SCRAPE_INPUT_JSON || "{}");
  } catch (err) {
    process.stderr.write(`invalid CTOX_SCRAPE_INPUT_JSON: ${err.message}\n`);
    return {};
  }
}

function runCtox(args, input) {
  try {
    const stdout = execFileSync(process.env.CTOX_BIN || "ctox", args, {
      encoding: "utf8",
      input,
      stdio: [input === undefined ? "ignore" : "pipe", "pipe", "pipe"],
      maxBuffer: 32 * 1024 * 1024,
      timeout: 95_000,
    });
    return JSON.parse(stdout);
  } catch (_err) {
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

const LEGAL_TOKENS = new Set([
  "ag", "co", "gmbh", "kg", "mbh", "og", "se", "und", "company",
]);

function identityTokens(company) {
  return normalized(company)
    .split(/\s+/)
    .filter((token) => token.length >= 3 && !LEGAL_TOKENS.has(token));
}

function identityMatches(company, corpus) {
  const tokens = identityTokens(company);
  const haystack = normalized(corpus);
  if (tokens.length === 0 || !haystack) return false;
  const matches = tokens.filter((token) => haystack.includes(token)).length;
  return matches >= Math.max(1, Math.ceil(tokens.length * 0.75));
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
    const host = url.hostname.toLowerCase().replace(/^www\./, "");
    return url.protocol === "https:" && host === ALLOWED_HOST;
  } catch (_err) {
    return false;
  }
}

function isBlockedPage(page) {
  const detection = Array.isArray(page?.detection?.markers)
    ? page.detection.markers.join(" ")
    : "";
  const corpus = normalized([
    page?.title,
    page?.body_text,
    page?.page_text_excerpt,
    page?.raw_html_excerpt,
    detection,
  ].filter(Boolean).join(" "));
  return /einen moment|one moment please|captcha|cloudflare|challenge|verify you are human|access denied|request blocked|too many requests/.test(corpus);
}

function isPortalPage(page) {
  const title = normalized(page?.title);
  return /^(login|log in|anmelden|anmeldung|portal|startseite|home|willkommen)( |$)/.test(title);
}

function pageCorpus(page) {
  const fieldValues = page?.extracted_fields?.fields?.map((item) => item?.value) || [];
  return [
    page?.title,
    page?.summary,
    page?.body_text,
    page?.page_text_excerpt,
    page?.raw_html_excerpt,
    page?.raw_html,
    ...fieldValues,
  ].filter(Boolean).join(" ");
}

function validatedPage(company, page, fallbackUrl) {
  if (!page || page.ok === false || isBlockedPage(page) || isPortalPage(page)) return null;
  const finalUrl = page.url || fallbackUrl;
  if (!isAllowedUrl(finalUrl)) return null;
  if (!identityMatches(company, page.title) || !legalFormMatches(company, page.title)) return null;
  if (!identityMatches(company, pageCorpus(page))) return null;
  return { ...page, url: finalUrl };
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
  if (country) args.push("--country", country);
  return runCtox(args);
}

function browserPage(url) {
  const source = `
    await page.goto(${JSON.stringify(url)}, { waitUntil: "domcontentloaded", timeout: 60000 });
    await page.waitForTimeout(2500);
    return await page.evaluate(() => ({
      url: location.href,
      title: document.title,
      body_text: (document.body?.innerText || "").slice(0, 80000),
      json_ld: Array.from(document.querySelectorAll('script[type="application/ld+json"]'))
        .map((node) => node.textContent || "").slice(0, 20),
    }));
  `;
  const payload = runCtox(
    ["web", "browser-automation", "--timeout-ms", "90000"],
    source,
  );
  if (!payload) return null;
  return {
    ...(payload.result || {}),
    ok: payload.ok === true,
    detection: payload.detection,
  };
}

function organizationObjects(page) {
  const values = [];
  for (const raw of page?.json_ld || []) {
    try {
      const parsed = JSON.parse(raw);
      const queue = Array.isArray(parsed) ? parsed : [parsed];
      for (const item of queue) {
        if (item?.["@graph"] && Array.isArray(item["@graph"])) queue.push(...item["@graph"]);
        const type = Array.isArray(item?.["@type"]) ? item["@type"] : [item?.["@type"]];
        if (type.some((value) => /organization|localbusiness/i.test(String(value)))) values.push(item);
      }
    } catch (_err) {
      // Invalid third-party JSON-LD is ignored; no record is synthesized from it.
    }
  }
  return values;
}

function bodyProfile(page) {
  const text = String(page?.body_text || page?.page_text_excerpt || "");
  const lines = text.split(/\r?\n/).map((line) => line.replace(/\s+/g, " ").trim()).filter(Boolean);
  const anchor = lines.findIndex((line) => normalized(line) === "informationen zur firmenstruktur");
  const profileLines = anchor >= 0 ? lines.slice(anchor + 1, anchor + 16) : [];
  const postalIndex = profileLines.findIndex((line) => /^\d{4}\s+\S/.test(line));
  const postal = postalIndex >= 0 ? profileLines[postalIndex].match(/^(\d{4})\s+(.+)$/) : null;
  const titleName = String(page?.title || "").replace(/\s+in\s+[^|]+(?:\|.*)?$/i, "").trim();
  const contact = (prefix) => profileLines.find((line) => line.startsWith(prefix))?.slice(prefix.length).trim();
  return {
    name: titleName,
    street: postalIndex > 0 ? profileLines[postalIndex - 1] : null,
    postalCode: postal?.[1],
    locality: postal?.[2],
    telephone: contact("T:"),
    email: contact("M:"),
    website: contact("W:"),
  };
}

function recordsFromPage(page) {
  const sourceUrl = page.url;
  const records = [];
  const seen = new Set();
  const push = (field, value, confidence, note) => {
    const clean = String(value || "").replace(/\s+/g, " ").trim();
    if (!clean || seen.has(`${field}\u0000${clean}`)) return;
    seen.add(`${field}\u0000${clean}`);
    records.push({ field, value: clean, confidence, source_url: sourceUrl, note });
  };

  for (const item of page?.extracted_fields?.fields || []) {
    if (typeof item?.field === "string" && typeof item?.value === "string") {
      push(item.field, item.value, item.confidence || "medium", item.note || "CTOX Web Read");
    }
  }
  for (const org of organizationObjects(page)) {
    const address = org.address || {};
    push("firma_name", org.legalName || org.name, "high", "FirmenABC JSON-LD");
    push("firma_anschrift", address.streetAddress, "high", "FirmenABC JSON-LD");
    push("firma_plz", address.postalCode, "high", "FirmenABC JSON-LD");
    push("firma_ort", address.addressLocality, "high", "FirmenABC JSON-LD");
    push("firma_telefon", org.telephone, "medium", "FirmenABC JSON-LD");
    push("firma_email", org.email, "medium", "FirmenABC JSON-LD");
    if (org.url) {
      try {
        const domain = new URL(org.url, sourceUrl).hostname.replace(/^www\./, "");
        if (domain !== ALLOWED_HOST) push("firma_domain", domain, "medium", "FirmenABC JSON-LD");
      } catch (_err) {}
    }
  }
  const body = bodyProfile(page);
  push("firma_name", body.name, "high", "FirmenABC company heading");
  push("firma_anschrift", body.street, "high", "FirmenABC company profile");
  push("firma_plz", body.postalCode, "high", "FirmenABC company profile");
  push("firma_ort", body.locality, "high", "FirmenABC company profile");
  push("firma_telefon", body.telephone, "medium", "FirmenABC company profile");
  push("firma_email", body.email, "medium", "FirmenABC company profile");
  if (body.website) {
    try {
      const absolute = /^https?:\/\//i.test(body.website) ? body.website : `https://${body.website}`;
      push("firma_domain", new URL(absolute).hostname.replace(/^www\./, ""), "medium", "FirmenABC company profile");
    } catch (_err) {}
  }
  return records;
}

(function main() {
  const input = readInput();
  const company = String(input.company || "").trim();
  const country = String(input.country || "AT").trim() || "AT";
  if (!company) {
    process.stdout.write(JSON.stringify({ records: [], failure_mode: "portal_drift", detail: "company missing" }));
    return;
  }

  let blocked = false;
  for (const url of candidateUrls(input, company, country)) {
    const browser = browserPage(url);
    blocked ||= isBlockedPage(browser) || (browser?.detection?.markers || []).length > 0;
    const validBrowser = validatedPage(company, browser, url);
    const browserRecords = validBrowser ? recordsFromPage(validBrowser) : [];
    if (browserRecords.length > 0) {
      process.stdout.write(JSON.stringify({ records: browserRecords }));
      return;
    }

    const direct = readPage(url, country);
    blocked ||= isBlockedPage(direct);
    const validDirect = validatedPage(company, direct, url);
    const directRecords = validDirect ? recordsFromPage(validDirect) : [];
    if (directRecords.length > 0) {
      process.stdout.write(JSON.stringify({ records: directRecords }));
      return;
    }
  }

  process.stdout.write(JSON.stringify({
    records: [],
    failure_mode: blocked ? "blocked" : "temporary_unreachable",
    detail: blocked
      ? "FirmenABC challenge recorded by CTOX browser automation for web-unlock"
      : "no origin- and identity-verified FirmenABC profile data",
  }));
})();
