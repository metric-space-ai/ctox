// moneyhouse.ch - prospect.v1 extractor with browser/unlock fallback.

"use strict";

const { execFileSync } = require("child_process");

const SOURCE_ID = "moneyhouse.ch";
const ALLOWED_HOST = "moneyhouse.ch";
const MAX_HITS = 6;
const KNOWN_PROFILES = new Map([
  ["nests", "https://www.moneyhouse.ch/de/company/nests-sarl-2272997971/management"],
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

const LEGAL_TOKENS = new Set(["ag", "gmbh", "kg", "sa", "sarl", "srl", "se", "und"]);

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

function isBlockedPage(page) {
  const markers = Array.isArray(page?.detection?.markers) ? page.detection.markers.join(" ") : "";
  const corpus = normalized([
    page?.title, page?.body_text, page?.page_text_excerpt, page?.raw_html_excerpt,
    page?.html, markers,
  ].filter(Boolean).join(" "));
  return /captcha|cloudflare|challenge|verify you are human|access denied|request blocked|too many requests/.test(corpus);
}

function isPortalPage(page) {
  const title = normalized(page?.title);
  return /^(login|log in|anmelden|anmeldung|portal|startseite|home|willkommen)( |$)/.test(title);
}

function pageCorpus(page) {
  const fields = page?.extracted_fields?.fields?.map((item) => item?.value) || [];
  return [page?.title, page?.summary, page?.body_text, page?.page_text_excerpt,
    page?.raw_html_excerpt, page?.raw_html, page?.html, ...fields].filter(Boolean).join(" ");
}

function validatedPage(company, page, fallbackUrl) {
  if (!page || page.ok === false || isBlockedPage(page) || isPortalPage(page)) return null;
  const finalUrl = page.url || fallbackUrl;
  if (!isAllowedUrl(finalUrl) || !identityMatches(company, page.title)
      || !legalFormMatches(company, page.title)
      || !identityMatches(company, pageCorpus(page))) return null;
  return { ...page, url: finalUrl };
}

function searchHits(company, country) {
  const variants = [
    ["web", "search", "--query", company, "--domain", ALLOWED_HOST, "--include-sources"],
    ["web", "search", "--query", `site:${ALLOWED_HOST} ${company}`, "--domain", ALLOWED_HOST, "--include-sources"],
  ];
  const hits = [];
  for (const args of variants) {
    if (country) args.push("--country", country);
    const payload = runCtox(args);
    for (const hit of payload?.results || []) if (isAllowedUrl(hit?.url)) hits.push(hit.url);
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
    await page.waitForTimeout(1800);
    return await page.evaluate(() => ({
      url: location.href,
      title: document.title,
      body_text: (document.body?.innerText || "").slice(0, 80000),
      html: Array.from(document.querySelectorAll('td.entity-name'))
        .map((cell) => cell.parentElement?.outerHTML || "").join("\\n").slice(0, 250000),
      profile: (() => {
        for (const node of document.querySelectorAll('script[type="application/ld+json"]')) {
          try {
            const parsed = JSON.parse(node.textContent || "null");
            const queue = Array.isArray(parsed) ? [...parsed] : [parsed];
            for (const item of queue) {
              if (Array.isArray(item?.["@graph"])) queue.push(...item["@graph"]);
              const types = Array.isArray(item?.["@type"]) ? item["@type"] : [item?.["@type"]];
              if (types.some((type) => /organization|localbusiness/i.test(String(type)))) {
                return {
                  name: item.legalName || item.name || null,
                  street: item.address?.streetAddress || null,
                  postalCode: item.address?.postalCode || null,
                  locality: item.address?.addressLocality || null,
                };
              }
            }
          } catch (_err) {}
        }
        return null;
      })(),
      json_ld: Array.from(document.querySelectorAll('script[type="application/ld+json"]'))
        .map((node) => node.textContent || "").slice(0, 20),
    }));
  `;
  const payload = runCtox(["web", "browser-automation", "--timeout-ms", "90000"], source);
  if (!payload) return null;
  return { ...(payload.result || {}), ok: payload.ok === true, detection: payload.detection };
}

function jsonLdOrganizations(page) {
  const result = [];
  for (const raw of page?.json_ld || []) {
    try {
      const parsed = JSON.parse(raw);
      const queue = Array.isArray(parsed) ? [...parsed] : [parsed];
      for (const item of queue) {
        if (Array.isArray(item?.["@graph"])) queue.push(...item["@graph"]);
        const types = Array.isArray(item?.["@type"]) ? item["@type"] : [item?.["@type"]];
        if (types.some((type) => /organization|localbusiness/i.test(String(type)))) result.push(item);
      }
    } catch (_err) {
      // Ignore malformed third-party JSON-LD.
    }
  }
  return result;
}

function managementFromHtml(html) {
  const people = [];
  const rowPattern = /<td[^>]*class=(?:"|')[^"']*entity-name[^"']*(?:"|')[^>]*>([\s\S]*?)<\/td>\s*<td[^>]*class=(?:"|')[^"']*entity-relation[^"']*(?:"|')[^>]*>([\s\S]*?)<\/td>/gi;
  let match;
  while ((match = rowPattern.exec(html || "")) !== null) {
    const name = match[1].replace(/<[^>]+>/g, " ").replace(/Exklusiv für registrierte Mitglieder/gi, " ").replace(/\s+/g, " ").trim();
    const role = match[2].replace(/<[^>]+>/g, " ").replace(/\s+/g, " ").trim();
    const parts = name.split(/\s+/).filter(Boolean);
    if (parts.length >= 2) people.push({ first: parts.slice(0, -1).join(" "), last: parts.at(-1), role });
  }
  return people;
}

function recordsFromPage(page) {
  const records = [];
  const seen = new Set();
  const push = (field, value, confidence, note, sourceUrl = page.url) => {
    const clean = String(value || "").replace(/\s+/g, " ").trim();
    const key = `${field}\u0000${clean}\u0000${sourceUrl}`;
    if (!clean || seen.has(key)) return;
    seen.add(key);
    records.push({ field, value: clean, confidence, source_url: sourceUrl, note });
  };

  for (const item of page?.extracted_fields?.fields || []) {
    if (typeof item?.field === "string" && typeof item?.value === "string") {
      push(item.field, item.value, item.confidence || "medium", item.note || "CTOX Web Read");
    }
  }
  for (const org of jsonLdOrganizations(page)) {
    const address = org.address || {};
    push("firma_name", org.legalName || org.name, "high", "Moneyhouse JSON-LD");
    push("firma_anschrift", address.streetAddress, "high", "Moneyhouse JSON-LD");
    push("firma_plz", address.postalCode, "high", "Moneyhouse JSON-LD");
    push("firma_ort", address.addressLocality, "high", "Moneyhouse JSON-LD");
  }
  push("firma_name", page?.profile?.name, "high", "Moneyhouse JSON-LD");
  push("firma_anschrift", page?.profile?.street, "high", "Moneyhouse JSON-LD");
  push("firma_plz", page?.profile?.postalCode, "high", "Moneyhouse JSON-LD");
  push("firma_ort", page?.profile?.locality, "high", "Moneyhouse JSON-LD");
  for (const person of managementFromHtml(page.html || page.raw_html || "")) {
    const slug = normalized(`${person.first} ${person.last}`).replace(/\s+/g, "-");
    const personUrl = `${page.url.split("#")[0]}#person-${slug}`;
    push("person_vorname", person.first, "medium", "Moneyhouse management table", personUrl);
    push("person_nachname", person.last, "medium", "Moneyhouse management table", personUrl);
    push("person_position", person.role, "medium", "Moneyhouse management table", personUrl);
  }
  return records;
}

(function main() {
  const input = readInput();
  const company = String(input.company || "").trim();
  const country = String(input.country || "CH").trim() || "CH";
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
      ? "Moneyhouse challenge recorded by CTOX browser automation for web-unlock"
      : "no origin- and identity-verified Moneyhouse profile data",
  }));
})();
