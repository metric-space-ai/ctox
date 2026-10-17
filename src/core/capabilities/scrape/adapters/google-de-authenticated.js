// Versioned successor to registered google-de rev32 (and the reverted
// dataset-based rev33-36).
// google.de — prospect.v1 extractor.
//
// Reads CTOX_SCRAPE_INPUT_JSON for the company + country. The Google SERP is
// fetched through the Bright Data SERP API (synchronous `/request` endpoint,
// zone from the runtime store key CTOX_WEB_SEARCH_BRIGHTDATA_ZONE, default
// `serp_api1`) because Google answers the server IP's own searches only with
// its robot check, signed in or not (owner approval 14.10.2026). The key is
// read at runtime from the CTOX secret store (credentials/BRIGHTDATA_API_KEY)
// and never printed.
//
// Company mode: discover the company's own website among the Google organic
// results, read it with the CTOX web stack and parse the field set documented
// in `tools/web-stack/src/sources/EXCEL_MATRIX.md`.
// Person mode (input.titles, input.title_groups or input.query): run
// `linkedin "<company>" <title(s)>` Google searches in parallel and return
// LinkedIn profile candidates whose hit names the company.
//
// The Google leg only accepts payloads whose search engine is reported as
// `google` (foreign provider results are rejected by target policy) and only
// emits evidence whose source URL is a safe public http(s) URL — result URLs
// may be external (the company's own site).
//
// Drift contract: if the SERP answers but nothing matches, this script returns
// an empty records array with failure_mode portal_drift.

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

