// Versioned successor to registered maps-google-com rev31. Requires #643.
// maps.google.com - provider-owned company detail extraction.
// rev0026 (relearned 2026-08-31): restore maps-specific extraction after the
// rev0025 generic overlay returned 0 records on consent-banner redirects.
// Patterned on the proven rev0022 maps-specific script with an extra consent
// button pass for the `ucbcb=1` Google consent banner, plus a small
// robustness pass on postal-code parsing for DE/AT/CH.

"use strict";

const { execFileSync } = require("child_process");

const SOURCE_ID = "maps.google.com";
const START_URL = "https://www.google.com/maps/search/?api=1";
const ALLOWED_FIELDS = new Set([
  "firma_name", "firma_anschrift", "firma_plz", "firma_ort", "firma_telefon", "firma_domain",
]);

function readInput() {
  try {
    return JSON.parse(process.env.CTOX_SCRAPE_INPUT_JSON || "{}");
  } catch (error) {
    process.stderr.write(`invalid CTOX_SCRAPE_INPUT_JSON: ${error.message}\n`);
    return {};
  }
}

function runCtox(args, input) {
  try {
    return JSON.parse(execFileSync(process.env.CTOX_BIN || "ctox", args, {
      encoding: "utf8",
      input,
      stdio: [input === undefined ? "ignore" : "pipe", "pipe", "pipe"],
      maxBuffer: 32 * 1024 * 1024,
      timeout: 95_000,
    }));
  } catch (error) {
    return { ok: false, command_error: String(error && error.stderr || error && error.message || "").slice(0, 4000) };
  }
}

function normalized(value) {
  return String(value || "")
    .normalize("NFKD").replace(/[\u0300-\u036f]/g, "")
    .toLocaleLowerCase("de-DE").replace(/\u00df/g, "ss")
    .replace(/[^a-z0-9]+/g, " ").trim();
}

const LEGAL_TOKENS = new Set(["ag", "co", "gmbh", "kg", "mbh", "se", "und"]);

function identityMatches(company, corpus) {
  const tokens = normalized(company).split(/\s+/)
    .filter((token) => token.length >= 3 && !LEGAL_TOKENS.has(token));
  const haystack = normalized(corpus);
  return tokens.length > 0 && tokens.every((token) => haystack.includes(token));
}

function isAllowedUrl(value) {
  try {
    const url = new URL(value);
    const host = url.hostname.toLowerCase();
    return url.protocol === "https:"
      && (host === "google.com" || host.endsWith(".google.com") || host === "google.de" || host.endsWith(".google.de"))
      && url.pathname.startsWith("/maps/");
  } catch (_error) {
    return false;
  }
}

function isConsentBannerUrl(value) {
  if (!isAllowedUrl(value)) return false;
  try {
    const url = new URL(value);
    return url.searchParams.has("ucbcb") || /\/consent/.test(url.pathname);
  } catch (_error) {
    return false;
  }
}

function blockingMarkers(page) {
  const detections = Array.isArray(page && page.detection && page.detection.markers) ? page.detection.markers.map(String) : [];
  const corpus = normalized([
    page && page.title, page && page.body_text, page && page.page_text_excerpt, page && page.command_error, detections.join(" "),
  ].filter(Boolean).join(" "));
  const markers = detections.filter((value) =>
    /captcha|challenge|blocked|human|rate.?limit|too.?many|consent.?wall|authorization_required/i.test(value)
  );
  for (const phrase of [
    "captcha", "unusual traffic", "verify you are human", "too many requests", "request blocked",
  ]) {
    if (corpus.includes(phrase)) markers.push(phrase.replace(/\s+/g, "-"));
  }
  return [...new Set(markers)];
}

function domainFromWebsite(value) {
  try {
    const url = new URL(value);
    const host = url.hostname.toLowerCase().replace(/^www\./, "");
    return url.protocol === "https:" && host && !host.includes("google.") ? host : null;
  } catch (_error) {
    return null;
  }
}

function isPortalTitle(value) {
  return /^(login|log in|anmelden|anmeldung|portal|startseite|home|willkommen)( |$)/.test(normalized(value));
}

function isMapsUiHeading(value) {
  return /^(results?|resultate?|ergebnisse?|resultats?|resultados?|risultati)$/.test(normalized(value));
}

