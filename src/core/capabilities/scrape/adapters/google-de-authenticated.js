// Versioned successor to registered google-de rev25. Requires #643.
// google.de — prospect.v1 extractor (Phase B initial revision).
//
// Reads CTOX_SCRAPE_INPUT_JSON for the company + country, drives the
// owner-bound native login/capture over the live google.de SERP to discover
// the company's own website, then parses that page for the field set
// documented in `tools/web-stack/src/sources/EXCEL_MATRIX.md`.
//
// The Google leg only accepts payloads whose provider is verified as
// `google` (foreign provider results are rejected by target policy) and
// only emits evidence whose source URL is a safe public http(s) URL —
// result URLs may be external (the company's own site).
//
// Drift contract: if the selectors below stop matching but pages load
// successfully, this script returns an empty records array.
// `ctox scrape execute --allow-heal` then classifies the run as
// `portal_drift` and enqueues a `universal-scraping` repair task that
// will revise this very file.

"use strict";

const { execFileSync } = require("child_process");

const SOURCE_ID = "google.de";
const MAX_HITS = 5;

const RECORD_FIELDS = new Set([
  "firma_name", "firma_anschrift", "firma_plz", "firma_ort", "firma_email",
  "firma_domain", "firma_telefon", "wz_code", "umsatz", "mitarbeiter",
  "crm_record_number", "person_titel", "person_vorname", "person_nachname",
  "person_funktion", "person_position", "person_email", "person_email_validation",
  "person_telefon", "person_linkedin", "person_xing",
]);
const CONFIDENCE_LEVELS = new Set(["low", "medium", "high", "user_provided"]);

// Hosts that are directories/aggregators, never the company's own site.
const DIRECTORY_HOST_PARTS = [
  "google.", "linkedin.", "xing.", "facebook.", "instagram.", "youtube.",
  "wikipedia.", "northdata.", "companyhouse.", "moneyhouse.", "firmenabc.",
  "bundesanzeiger.", "handelsregister.", "firmenwissen.", "werkenntdenbesten.",
  "11880.", "golocal.", "kennstdueinen.", "cylex.", "dastelefonbuch.",
  "gelbeseiten.", "europages.", "kompass.", "indeed.", "stepstone.",
  "kununu.", "glassdoor.", "partcommunity.",
];

