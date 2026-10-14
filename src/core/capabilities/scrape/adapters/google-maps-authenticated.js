// Versioned successor to registered maps-google-com rev32.
// maps.google.com - provider-owned company detail extraction.
//
// Google answers the server IP's own Maps/search requests only with its robot
// check, signed in or not. Since 14.10.2026 (owner approval) the Google
// business profile is read through the Bright Data SERP API (synchronous
// `/request` endpoint, zone from the runtime store key
// CTOX_WEB_SEARCH_BRIGHTDATA_ZONE, default `serp_api1`): first the business
// panel of the Google web search for the company, then Google's local results
// (tbm=lcl). The Maps endpoint itself is disabled on the SERP zone ("disabled
// due to low success rate"). The key is read at runtime from the CTOX secret
// store (credentials/BRIGHTDATA_API_KEY) and never printed.

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

function ctoxBin() {
  return process.env.CTOX_BIN || "ctox";
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
    return ["https:", "http:"].includes(url.protocol) && host && !host.includes("google.") ? host : null;
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
  const panelNote = `Google Maps business profile (${(page && page.provider_section) || "detail panel"}, Google SERP via Bright Data)`
    + (page && page.opening_hours ? `; Öffnungszeiten: ${page.opening_hours}` : "");
  push("firma_name", placeName, "high", page && page.provider_section ? panelNote : "Google Maps detail panel");
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

const BRIGHTDATA_HOST = "api.brightdata.com";
const BRIGHTDATA_SECRET = { scope: "credentials", name: "BRIGHTDATA_API_KEY" };
const BRIGHTDATA_ZONE_KEY = "CTOX_WEB_SEARCH_BRIGHTDATA_ZONE";
const BRIGHTDATA_DEFAULT_ZONE = "serp_api1";
const SERP_ATTEMPT_TIMEOUT_MS = 40_000;
const SERP_BUDGET_MS = 100_000;

function brightDataToken() {
  try {
    const out = String(execFileSync(
      ctoxBin(),
      ["secret", "get", "--scope", BRIGHTDATA_SECRET.scope, "--name", BRIGHTDATA_SECRET.name],
      { encoding: "utf8", stdio: ["ignore", "pipe", "pipe"], timeout: 20_000 },
    ) || "").trim();
    if (!out.startsWith("{")) return out;
    const parsed = JSON.parse(out);
    return String(parsed.value || parsed.secret || parsed.data || "").trim();
  } catch {
    return "";
  }
}

// Zone name from the CTOX runtime store (read-only); the adapter env is
// cleared, so the store is read directly next to the scrape workspace.
function brightDataZone() {
  const targetDir = String(process.env.CTOX_SCRAPE_TARGET_DIR || "");
  if (targetDir) {
    try {
      const db = require("path").resolve(targetDir, "..", "..", "..", "ctox-runtime.sqlite3");
      const out = String(execFileSync("sqlite3", [
        "-readonly", db,
        `SELECT env_value FROM runtime_env_kv WHERE env_key = '${BRIGHTDATA_ZONE_KEY}' LIMIT 1`,
      ], { encoding: "utf8", stdio: ["ignore", "pipe", "pipe"], timeout: 5_000 }) || "").trim();
      if (/^[A-Za-z0-9_-]{1,64}$/.test(out)) return out;
    } catch {
      // fall through to the default zone
    }
  }
  return BRIGHTDATA_DEFAULT_ZONE;
}

// Variants differ only in the result count: Bright Data refuses to repeat a
// URL that "recently failed" for a while, so a retry asks a different URL.
const NUM_VARIANTS = ["10", "20", ""];

function googleSerpUrl(query, country, local = false, variant = 0) {
  const gl = /^[A-Z]{2}$/.test(country || "") ? country.toLowerCase() : "de";
  const params = { q: query, hl: "de", gl, brd_json: "1" };
  const num = NUM_VARIANTS[variant % NUM_VARIANTS.length];
  if (num) params.num = num;
  if (local) params.tbm = "lcl";
  return "https://www.google.de/search?" + new URLSearchParams(params).toString();
}

// One synchronous SERP request. NOTE: no `Accept: application/json` header —
// with it Bright Data answers HTTP 200 with an empty body.
function brightDataRequest(token, zone, url, timeoutMs = SERP_ATTEMPT_TIMEOUT_MS) {
  return new Promise((resolve) => {
    const body = JSON.stringify({ zone, url, format: "raw" });
    let settled = false;
    const done = (value) => { if (!settled) { settled = true; resolve(value); } };
    let req;
    try {
      req = require("https").request({
        host: BRIGHTDATA_HOST,
        path: "/request",
        method: "POST",
        headers: {
          Authorization: `Bearer ${token}`,
          "Content-Type": "application/json",
          "Content-Length": Buffer.byteLength(body),
        },
        timeout: timeoutMs,
      }, (res) => {
        let raw = "";
        res.setEncoding("utf8");
        res.on("data", (chunk) => { raw += chunk; });
        res.on("end", () => done({ status: res.statusCode || 0, body: raw }));
        res.on("error", (error) => done({ status: 0, body: "", error: String(error?.message || error) }));
      });
    } catch (error) {
      done({ status: 0, body: "", error: String(error?.message || error) });
      return;
    }
    req.on("timeout", () => { req.destroy(new Error("no answer within the request budget")); });
    req.on("error", (error) => done({ status: 0, body: "", error: String(error?.message || error) }));
    req.write(body);
    req.end();
  });
}

function parseSerp(response) {
  if (response.status !== 200) {
    return { ok: false, status: response.status, error: String(response.error || response.body || "").slice(0, 200) };
  }
  let parsed = null;
  try { parsed = JSON.parse(response.body); } catch { parsed = null; }
  if (!parsed || typeof parsed !== "object") {
    return {
      ok: false,
      status: 200,
      error: response.body
        ? `unparsable SERP body (${response.body.length} bytes: ${response.body.slice(0, 80).replace(/\s+/g, " ")})`
        : "empty SERP body",
    };
  }
  // Target policy: only results reported as coming from Google count.
  if (String(parsed.general?.search_engine || "").toLowerCase() !== "google") {
    return { ok: false, status: 200, error: "SERP provider is not google" };
  }
  const knowledge = parsed.knowledge && typeof parsed.knowledge === "object" ? parsed.knowledge : null;
  const local = (Array.isArray(parsed.snack_pack) ? parsed.snack_pack : [])
    .concat(Array.isArray(parsed.local_results) ? parsed.local_results : [])
    .filter((item) => item && typeof item === "object");
  return { ok: true, status: 200, knowledge, local };
}

const sleep = (ms) => new Promise((resolve) => setTimeout(resolve, ms));

// Bright Data intermittently answers HTTP 200 with an empty or non-JSON body
// ("This query recently failed ..."), times out, or answers a transient 5xx
// (measured 14.10.2026); such answers are retried with a URL variant while the
// budget lasts. Authorization and account answers are final.
async function brightSerp(token, zone, query, country, local = false, budgetMs = SERP_BUDGET_MS, attempts = 3) {
  const deadline = Date.now() + budgetMs;
  let last = { ok: false, status: 0, error: "no attempt" };
  for (let attempt = 0; attempt < attempts; attempt += 1) {
    const remaining = deadline - Date.now();
    if (remaining < 4_000) break;
    last = parseSerp(await brightDataRequest(token, zone, googleSerpUrl(query, country, local, attempt), Math.min(remaining, SERP_ATTEMPT_TIMEOUT_MS)));
    if (last.ok || [400, 401, 403, 407].includes(last.status)) return last;
    await sleep(400 + attempt * 600);
  }
  return last;
}

function serpFailure(searches) {
  const statuses = searches.map((search) => search.status);
  const detail = `Bright Data SERP failed: ${searches.map((search) => `${search.status}${search.error ? ` ${search.error}` : ""}`).join(" | ").slice(0, 400)}`;
  if (statuses.some((status) => status === 401 || status === 403 || status === 407)) {
    return { records: [], failure_mode: "authorization_required", detail };
  }
  return { records: [], failure_mode: "temporary_unreachable", detail };
}

function hoursText(value) {
  if (!Array.isArray(value)) return "";
  return value
    .map((item) => [item?.day, item?.hours].map((part) => String(part || "").trim()).filter(Boolean).join(" "))
    .filter(Boolean)
    .join("; ")
    .slice(0, 300);
}

// "https://www.google.de/maps/dir//SGS+Industrial+Services,+Gewerbepark+1,+4751+Dorf+an+der+Pram/data=..."
// -> "Gewerbepark 1, 4751 Dorf an der Pram"
function addressFromMapsLink(link, name) {
  try {
    const url = new URL(link);
    const match = url.pathname.match(/\/maps\/dir\/\/([^/]+)/);
    if (!match) return "";
    const parts = decodeURIComponent(match[1].replace(/\+/g, " ")).split(",").map((part) => part.trim()).filter(Boolean);
    if (parts.length > 1 && normalized(parts[0]) === normalized(name)) parts.shift();
    return /\d{4,5}\s+\S/.test(parts.join(", ")) ? parts.join(", ") : "";
  } catch (_error) {
    return "";
  }
}

function mapsSearchUrl(company, country) {
  const countryName = { DE: "Deutschland", AT: "Oesterreich", CH: "Schweiz" }[country] || country;
  const query = [company, countryName].filter(Boolean).join(", ");
  return `${START_URL}&hl=de&gl=${encodeURIComponent(String(country).toLowerCase())}&query=${encodeURIComponent(query)}`;
}

function placeFromKnowledge(company, knowledge, sourceUrl) {
  if (!knowledge || !identityMatches(company, knowledge.name)) return null;
  const address = String(knowledge.address || "").trim();
  const hours = hoursText(knowledge.open_hours);
  return {
    ok: true,
    url: sourceUrl,
    title: String(knowledge.name || ""),
    body_text: [knowledge.name, knowledge.subtitle, address].filter(Boolean).join(" "),
    place_name: String(knowledge.name || ""),
    address,
    phone: String(knowledge.phone || ""),
    website: String(knowledge.site || ""),
    opening_hours: hours,
    provider_section: "business panel",
  };
}

function placeFromLocal(company, items, sourceUrl) {
  const item = (items || []).find((entry) => identityMatches(company, entry.name || entry.title));
  if (!item) return null;
  const name = String(item.name || item.title || "");
  const address = addressFromMapsLink(item.maps_link, name)
    || (/\d{4,5}\s+\S/.test(String(item.address || "")) ? String(item.address) : "");
  return {
    ok: true,
    url: sourceUrl,
    title: name,
    body_text: [name, item.type, address || item.address].filter(Boolean).join(" "),
    place_name: name,
    address,
    phone: String(item.phone || ""),
    website: String(item.site || item.website || item.link || ""),
    opening_hours: "",
    provider_section: "local results",
  };
}

// Business profile lookup: web-search business panel first, local results second.
async function placeLookup(company, country, city) {
  const token = brightDataToken();
  if (!token) {
    return { ok: false, failure: { records: [], failure_mode: "authorization_required", detail: `credential_missing: ${BRIGHTDATA_SECRET.name} not in CTOX secret store (scope ${BRIGHTDATA_SECRET.scope})` } };
  }
  const zone = brightDataZone();
  const sourceUrl = mapsSearchUrl(company, country);
  const query = [company, city].filter(Boolean).join(" ");
  // Both lookups run in parallel; the business panel wins when it names the company.
  const [web, local] = await Promise.all([
    brightSerp(token, zone, query, country),
    brightSerp(token, zone, query, country, true),
  ]);
  const searches = [{ status: web.status, error: web.error }, { status: local.status, error: local.error }];
  if ([web, local].some((serp) => !serp.ok && [400, 401, 403, 407].includes(serp.status))) {
    return { ok: false, failure: serpFailure(searches) };
  }
  const place = (web.ok && placeFromKnowledge(company, web.knowledge, sourceUrl))
    || (local.ok && placeFromLocal(company, local.local, sourceUrl));
  if (place) return place;
  if (!web.ok && !local.ok) return { ok: false, failure: serpFailure(searches) };
  return { ok: false, url: sourceUrl };
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

async function run(input) {
  const company = String(input.company || "").trim();
  const country = String(input.country || "DE").trim().toUpperCase() || "DE";
  const city = String(input.city || input.firma_ort || input.ort || "").trim();
  if (!company) {
    return { records: [], failure_mode: "invalid_input", detail: "company missing" };
  }
  const page = await placeLookup(company, country, city);
  if (page.failure) return page.failure;
  const records = recordsFromPage(company, page);
  if (records.length > 1) {
    return { records, source_session: { source_id: SOURCE_ID, provider: "google", transport: "brightdata_serp" } };
  }
  const matchingPageSeen = page.ok === true;
  return failureResult([], matchingPageSeen);
}

async function main() {
  let result;
  try {
    result = await run(readInput());
  } catch (error) {
    result = { records: [], failure_mode: "temporary_unreachable", detail: String(error && error.message || error).slice(0, 200) };
  }
  process.stdout.write(JSON.stringify(result));
}

if (require.main === module) main();

module.exports = {
  addressFromMapsLink,
  googleSerpUrl,
  hoursText,
  mapsAddressParts,
  mapsSearchUrl,
  parseSerp,
  placeFromKnowledge,
  placeFromLocal,
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