function mapsAddressParts(value) {
  const full = String(value || "").replace(/^(?:Adresse|Address|Indirizzo|Dirección)\s*:\s*/i, "").replace(/\s+/g, " ").trim();
  const postal = full.match(/(?:^|[, ]+)(?:D-|A-|CH-)?(\d{4,5})\s+([^,]+)/);
  if (!postal) return { full, street: "", postal_code: "", locality: "" };
  const street = full.slice(0, postal.index).replace(/[ ,]+$/, "").trim();
  return { full, street, postal_code: postal[1], locality: postal[2].trim() };
}

function recordsFromPage(company, page) {
  const sourceUrl = page && page.url;
  if (page && page.ok === false || isPortalTitle(page && page.title) || !isAllowedUrl(sourceUrl)
      || !identityMatches(company, [page && page.title, page && page.body_text, page && page.page_text_excerpt].join(" "))) {
    return [];
  }
  const records = [];
  const seen = new Set();
  const push = (field, value, confidence = "high", note = "Google Maps detail panel") => {
    const clean = String(value || "").replace(/\s+/g, " ").trim();
    const key = `${field}\u0000${clean}`;
    if (!ALLOWED_FIELDS.has(field) || !clean || seen.has(key)) return;
    seen.add(key);
    records.push({ field, value: clean, confidence, source_url: sourceUrl, note });
  };
  const placeName = isMapsUiHeading(page && page.place_name) ? company : (page && page.place_name) || company;
  push("firma_name", placeName);
  for (const field of (page && page.extracted_fields && page.extracted_fields.fields) || []) {
    if (["firma_anschrift", "firma_plz", "firma_ort"].includes(field.field)) continue;
    push(field.field, field.value, field.confidence || "high", field.note || "Google Maps detail panel");
  }
  const address = mapsAddressParts(page && page.address);
  const addressNote = address.full ? `Google Maps displayed address: ${address.full}` : "Google Maps detail panel";
  push("firma_anschrift", address.street, "high", addressNote);
  push("firma_plz", address.postal_code || page && page.postal_code, "high", addressNote);
  // Maps' postal address label may name a borough (e.g. Nippes), not the municipality.
  // Keep the visible label in the address quotation, never promote it to firma_ort.
  push("firma_telefon", page && page.phone);
  push("firma_domain", domainFromWebsite(page && page.website));
  return records;
}