function runBrowserAutomation(source, timeoutMs = 60_000) {
  const payload = runCtox(["web", "browser-automation", "--timeout-ms", String(timeoutMs)], source, timeoutMs + 10_000);
  const markers = Array.isArray(payload?.detection?.markers) ? payload.detection.markers.map(String) : [];
  if (!payload?.ok) return { ok: false, markers };
  return { ok: true, markers, result: payload.result || {} };
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
  const payload = runBrowserAutomation(source, 50_000);
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
// Bright Data SERP API (Google results without the server IP's robot check)
// ---------------------------------------------------------------------------

const BRIGHTDATA_HOST = "api.brightdata.com";
const BRIGHTDATA_SECRET = { scope: "credentials", name: "BRIGHTDATA_API_KEY" };
const BRIGHTDATA_ZONE_KEY = "CTOX_WEB_SEARCH_BRIGHTDATA_ZONE";
const BRIGHTDATA_DEFAULT_ZONE = "serp_api1";
const SERP_ATTEMPT_TIMEOUT_MS = 40_000;
const SERP_BUDGET_MS = 100_000;
const PERSON_DEADLINE_MS = 55_000;
const MAX_PERSON_QUERIES = 20;

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

function googleSerpUrl(query, country, variant = 0) {
  const gl = /^[A-Z]{2}$/.test(country || "") ? country.toLowerCase() : "de";
  const params = { q: query, hl: "de", gl, brd_json: "1" };
  const num = NUM_VARIANTS[variant % NUM_VARIANTS.length];
  if (num) params.num = num;
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
  const organic = (Array.isArray(parsed.organic) ? parsed.organic : []).map((hit) => ({
    url: String(hit?.link || hit?.url || ""),
    title: String(hit?.title || ""),
    summary: String(hit?.description || hit?.snippet || ""),
  })).filter((hit) => hit.url);
  const knowledge = parsed.knowledge && typeof parsed.knowledge === "object" ? parsed.knowledge : null;
  return { ok: true, status: 200, results: organic, knowledge };
}

const sleep = (ms) => new Promise((resolve) => setTimeout(resolve, ms));

// Bright Data intermittently answers HTTP 200 with an empty or non-JSON body
// ("This query recently failed ..."), times out, or answers a transient 5xx
// (measured 14.10.2026); such answers are retried with a URL variant while the
// budget lasts. Authorization and account answers are final.
async function brightSerp(token, zone, query, country, budgetMs = SERP_BUDGET_MS, attempts = 3) {
  const deadline = Date.now() + budgetMs;
  let last = { ok: false, status: 0, error: "no attempt" };
  for (let attempt = 0; attempt < attempts; attempt += 1) {
    const remaining = deadline - Date.now();
    if (remaining < 4_000) break;
    last = parseSerp(await brightDataRequest(token, zone, googleSerpUrl(query, country, attempt), Math.min(remaining, SERP_ATTEMPT_TIMEOUT_MS)));
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

// ---------------------------------------------------------------------------
// Person discovery (LinkedIn candidates from Google)
// ---------------------------------------------------------------------------

// "Vorname Nachname – Titel – Firma | LinkedIn" -> { name, headline }
function parseLinkedinTitle(title) {
  const cleaned = String(title || "").replace(/\s*[|·–-]\s*LinkedIn.*$/i, "").trim();
  const parts = cleaned.split(/\s+[–—·-]\s+/).map((part) => part.trim()).filter(Boolean);
  const name = (parts[0] || "").replace(/,.*$/, "").replace(/\b(Dr\.|Prof\.|Dipl\.-\S+|MBA|M\.Sc\.|B\.Sc\.)\s*/g, "").trim();
  // Drop location parts ("Münster, Nordrhein-Westfalen, Deutschland").
  const headline = parts.slice(1)
    .filter((part) => !/(?:^|,\s*)(?:Deutschland|Germany|Österreich|Austria|Schweiz|Switzerland|Region)\b|\bRegion\s/i.test(part))
    .join(" – ");
  return { name, headline };
}

function linkedinProfileUrl(value) {
  try {
    const url = new URL(value);
    if (!/(^|\.)linkedin\.com$/i.test(url.hostname)) return "";
    const match = url.pathname.match(/^\/in\/([^/?#]+)/);
    if (!match) return "";
    let slug = match[1];
    try { slug = decodeURIComponent(slug); } catch { /* keep the raw slug */ }
    return `https://www.linkedin.com/in/${encodeURIComponent(slug.toLowerCase())}`;
  } catch {
    return "";
  }
}

// Search name without legal form: "Kiesow Dr. Brinkmann GmbH & Co. KG" -> "Kiesow Dr. Brinkmann".
function searchName(company) {
  return String(company || "")
    .replace(/\b(GmbH|mbH|AG|SE|KG|KGaA|OHG|UG|e\.?\s?K\.?|Ltd\.?|Inc\.?|Holding)(?=\s|$|[,.&])/gi, " ")
    .replace(/&\s*Co\.?/gi, " ")
    .replace(/\(haftungsbeschr[aä]nkt\)/gi, " ")
    .replace(/[,&]+/g, " ")
    .replace(/\s+/g, " ")
    .trim() || String(company || "").trim();
}

function quoteTerm(term) {
  return /\s/.test(term) ? `"${term.replace(/"/g, "")}"` : term;
}

// A free query that lists the company with all its legal form plus every
// title ("linkedin Hüttenes-Albertus Chemische Werke GmbH Supply Chain
// Management Leiter Logistik") makes Google AND all words and returns no
// profile. Measured 15.10.2026: the same company and titles as
// site:linkedin.com/in "Hüttenes-Albertus" ("Supply" OR "Logistik" …)
// returned 9 employee profiles. So every free query also runs in this form.
const GENERIC_TITLE_WORDS = new Set([
  "linkedin", "xing", "site", "com", "in", "leiter", "leiterin", "head", "of", "manager", "managerin",
  "management", "director", "direktor", "senior", "vice", "president", "chief", "officer", "und", "and",
  "oder", "or", "der", "die", "das", "bei", "the", "for", "fur", "von", "deutschland", "germany",
]);

// LinkedIn writes "Budenheim", not "Chemische Fabrik Budenheim KG": search
// and filter on the most distinctive word of the name instead of the first.
const GENERIC_COMPANY_WORDS = new Set([
  "chemische", "chemie", "fabrik", "werke", "werk", "germany", "deutschland", "gruppe", "group",
  "international", "industries", "industrie", "gesellschaft", "verwaltung", "service", "services",
]);

function distinctiveWord(company) {
  const words = searchName(company).split(/\s+/)
    .filter((word) => normalized(word).replace(/\s+/g, "").length >= 3 && !GENERIC_COMPANY_WORDS.has(normalized(word)));
  return words.find((word) => word.length >= 5)
    || words.slice(0, 3).reduce((best, word) => (word.length > best.length ? word : best), "")
    || searchName(company);
}

function linkedinSiteQuery(query, company, shortName) {
  if (/\bsite:/i.test(query)) return "";
  const companyWords = new Set([...normalized(company).split(/\s+/), ...LEGAL_TOKENS]);
  const terms = [];
  for (const word of normalized(query).split(/\s+/)) {
    if (word.length < 3 || companyWords.has(word) || GENERIC_TITLE_WORDS.has(word) || terms.includes(word)) continue;
    terms.push(word);
  }
  const name = distinctiveWord(company) || shortName;
  if (terms.length === 0) return `site:linkedin.com/in "${name}"`;
  return `site:linkedin.com/in "${name}" (${terms.slice(0, 8).join(" OR ")})`;
}

function personQueries(input, company) {
  const queries = [];
  const shortName = searchName(company);
  if (String(input.query || "").trim()) {
    queries.push(String(input.query).trim());
    const site = linkedinSiteQuery(String(input.query), company, shortName);
    if (site) queries.push(site);
  }
  const titles = Array.isArray(input.titles) ? input.titles.map((t) => String(t || "").trim()).filter(Boolean) : [];
  for (const title of titles) queries.push(`linkedin "${shortName}" ${quoteTerm(title)}`);
  // title_groups: one search per group, titles OR-combined.
  const groups = Array.isArray(input.title_groups) ? input.title_groups : [];
  for (const group of groups) {
    const list = (Array.isArray(group) ? group : [group]).map((t) => String(t || "").trim()).filter(Boolean).slice(0, 8);
    if (list.length > 0) queries.push(`linkedin "${shortName}" (${list.map(quoteTerm).join(" OR ")})`);
  }
  return [...new Set(queries)].slice(0, MAX_PERSON_QUERIES);
}

function wantsPersonMode(input) {
  return Boolean(String(input.query || "").trim())
    || (Array.isArray(input.titles) && input.titles.length > 0)
    || (Array.isArray(input.title_groups) && input.title_groups.length > 0);
}

async function personSearch(input, company, country, token, zone) {
  const queries = personQueries(input, company);
  const firstToken = normalized(distinctiveWord(company)).split(/\s+/)[0] || identityTokens(company)[0];
  if (queries.length === 0 || !firstToken) {
    return { records: [], failure_mode: "invalid_input", detail: "person search needs a company name token and at least one title or query" };
  }
  const deadline = Date.now() + PERSON_DEADLINE_MS;
  const answers = await Promise.all(queries.map((query) =>
    brightSerp(token, zone, query, country, Math.max(5_000, deadline - Date.now()))));
  const records = [];
  const searches = [];
  const seen = new Set();
  queries.forEach((query, index) => {
    const serp = answers[index];
    searches.push({ query, ok: serp.ok, status: serp.status, hits: serp.ok ? serp.results.length : 0, ...(serp.ok ? {} : { error: serp.error }) });
    if (!serp.ok) return;
    for (const hit of serp.results) {
      const profile = linkedinProfileUrl(hit.url);
      if (!profile || seen.has(profile)) continue;
      // LinkedIn shortens company names in titles ("… bei KIESOW …"): the
      // first distinctive name token is enough for a candidate; confirmation
      // of the current employer is left to linkedin-com.
      if (!normalized([hit.title, hit.summary].join(" ")).includes(firstToken)) continue;
      const { name, headline } = parseLinkedinTitle(hit.title);
      const nameParts = name.split(/\s+/).filter(Boolean);
      if (nameParts.length < 2) continue;
      seen.add(profile);
      const note = `Google (Bright Data SERP) "${query}": ${hit.title}${hit.summary ? ` — ${hit.summary.slice(0, 220)}` : ""}`;
      appendRecord(records, { field: "person_vorname", value: nameParts.slice(0, -1).join(" "), confidence: "medium", source_url: profile, note });
      appendRecord(records, { field: "person_nachname", value: nameParts[nameParts.length - 1], confidence: "medium", source_url: profile, note });
      if (headline) appendRecord(records, { field: "person_position", value: headline.slice(0, 200), confidence: "low", source_url: profile, note });
      appendRecord(records, { field: "person_linkedin", value: profile, confidence: "medium", source_url: profile, note });
    }
  });
  if (records.length > 0) return { records, searches };
  if (searches.every((search) => !search.ok)) return { ...serpFailure(searches), searches };
  return { records: [], failure_mode: "portal_drift", detail: "no LinkedIn profile in the Google results named the company for the searched titles", searches };
}

// ---------------------------------------------------------------------------
// Company discovery
// ---------------------------------------------------------------------------

async function companySearch(company, country, token, zone) {
  const serp = await brightSerp(token, zone, company, country);
  if (!serp.ok) return serpFailure([{ status: serp.status, error: serp.error }]);

  const records = [];
  let blocked = false;
  let blockedUrl = "";
  const blockedMarkers = [];
  const markBlocked = (url, markers) => {
    blocked = true;
    blockedUrl ||= url;
    blockedMarkers.push(...(markers || []));
  };

  const candidates = [];
  const seen = new Set();
  const pushCandidate = (hit) => {
    const url = String(hit?.url || "").trim();
    if (!url || seen.has(url) || !safePublicHttpUrl(url)) return;
    seen.add(url);
    candidates.push(hit);
  };
  // Google's own business panel names the official website; it counts as a
  // candidate only when the panel names the company.
  const knowledge = serp.knowledge;
  if (knowledge?.site && identityMatches(company, knowledge.name)) {
    pushCandidate({ url: String(knowledge.site), title: String(knowledge.name || ""), summary: String(knowledge.summary || ""), fromPanel: true });
  }
  for (const hit of serp.results.slice(0, MAX_HITS)) pushCandidate(hit);

  for (const hit of candidates.slice(0, MAX_HITS + 1)) {
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
    // Identity: the page names the company; or, for the website Google's own
    // business panel links to that company, the page carries its first
    // distinctive name token (homepages rarely spell out the legal name).
    const panelConfirmed = hit.fromPanel === true
      && hostOf(finalUrl) === hostOf(hit.url)
      && normalized(pageText(page)).includes(identityTokens(company)[0]);
    if (!identityMatches(company, pageText(page)) && !panelConfirmed) continue;

    appendRecord(records, {
      field: "firma_name",
      value: company,
      confidence: "high",
      source_url: finalUrl,
      note: panelConfirmed && !identityMatches(company, pageText(page))
        ? `Google business panel "${hit.title}" links this website`
        : "Google result and company page confirm the company identity",
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
    // The legal notice carries the published contact data.
    try {
      const imprintUrl = new URL("/impressum", finalUrl).toString();
      if (imprintUrl !== finalUrl) {
        const imprint = readPage(imprintUrl, country);
        const imprintFinal = imprint?.url || imprintUrl;
        if (imprint?.ok && hostOf(imprintFinal) === hostOf(finalUrl) && safePublicHttpUrl(imprintFinal)) {
          appendContactHeuristics(records, imprint, imprintFinal);
        }
      }
    } catch {
      // no legal notice page; homepage evidence stands
    }
    if (records.some((record) => record.field === "firma_domain")) break;
  }

  if (records.length > 0) {
    return { records, source_session: { source_id: SOURCE_ID, provider: "google", transport: "brightdata_serp" } };
  }
  if (blocked) {
    recordUnlockSignal(blockedUrl, blockedMarkers.length > 0 ? blockedMarkers : ["access_challenge"]);
    return { records: [], failure_mode: "blocked", detail: "company page access challenge prevented evidence" };
  }
  return {
    records: [],
    failure_mode: "portal_drift",
    detail: `Google search returned no extractable company records (${serp.results.length} organic results)`,
  };
}

// ---------------------------------------------------------------------------
// main
// ---------------------------------------------------------------------------

async function run(input) {
  const company = String(input.company || "").trim();
  const country = String(input.country || "").trim().toUpperCase();
  if (!company) {
    return { records: [], failure_mode: "portal_drift", detail: "CTOX_SCRAPE_INPUT_JSON.company missing" };
  }
  const token = brightDataToken();
  if (!token) {
    return { records: [], failure_mode: "authorization_required", detail: `credential_missing: ${BRIGHTDATA_SECRET.name} not in CTOX secret store (scope ${BRIGHTDATA_SECRET.scope})` };
  }
  const zone = brightDataZone();
  if (wantsPersonMode(input)) return personSearch(input, company, country, token, zone);
  return companySearch(company, country, token, zone);
}

async function main() {
  let result;
  try {
    result = await run(readInput());
  } catch (error) {
    result = { records: [], failure_mode: "temporary_unreachable", detail: String(error?.message || error).slice(0, 200) };
  }
  process.stdout.write(JSON.stringify(result));
}

if (require.main === module) {
  main();
}

module.exports = {
  appendContactHeuristics,
  appendRecord,
  googleSerpUrl,
  hostOf,
  identityMatches,
  identityTokens,
  isDirectoryHost,
  isPortalOrLoginTitle,
  linkedinProfileUrl,
  looksChallenged,
  normalized,
  parseLinkedinTitle,
  parseSerp,
  personQueries,
  safePublicHttpUrl,
  searchName,
};