function readInput() {
  const raw = process.env.CTOX_SCRAPE_INPUT_JSON;
  if (!raw) return { company: "", country: "" };
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

function runCtox(args, input, timeout = 35_000) {
  try {
    const out = execFileSync(ctoxBin(), args, {
      encoding: "utf8",
      input,
      stdio: [input === undefined ? "ignore" : "pipe", "pipe", "pipe"],
      maxBuffer: 32 * 1024 * 1024,
      timeout,
    });
    return JSON.parse(out);
  } catch (_err) {
    // Stay silent on per-hit failures: `classify_outcome` in
    // src/capabilities/scrape.rs substring-matches stderr for "temporary",
    // "timeout", "429", … and would misclassify the whole run if one
    // candidate page failed while others succeeded.
    return null;
  }
}

function normalized(value) {
  return String(value || "")
    .normalize("NFKD")
    .replace(/[̀-ͯ]/g, "")
    .toLocaleLowerCase("de-DE")
    .replace(/ß/g, "ss")
    .replace(/[^a-z0-9]+/g, " ")
    .trim();
}

const LEGAL_TOKENS = new Set([
  "ag", "gmbh", "mbh", "se", "kg", "kgaa", "ohg", "ug", "ltd", "inc", "und",
  "co", "company", "holding", "gruppe",
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
  return tokens.every((token) => haystack.includes(token));
}

function isPortalOrLoginTitle(title) {
  const text = String(title || "").replace(/\s+/g, " ").trim();
  if (!text) return false;
  return /\b(?:log[ -]?in|sign[ -]?in|anmeld(?:en|ung)|authentication|authentifizierung|kundenportal|customer portal)\b/i.test(text)
    || /^(?:portal|startseite|home|willkommen)(?:\s*[-|:]\s*.*)?$/i.test(text);
}

function safePublicHttpUrl(value) {
  try {
    const url = new URL(value);
    if (!["http:", "https:"].includes(url.protocol) || url.username || url.password) {
      return false;
    }
    const host = url.hostname.toLowerCase();
    return Boolean(host)
      && host !== "localhost"
      && !host.endsWith(".localhost")
      && !host.endsWith(".local")
      && !/^(?:127\.|10\.|169\.254\.|192\.168\.)/.test(host)
      && !/^172\.(?:1[6-9]|2\d|3[01])\./.test(host)
      && host !== "::1";
  } catch {
    return false;
  }
}

function hostOf(value) {
  try {
    return new URL(value).hostname.replace(/^www\./, "").toLowerCase();
  } catch {
    return "";
  }
}

function isDirectoryHost(value) {
  const host = hostOf(value);
  if (!host) return true;
  return DIRECTORY_HOST_PARTS.some((part) => host.includes(part));
}

// ---------------------------------------------------------------------------
// CTOX web-stack drivers
// ---------------------------------------------------------------------------

function searchGoogle(company, country) {
  const args = [
    "web", "search",
    "--query", company,
    "--source", SOURCE_ID,
    "--include-sources",
  ];
  if (country) args.push("--country", country);
  const payload = runCtox(args);
  const providers = [String(payload?.provider || "").toLowerCase()];
  const sourceFailures = Array.isArray(payload?.source_failures) ? payload.source_failures : [];
  // Foreign provider results are rejected by target policy: only accept
  // payloads the web stack verified as coming from Google.
  const providerOk = providers.includes("google");
  const results = providerOk && Array.isArray(payload?.results) ? payload.results : [];
  return { results, sourceFailures, providerOk };
}

// Consent-aware SERP pass over the live google.de site. Mirrors the proven
// solo probe: handle the ordinary consent dialog, wait for organic h3
// results, extract anchors. A challenge page (unusual traffic / sorry) is
// surfaced via the ctox detection markers, never bypassed.
function serpBrowserSource(company) {
  return `
const company = ${JSON.stringify(company)};
const url = "https://www.google.de/search?q=" + encodeURIComponent(company) + "&hl=de";
await ctoxBrowser.goto(url, { timeoutMs: 30000 });
await page.waitForLoadState("domcontentloaded", { timeout: 10000 }).catch(() => null);
const consentNames = [/alle ablehnen/i, /alles ablehnen/i, /reject all/i, /alle akzeptieren/i, /alles akzeptieren/i, /accept all/i, /zustimmen/i];
for (const name of consentNames) {
  const button = page.getByRole("button", { name }).first();
  if (await button.count()) {
    await button.click({ timeout: 2500 }).catch(() => null);
    await page.waitForLoadState("domcontentloaded", { timeout: 10000 }).catch(() => null);
    break;
  }
}
await page.waitForSelector("h3", { timeout: 15000 }).catch(() => null);
const text = await page.locator("body").innerText().catch(() => "");
const results = await page.evaluate(() => {
  const out = [];
  for (const h3 of document.querySelectorAll("h3")) {
    const anchor = h3.closest("a[href]");
    if (!anchor || !/^https?:/.test(anchor.href)) continue;
    let container = anchor;
    for (let i = 0; i < 6 && container.parentElement; i += 1) {
      container = container.parentElement;
      if ((container.innerText || "").length > (anchor.innerText || "").length + 60) break;
    }
    out.push({
      url: anchor.href,
      title: (h3.innerText || "").replace(/\\s+/g, " ").trim(),
      summary: (container.innerText || "").replace(/\\s+/g, " ").trim().slice(0, 600),
    });
  }
  return out;
});
return {
  url: page.url(),
  title: await page.title().catch(() => ""),
  page_text_excerpt: text.replace(/\\s+/g, " ").trim().slice(0, 16000),
  results,
};
`;
}

function runBrowserAutomation(name, source, timeoutMs = 60_000) {
  const authenticated = name === "portal-search-google-de";
  const binding = readInput();
  if (authenticated && (!String(binding.owner_user_id || "").trim()
      || !String(binding.task_id || "").trim())) {
    return { ok: false, markers: ["authorization_required"], result: {} };
  }
  const args = authenticated ? [
    "business-os", "web-stack", "authenticated-automation",
    "--source-id", SOURCE_ID,
    "--target-url", "https://accounts.google.com/",
    "--credential-ref", "ctox-secret://credentials/GOOGLE_BROWSER_LOGIN",
    "--owner-user-id", String(binding.owner_user_id),
    "--task-id", String(binding.task_id),
    "--timeout-ms", String(timeoutMs),
  ] : ["web", "browser-automation", "--timeout-ms", String(timeoutMs)];
  const payload = runCtox(args, source, timeoutMs + 10_000);
  if (authenticated && (!payload?.ok || !payload.session_id
      || payload.authenticated_login?.status !== "completed")) {
    return { ok: false, markers: ["authorization_required"], result: {} };
  }
  const markers = Array.isArray(payload?.detection?.markers) ? payload.detection.markers.map(String) : [];
  if (!payload?.ok) return { ok: false, markers };
  return { ok: true, markers, result: authenticated ? payload : (payload.result || {}) };
}

function serpPage(company) {
  return runBrowserAutomation("portal-search-google-de", serpBrowserSource(company), 70_000);
}

function readPage(url, country) {
  const args = ["web", "read", "--url", url];
  if (country) args.push("--country", country);
  return runCtox(args, undefined, 20_000);
}

function readPageWithBrowser(url) {
  const source = `
const targetUrl = ${JSON.stringify(url)};
await ctoxBrowser.goto(targetUrl, { timeoutMs: 30000 });
await page.waitForLoadState("domcontentloaded", { timeout: 10000 }).catch(() => null);
const consentPatterns = [/nur technisch notwendige/i, /alle akzeptieren/i, /akzeptieren/i, /zustimmen/i, /verstanden/i, /agree/i];
for (const pattern of consentPatterns) {
  const button = page.getByRole("button", { name: pattern }).first();
  if (await button.count()) {
    await button.click({ timeout: 2500 }).catch(() => null);
    break;
  }
}
await page.waitForTimeout(1200);
const text = await page.locator("body").innerText().catch(() => "");
return {
  ok: text.trim().length > 0,
  url: page.url(),
  title: await page.title().catch(() => ""),
  page_text_excerpt: text.replace(/\\s+/g, " ").trim().slice(0, 16000),
};
`;
  const payload = runBrowserAutomation(
    `source-read-${SOURCE_ID.replace(/[^a-z0-9]/gi, "-")}`,
    source,
    50_000,
  );
  return payload.ok ? { ...payload.result, markers: payload.markers } : { ok: false, markers: payload.markers };
}

function recordUnlockSignal(url, markers) {
  return runCtox([
    "web", "unlock", "signals", "record",
    "--source", "scrape-target:google.de",
    "--url", safePublicHttpUrl(url) ? url : "https://www.google.de/",
    "--evidence", JSON.stringify({
      source_id: SOURCE_ID,
      detection: "access_challenge",
      markers: [...new Set((markers || []).map(String))].slice(0, 12),
      secret_value_in_payload: false,
    }),
  ]);
}

function looksChallenged(url, text, markers) {
  const markerText = (markers || []).join(" ");
  return /\/sorry\//.test(String(url || ""))
    || /unusual traffic|ungewöhnlicher Datenverkehr|not a robot|kein Roboter|automatisierte Anfragen/i.test(String(text || ""))
    || /captcha|challenge|turnstile|access[_ -]?denied|rate[_ -]?limit/i.test(markerText);
}

// ---------------------------------------------------------------------------
// Records
// ---------------------------------------------------------------------------

function appendRecord(records, record) {
  const field = String(record?.field || "").trim();
  const value = String(record?.value || "").replace(/\s+/g, " ").trim();
  if (!RECORD_FIELDS.has(field) || !value) return;
  const sourceUrl = String(record?.source_url || "").trim();
  if (!safePublicHttpUrl(sourceUrl)) return;
  const confidence = String(record?.confidence || "medium").toLowerCase();
  if (!CONFIDENCE_LEVELS.has(confidence)) return;
  if (records.some((item) => item.field === field && item.value === value && item.source_url === sourceUrl)) return;
  records.push({
    field,
    value,
    confidence,
    source_url: sourceUrl,
    note: String(record?.note || "CTOX web-stack google.de adapter").replace(/\s+/g, " ").trim(),
  });
}

function pageText(page) {
  return [page?.title, page?.summary, page?.page_text_excerpt]
    .filter(Boolean)
    .join(" ")
    .replace(/\s+/g, " ")
    .trim();
}

function appendContactHeuristics(records, page, sourceUrl) {
  const text = pageText(page);
  if (!text) return;
  const emails = [...text.matchAll(/\b[A-Z0-9._%+-]+@[A-Z0-9.-]+\.[A-Z]{2,}\b/gi)]
    .map((match) => match[0].toLowerCase())
    .filter((email) => !/\.(?:png|jpe?g|gif|webp|svg)$/i.test(email))
    .filter((email) => !/(?:example\.|sentry|wixpress|googleapis|gstatic|google\.)/i.test(email))
    .slice(0, 3);
  for (const email of emails) {
    appendRecord(records, {
      field: "firma_email",
      value: email,
      confidence: "medium",
      source_url: sourceUrl,
      note: "Email published on the company page discovered by Google",
    });
  }
  const phones = [...text.matchAll(/(?:\+|00)\d[\d\s()\/-]{7,}\d/g)]
    .map((match) => match[0].replace(/\s+/g, " ").trim())
    .slice(0, 2);
  for (const phone of phones) {
    appendRecord(records, {
      field: "firma_telefon",
      value: phone,
      confidence: "medium",
      source_url: sourceUrl,
      note: "google.de company page text",
    });
  }
  const postal = text.match(/\b(?:D-)?(\d{5})\s+([A-ZÄÖÜ][A-Za-zÄÖÜäöüß.'-]+)/);
  if (postal) {
    appendRecord(records, {
      field: "firma_plz",
      value: postal[1],
      confidence: "medium",
      source_url: sourceUrl,
      note: "google.de company page address text",
    });
    appendRecord(records, {
      field: "firma_ort",
      value: postal[2],
      confidence: "medium",
      source_url: sourceUrl,
      note: "google.de company page address text",
    });
  }
}

// ---------------------------------------------------------------------------
// main
// ---------------------------------------------------------------------------

function main() {
  const input = readInput();
  const company = String(input.company || "").trim();
  const country = String(input.country || "").trim().toUpperCase();
  if (!company) {
    process.stdout.write(JSON.stringify({
      records: [],
      failure_mode: "portal_drift",
      detail: "CTOX_SCRAPE_INPUT_JSON.company missing",
    }));
    return;
  }

  const records = [];
  let blocked = false;
  let blockedUrl = "";
  let blockedMarkers = [];
  const markBlocked = (url, markers) => {
    blocked = true;
    blockedUrl ||= url;
    blockedMarkers.push(...(markers || []));
  };

  // Candidate discovery, provider-verified Google first.
  const candidates = [];
  const seen = new Set();
  const pushCandidate = (hit) => {
    const url = String(hit?.url || "").trim();
    if (!url || seen.has(url) || !safePublicHttpUrl(url)) return;
    seen.add(url);
    candidates.push(hit);
  };


  // Live SERP pass (consent-aware) adds organic results the search API may
  // have missed and doubles as the challenge detector for the target.
  const serp = serpPage(company);
  if (serp.ok) {
    if (looksChallenged(serp.result.url, serp.result.page_text_excerpt, serp.markers)) {
      markBlocked(serp.result.url, serp.markers.length > 0 ? serp.markers : ["access_challenge"]);
    } else if (serp.markers.length > 0) {
      markBlocked(serp.result.url, serp.markers);
    } else {
      for (const hit of (serp.result.results || []).slice(0, MAX_HITS)) pushCandidate(hit);
    }
  } else if (serp.markers.length > 0) {
    markBlocked("https://www.google.de/", serp.markers);
  }

  for (const hit of candidates.slice(0, MAX_HITS)) {
    if (isPortalOrLoginTitle(hit?.title)) continue;
    const hitCorpus = [hit?.title, hit?.summary, hit?.snippet].filter(Boolean).join(" ");
    if (!identityMatches(company, hitCorpus)) continue;
    if (isDirectoryHost(hit.url)) continue;

    let page = readPage(hit.url, country);
    if (!page?.ok) {
      const browserPage = readPageWithBrowser(hit.url);
      if (looksChallenged(browserPage.url, browserPage.page_text_excerpt, browserPage.markers)) {
        markBlocked(browserPage.url || hit.url, browserPage.markers);
        continue;
      }
      if (browserPage.ok) page = browserPage;
    }
    if (!page?.ok) continue;
    const finalUrl = page.url || hit.url;
    if (!safePublicHttpUrl(finalUrl)) continue;
    if (isPortalOrLoginTitle(page.title)) continue;
    if (!identityMatches(company, pageText(page))) continue;

    appendRecord(records, {
      field: "firma_name",
      value: company,
      confidence: "high",
      source_url: finalUrl,
      note: "Google result and company page confirm the company identity",
    });
    if (!records.some((record) => record.field === "firma_domain") && !isDirectoryHost(finalUrl)) {
      appendRecord(records, {
        field: "firma_domain",
        value: hostOf(finalUrl),
        confidence: "medium",
        source_url: finalUrl,
        note: `Google result for ${company}`,
      });
    }
    appendContactHeuristics(records, page, finalUrl);
    if (records.some((record) => record.field === "firma_domain")) break;
  }

  if (records.length > 0) {
    process.stdout.write(JSON.stringify({ records, source_session: { source_id: SOURCE_ID, session_id: serp.result.session_id, authenticated_login: serp.result.authenticated_login } }));
    return;
  }
  if (blocked) {
    recordUnlockSignal(blockedUrl, blockedMarkers.length > 0 ? blockedMarkers : ["access_challenge"]);
    process.stdout.write(JSON.stringify({
      records: [],
      failure_mode: blockedMarkers.includes("authorization_required") ? "authorization_required" : "blocked",
      detail: blockedMarkers.includes("authorization_required") ? "owner-bound Google authentication did not complete" : "google.de access challenge prevented provider evidence",
    }));
    return;
  }
  process.stdout.write(JSON.stringify({
    records: [],
    failure_mode: "portal_drift",
    detail: "authenticated Google search returned no extractable company records",
  }));
}

if (require.main === module) {
  main();
}

module.exports = {
  appendContactHeuristics,
  appendRecord,
  hostOf,
  identityMatches,
  identityTokens,
  isDirectoryHost,
  isPortalOrLoginTitle,
  looksChallenged,
  normalized,
  safePublicHttpUrl,
  serpBrowserSource,
};