function browserPage(company, country) {
  const countryName = { DE: "Deutschland", AT: "Oesterreich", CH: "Schweiz" }[country] || country;
  const query = [company, countryName].filter(Boolean).join(", ");
  const targetUrl = `${START_URL}&hl=de&gl=${encodeURIComponent(String(country).toLowerCase())}&query=${encodeURIComponent(query)}`;
  // rev0026: extra consent patterns for the `ucbcb=1` Google consent banner,
  // plus a short settle wait after each click so the Maps app actually mounts.
  const consentPatterns = [
    "alle ablehnen", "reject all", "tout refuser", "alles ablehnen",
    "only reject", "nur ablehnen", "decline all", "use necessary cookies only",
    "non necessari", "nur erforderliche",
  ];
  const patternJson = JSON.stringify(consentPatterns);
  const companyJson = JSON.stringify(company);
  const targetUrlJson = JSON.stringify(targetUrl);
  const source = `
    const expectedCompany = ${companyJson};
    const response = await page.goto(${targetUrlJson}, { waitUntil: "domcontentloaded", timeout: 60000 });
    const consentSeen = /[?&]ucbcb=/.test(page.url()) || /\\/consent/.test(page.url());
    for (const pattern of ${patternJson}) {
      const button = page.getByRole("button", { name: new RegExp(pattern, "i") }).first();
      if (await button.count()) { await button.click({ timeout: 2500 }).catch(() => null); await page.waitForTimeout(500); break; }
    }
    if (consentSeen) {
      await page.goto(${targetUrlJson}, { waitUntil: "domcontentloaded", timeout: 60000 });
    }
    await page.waitForFunction(
      (value) => document.body && document.body.innerText.toLowerCase().includes(value.toLowerCase()),
      expectedCompany,
      { timeout: 30000 },
    ).catch(() => null);
    const exact = page.locator('a[href*="/maps/place/"]').filter({ hasText: expectedCompany }).first();
    if (await exact.count()) { await exact.click({ timeout: 5000 }).catch(() => null); await page.waitForTimeout(1500); }
    const text = await page.locator("body").innerText().catch(() => "");
    const attr = async (selector, name) => page.locator(selector).first().getAttribute(name).catch(() => "");
    const addressLabel = await attr('button[data-item-id="address"]', "aria-label") || "";
    const address = addressLabel.replace(/^(?:Adresse|Address):\\s*/i, "").trim();
    const phone = (await attr('button[data-item-id^="phone:tel:"]', "data-item-id") || "").replace(/^phone:tel:/, "");
    const website = await attr('a[data-item-id="authority"]', "href") || await attr('a[aria-label^="Website" i]', "href");
    const placeName = await page.locator("h1").first().innerText().catch(() => expectedCompany);
    const postal = address.match(/\\b(?:D-|A-|CH-)?(\\d{4,5})\\s+([^,]+)/);
    return {
      url: page.url(), title: await page.title().catch(() => ""), http_status: response && response.status(),
      body_text: text.slice(0, 30000), place_name: placeName, address, phone, website,
      postal_code: postal && postal[1] || "", locality: postal && postal[2] && postal[2].trim() || "",
      consent_seen: consentSeen,
    };
  `;
  const binding = readInput();
  if (!String(binding.owner_user_id || "").trim() || !String(binding.task_id || "").trim()) {
    return { ok: false, detection: { markers: ["authorization_required"] } };
  }
  const payload = runCtox([
    "business-os", "web-stack", "authenticated-automation",
    "--source-id", SOURCE_ID, "--target-url", "https://accounts.google.com/",
    "--credential-ref", "ctox-secret://credentials/GOOGLE_BROWSER_LOGIN",
    "--owner-user-id", String(binding.owner_user_id), "--task-id", String(binding.task_id),
    "--timeout-ms", "90000",
  ], source);
  if (!payload?.ok || !payload.session_id || payload.authenticated_login?.status !== "completed") {
    return { ok: false, detection: { markers: ["authorization_required"] } };
  }
  const result = payload;
  return Object.assign({}, result, {
    ok: payload && payload.ok === true,
    detection: payload && payload.detection,
    command_error: payload && payload.command_error,
  });
}

function recordUnlockSignal(url, markers) {
  runCtox([
    "web", "unlock", "signals", "record",
    "--source", `scrape-target:${SOURCE_ID}`,
    "--url", isAllowedUrl(url) ? url : START_URL,
    "--evidence", JSON.stringify({
      source_id: SOURCE_ID, detection: "access_challenge",
      markers: [...new Set(markers)].slice(0, 12), secret_value_in_payload: false,
    }),
  ]);
}

function failureResult(markers, matchingPageSeen = false) {
  return {
    records: [],
    failure_mode: markers.includes("authorization_required") ? "authorization_required" : markers.length > 0 ? "blocked" : matchingPageSeen ? "portal_drift" : "temporary_unreachable",
    detail: markers.length > 0
      ? "Google Maps challenge recorded for the generic web-unlock path"
      : matchingPageSeen
        ? "matching Google Maps detail panel did not expose current provider fields"
        : "no exact Google Maps company result",
  };
}

function main() {
  const input = readInput();
  const company = String(input.company || "").trim();
  const country = String(input.country || "DE").trim() || "DE";
  if (!company) {
    process.stdout.write(JSON.stringify({ records: [], failure_mode: "invalid_input", detail: "company missing" }));
    return;
  }
  const page = browserPage(company, country);
  const markers = blockingMarkers(page);
  const matchingPageSeen = isAllowedUrl(page && page.url) && identityMatches(company, [(page && page.title) || "", (page && page.body_text) || ""].join(" "));
  const records = recordsFromPage(company, page);
  if (records.length > 1) {
    process.stdout.write(JSON.stringify({ records, source_session: { source_id: SOURCE_ID, session_id: page.session_id, authenticated_login: page.authenticated_login } }));
    return;
  }
  if (markers.length > 0) recordUnlockSignal(page && page.url, markers);
  process.stdout.write(JSON.stringify(failureResult(markers, matchingPageSeen)));
}

if (require.main === module) main();

module.exports = {
  mapsAddressParts,
  blockingMarkers,
  domainFromWebsite,
  failureResult,
  identityMatches,
  isAllowedUrl,
  isConsentBannerUrl,
  isPortalTitle,
  isMapsUiHeading,
  recordsFromPage,
};

