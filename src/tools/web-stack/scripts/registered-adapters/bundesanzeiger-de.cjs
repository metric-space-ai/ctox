// bundesanzeiger.de - direct Playwright extractor for prospect.v1.
//
// Repaired 2026-07-29 against the LIVE site (see solo/probe.mjs for the
// standalone plain-Playwright proof):
//  - consent banner is clicked via role/name patterns (exact label first),
//    tolerating late-rendering banners instead of requiring count === 1;
//  - the search submit is honeypot-aware: the form contains a hidden
//    decoy input[name="search-button"] (tabindex -1, no value); only the
//    visible input[name="search-button"][value="Suchen"] is clicked;
//  - an explicit "keine passenden Daten gefunden" page is recognised as a
//    real (empty) results page instead of being misread as drift;
//  - one resubmit retry covers Wicket session bounces back to the form;
//  - the per-document "Sicherheitsabfrage" (image CAPTCHA) that gates every
//    Rechnungslegung document is treated as an access challenge and is
//    NEVER opened or solved; extraction stays on the public results table.
//
// Drift contract: if the selectors below stop matching but a result page
// loads successfully, this script returns an empty records array with
// failure_mode "portal_drift" - never a crash.

"use strict";

const { execFileSync } = require("child_process");
const fs = require("node:fs");
const path = require("node:path");
const { createHash } = require("node:crypto");

const ALLOWED_HOST = "bundesanzeiger.de";
const SEARCH_URL = "https://www.bundesanzeiger.de/pub/de/suche?0";
const BROWSER_TIMEOUT_MS = 120_000;

function readInput(raw = process.env.CTOX_SCRAPE_INPUT_JSON) {
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

function runCtox(args, input) {
  try {
    const out = execFileSync(ctoxBin(), args, {
      encoding: "utf8",
      stdio: ["pipe", "pipe", "pipe"],
      input,
      timeout: BROWSER_TIMEOUT_MS + 10_000,
      maxBuffer: 32 * 1024 * 1024,
    });
    return JSON.parse(out);
  } catch {
    return null;
  }
}

function recordUnlockSignal(url, markers) {
  const safeUrl = allowedSourceUrl(url);
  return runCtox([
    "web",
    "unlock",
    "signals",
    "record",
    "--source",
    "scrape-target:bundesanzeiger.de",
    "--url",
    safeUrl?.href || SEARCH_URL,
    "--evidence",
    JSON.stringify({
      source_id: "bundesanzeiger.de",
      detection: "access_challenge",
      markers: [...new Set((markers || []).map(String))].slice(0, 12),
      secret_value_in_payload: false,
    }),
  ]);
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

function normalizeCompanyName(value) {
  return String(value || "")
    .toLocaleLowerCase("de-DE")
    .normalize("NFKD")
    .replace(/\p{M}/gu, "")
    .replace(/\s*\((?:vormals|ehemals|fruher|früher)\s*:[^)]+\)\s*$/i, "")
    .replace(/[^a-z0-9]+/g, " ")
    .trim()
    .replace(/\s+/g, " ");
}

function entryMatchesCompany(company, entry) {
  const expected = normalizeCompanyName(company);
  const actual = normalizeCompanyName(entry && entry.name);
  return expected.length > 0 && actual === expected;
}

function selectMatchingEntry(company, entries) {
  return (
    (Array.isArray(entries) ? entries : []).find((entry) => entryMatchesCompany(company, entry)) ||
    null
  );
}

function requestedQuery(input) {
  const value = Object.prototype.hasOwnProperty.call(input, "query") ? input.query : input.company;
  return typeof value === "string" ? value.trim() : "";
}

// Only a response to this form submission (or its redirect chain) can prove
// query completion. An initial home-page response or analytics request cannot.
function createQueryTrace(query, origin, initialFrame) {
  let current = null;
  const safeUrl = (value) => {
    const url = new URL(value);
    return url.origin === origin && !url.username && !url.password;
  };
  return {
    capture(event, data) {
      try {
        if (event === "Network.requestWillBeSent") {
          if (data.type !== "Document" || data.frameId !== initialFrame.id) return;
          const same = current && current.request_id === data.requestId;
          const previousUrl = same ? current.url : null;
          if (!same) current = { request_id: data.requestId, loader_id: data.loaderId,
            valid: typeof data.requestId === "string" && data.requestId.length > 0
              && typeof data.loaderId === "string" && data.loaderId.length > 0
              && data.loaderId !== initialFrame.loaderId,
            query_found: false, hops: 0, finished: false, response: null };
          current.hops += 1;
          const url = new URL(data.request.url);
          const values = [...url.searchParams.getAll("fulltext"),
            ...new URLSearchParams(data.request.postData || "").getAll("fulltext")];
          current.valid &&= current.hops <= 8 && safeUrl(url.href)
            && data.loaderId === current.loader_id
            && !(data.request.hasPostData && typeof data.request.postData !== "string")
            && values.length <= 1 && (values.length === 0 || values[0] === query)
            && (same ? Boolean(data.redirectResponse)
              && data.redirectResponse.url === previousUrl
              && data.redirectResponse.status >= 300 && data.redirectResponse.status < 400
              : !data.redirectResponse);
          current.query_found ||= values.length === 1;
          current.url = url.href;
          current.finished = false;
          current.response = null;
        } else if (current && data.requestId === current.request_id) {
          if (event === "Network.responseReceived") {
            const response = data.response;
            current.valid &&= data.type === "Document" && data.frameId === initialFrame.id
              && data.loaderId === current.loader_id && safeUrl(response.url)
              && response.url === current.url && Number.isInteger(response.status)
              && response.status >= 200 && response.status < 300
              && !response.fromServiceWorker && !response.fromDiskCache;
            current.response = { url: response.url, status: response.status };
          } else if (event === "Network.loadingFinished") current.finished = true;
          else if (event === "Network.loadingFailed") current.valid = false;
        }
      } catch {
        // Missing or unsupported network data cannot establish completion.
        if (current) current.valid = false;
      }
    },
    snapshot(frame) {
      if (!current?.valid || !current.query_found || !current.finished || !current.response
          || frame.id !== initialFrame.id || frame.loaderId !== current.loader_id
          || frame.url !== current.response.url) return null;
      return { ...current.response, request_id: current.request_id, loader_id: current.loader_id };
    },
  };
}

function isCompletedEmptyQuery(company, result) {
  if (!normalizeCompanyName(company) || !result || result.blocked !== false || result.results_page !== true
      || result.query_completed !== true || !Number.isInteger(result.http_status)
      || result.http_status < 200 || result.http_status >= 300) return false;
  const pageUrl = allowedSourceUrl(result.url);
  const responseUrl = allowedSourceUrl(result.response_url);
  const origin = new URL(SEARCH_URL).origin;
  if (!pageUrl || !responseUrl || pageUrl.origin !== origin || responseUrl.origin !== origin) return false;
  if (!Array.isArray(result.entries)
      || result.entries.some((entry) => !entry || typeof entry.name !== "string" || !entry.name.trim())
      || selectMatchingEntry(company, result.entries)) return false;
  if (result.no_results === true) return result.entries.length === 0;
  return result.entries.length > 0;
}

function writeQueryCompletion(rawInput, company, result, env = process.env) {
  const input = JSON.parse(rawInput);
  const query = requestedQuery(input);
  if (!query || result.query !== query || typeof input.company !== "string"
      || input.company.trim() !== company || !isCompletedEmptyQuery(company, result)) {
    throw new Error("current input and completed provider query do not match");
  }
  const runDir = env.CTOX_SCRAPE_RUN_DIR;
  if (!runDir || !path.isAbsolute(runDir) || !fs.lstatSync(runDir).isDirectory()
      || !env.CTOX_SCRAPE_TARGET_KEY?.trim()) throw new Error("native run context missing");
  const receipt = {
    schema: "ctox.scrape.query_completion.v1",
    run_id: path.basename(runDir),
    target_key: env.CTOX_SCRAPE_TARGET_KEY,
    input_sha256: createHash("sha256").update(rawInput, "utf8").digest("hex"),
    query,
    provider_url: result.response_url,
    http_status: result.http_status,
    completed: true,
    results_page: true,
    blocked: false,
    matched_records: 0,
    observed_records: result.entries.length,
    observation: {
      result_count: result.entries.length,
      exact_matches: [],
      matching_rule: "normalized exact publisher name",
      requested_company: company,
      page_url: result.url,
      explicit_no_results: result.no_results === true,
      scope: "inspected rows on the returned result page; no exhaustive pagination claim",
      publisher_names: result.entries.map((entry) => entry.name),
    },
  };
  const outputs = path.join(runDir, "outputs");
  fs.mkdirSync(outputs, { recursive: true });
  if (!fs.lstatSync(outputs).isDirectory()) throw new Error("outputs must be a real directory");
  const bytes = Buffer.from(JSON.stringify(receipt) + "\n", "utf8");
  if (bytes.length > 1_048_576) throw new Error("query evidence exceeds native receipt limit");
  fs.writeFileSync(path.join(outputs, "query-evidence.json"), bytes, { flag: "wx", mode: 0o600 });
  return {
    records: [],
    query_completion: {
      evidence_path: "outputs/query-evidence.json",
      evidence_sha256: createHash("sha256").update(bytes).digest("hex"),
    },
  };
}

function buildRecords(entry, sourceUrl) {
  const safeUrl = allowedSourceUrl(sourceUrl);
  if (!entry || !safeUrl || !entry.name) return [];
  const noteParts = ["Bundesanzeiger Suchergebnis"];
  if (entry.section) noteParts.push(entry.section);
  if (entry.information) noteParts.push(entry.information);
  if (entry.date) noteParts.push(`Veroeffentlicht: ${entry.date}`);
  const note = noteParts.join(" - ");
  const records = [
    {
      field: "firma_name",
      value: entry.name,
      confidence: "medium",
      source_url: safeUrl.href,
      note,
    },
  ];
  if (entry.city) {
    records.push({
      field: "firma_ort",
      value: entry.city,
      confidence: "medium",
      source_url: safeUrl.href,
      note,
    });
  }
  return records;
}

function classifyBrowserResult(company, result) {
  if (!result) {
    return {
      records: [],
      failure_mode: "temporary_unreachable",
      detail: "bundesanzeiger.de browser automation did not return a result",
    };
  }
  if (result.blocked === true) {
    return {
      records: [],
      failure_mode: "blocked",
      detail: "bundesanzeiger.de requires web unlock after a visible access challenge",
    };
  }
  const sourceUrl = allowedSourceUrl(result.url);
  if (!sourceUrl) {
    return {
      records: [],
      failure_mode: "temporary_unreachable",
      detail: "bundesanzeiger.de browser left the allowed origin",
    };
  }
  const entry = selectMatchingEntry(company, result.entries);
  if (entry) return { records: buildRecords(entry, sourceUrl.href) };
  if (isCompletedEmptyQuery(company, result)) return { records: [], query_completed: true };
  if (result.no_results === true) {
    return {
      records: [],
      failure_mode: "temporary_unreachable",
      detail: "bundesanzeiger.de found no publications for the exact company name",
    };
  }
  if (result.results_page === true) {
    return {
      records: [],
      failure_mode:
        Array.isArray(result.entries) && result.entries.length > 0
          ? "temporary_unreachable"
          : "portal_drift",
      detail:
        Array.isArray(result.entries) && result.entries.length > 0
          ? "bundesanzeiger.de returned no exact company match"
          : "bundesanzeiger.de result page did not match known result selectors",
    };
  }
  return {
    records: [],
    failure_mode: "temporary_unreachable",
    detail: "bundesanzeiger.de did not reach a readable result page",
  };
}

function browserSearch(company) {
  const source = `// ctox-browser: timeout_ms=${BROWSER_TIMEOUT_MS}
const searchUrl = ${JSON.stringify(SEARCH_URL)};
const query = ${JSON.stringify(company)};
const createQueryTrace = ${createQueryTrace.toString()};
let queryResponse = null;
let queryTrace = null;
let cdp = null;
const completedResult = async (result) => {
  const { frameTree } = await cdp.send("Page.getFrameTree");
  const current = queryTrace.snapshot(frameTree.frame);
  return ({
  ...result,
  query,
  query_completed: Boolean(current && queryResponse)
    && current.request_id === queryResponse.request_id
    && current.loader_id === queryResponse.loader_id && result.url === current.url,
  http_status: queryResponse?.status ?? null,
  response_url: queryResponse?.url ?? null,
  });
};
await page.goto(searchUrl, { waitUntil: "domcontentloaded", timeout: 30000 });
await page.waitForTimeout(1500);

const challengeState = async () => await page.evaluate(() => {
  const text = document.body ? document.body.innerText : "";
  const challenge = document.querySelector(
    'iframe[src*="captcha" i], iframe[src*="challenge" i], .g-recaptcha, [data-sitekey]',
  );
  return {
    blocked: Boolean(challenge) || /schutzma(?:ß|ss)nahme|sicherheitsabfrage|to_nlp_start|cf-chl-|turnstile|captcha|verify (?:that )?you are human|access denied|request blocked|zugriff verweigert|zu viele anfragen/i.test(text),
    noResults: /keine passenden Daten gefunden/i.test(text),
  };
});

if ((await challengeState()).blocked) {
  return { url: page.url(), blocked: true, results_page: false, entries: [] };
}

// Consent: exact label first, then generic patterns; tolerate late banners.
for (const pattern of [
  "Nur technisch notwendige Cookies akzeptieren",
  /alle akzeptieren|akzeptieren|zustimmen|verstanden/i,
]) {
  const button = page.getByRole("button", { name: pattern }).first();
  if (await button.count()) {
    await button.click({ timeout: 2500 }).catch(() => null);
    break;
  }
}
await page.waitForTimeout(1200);

const searchInput = page.locator('input[name="fulltext"]');
if (await searchInput.count() !== 1) {
  return { url: page.url(), blocked: false, results_page: false, entries: [] };
}

// Patchright's high-level navigation response can be an internal injection
// response. Only the actual CDP network chain and document loader are evidence.
cdp = await page.context().newCDPSession(page);
try {
await cdp.send("Network.enable");
await cdp.send("Page.enable");
for (const event of ["Network.requestWillBeSent", "Network.responseReceived",
  "Network.loadingFinished", "Network.loadingFailed"]) {
  cdp.on(event, data => queryTrace?.capture(event, data));
}
let onResults = false;
for (let attempt = 0; attempt < 2 && !onResults; attempt += 1) {
  await searchInput.fill(query);
  queryResponse = null;
  const { frameTree } = await cdp.send("Page.getFrameTree");
  queryTrace = createQueryTrace(query, new URL(searchUrl).origin, frameTree.frame);
  // Register before submitting. A pre-existing result container or a completed
  // AJAX request cannot satisfy this current-document navigation boundary.
  const navigationPromise = page.waitForNavigation({
    waitUntil: "domcontentloaded", timeout: 30000,
  }).catch(() => null);
  // Honeypot-aware: a hidden decoy input[name="search-button"] exists; only
  // the visible valued submit triggers the search.
  const searchButton = page.locator('input[name="search-button"][value="Suchen"]');
  if (await searchButton.count() === 1) {
    await searchButton.click();
  } else {
    await searchInput.press("Enter");
  }
  const response = await navigationPromise;
  if (response) {
    await page.waitForLoadState("load", { timeout: 30000 }).catch(() => {});
    const { frameTree: currentFrame } = await cdp.send("Page.getFrameTree");
    queryResponse = queryTrace.snapshot(currentFrame.frame);
  }
  if (!queryResponse || queryResponse.status < 200 || queryResponse.status >= 300) {
    const state = await challengeState();
    return { url: page.url(), blocked: state.blocked, results_page: false, entries: [] };
  }
  await page.waitForLoadState("domcontentloaded", { timeout: 30000 }).catch(() => {});
  onResults = await page.waitForSelector(".result_container", { timeout: 20000 })
    .then(() => true)
    .catch(() => false);
  if (!onResults) {
    const state = await challengeState();
    if (state.blocked) {
      return { url: page.url(), blocked: true, results_page: false, entries: [] };
    }
    if (state.noResults) {
      return await completedResult({ url: page.url(), blocked: false, results_page: true, entries: [], no_results: true });
    }
    // Wicket sometimes bounces back to the empty form; retry once.
    if (attempt === 0 && (await searchInput.count()) === 1) {
      await page.waitForTimeout(1500);
      continue;
    }
    return { url: page.url(), blocked: false, results_page: false, entries: [] };
  }
}

const result = await page.evaluate(() => {
  const text = document.body ? document.body.innerText : "";
  const challenge = document.querySelector(
    'iframe[src*="captcha" i], iframe[src*="challenge" i], .g-recaptcha, [data-sitekey]',
  );
  const blocked = Boolean(challenge) || /schutzma(?:ß|ss)nahme|sicherheitsabfrage|to_nlp_start|cf-chl-|turnstile|captcha|verify (?:that )?you are human|access denied|request blocked|zugriff verweigert|zu viele anfragen/i.test(text);
  const container = document.querySelector(".result_container");
  const entries = container
    ? [...container.querySelectorAll(":scope > .row")].map((row) => {
        if (row.classList.contains("result_header")
            || row.classList.contains("concern_list")
            || row.classList.contains("subsidiary_list")) return null;
        const first = row.querySelector(":scope > .col-md-3 .first");
        if (!first) return null;
        const lines = first.innerText.split("\\n").map((part) => part.trim()).filter(Boolean);
        return {
          name: lines[0] || "",
          city: lines[1] || "",
          section: (row.querySelector(":scope > .col-md-2 .part")?.innerText || "").replace(/\\s+/g, " ").trim(),
          information: (row.querySelector(":scope > .col-md-5 .info > a")?.innerText || "").replace(/\\s+/g, " ").trim(),
          date: (row.querySelector(":scope > .col-md-2 .date")?.innerText || "").replace(/\\s+/g, " ").trim(),
        };
      }).filter((entry) => entry && entry.name)
    : [];
  return {
    url: location.href,
    blocked,
    results_page: Boolean(container) || /Suchergebnis/i.test(document.title),
    no_results: /keine passenden Daten gefunden/i.test(text),
    entries,
  };
});
return await completedResult(result);
} finally {
  await cdp.detach().catch(() => {});
}
`;
  const payload = runCtox(
    ["web", "browser-automation", "--timeout-ms", String(BROWSER_TIMEOUT_MS)],
    source,
  );
  if (payload && payload.ok === true) return payload.result;
  if (Array.isArray(payload?.detection?.markers) && payload.detection.markers.length > 0) {
    return { url: SEARCH_URL, blocked: true, results_page: false, entries: [] };
  }
  return null;
}

async function main() {
  const rawInput = process.env.CTOX_SCRAPE_INPUT_JSON;
  const input = readInput(rawInput);
  const company = typeof input?.company === "string" ? input.company.trim() : "";
  const query = company ? requestedQuery(input) : "";
  if (!company || !query) {
    process.stdout.write(
      JSON.stringify({
        records: [],
        failure_mode: "portal_drift",
        detail: "CTOX_SCRAPE_INPUT_JSON.company or query missing/invalid",
      }),
    );
    return;
  }
  const browserResult = browserSearch(query);
  let output = classifyBrowserResult(company, browserResult);
  if (output.query_completed) output = writeQueryCompletion(rawInput, company, browserResult);
  if (output.failure_mode === "blocked") {
    recordUnlockSignal(browserResult?.url, ["access_challenge"]);
  }
  process.stdout.write(JSON.stringify(output));
}

if (require.main === module) {
  main().catch((err) => {
    process.stdout.write(
      JSON.stringify({
        records: [],
        failure_mode: "temporary_unreachable",
        detail: `bundesanzeiger.de browser flow failed: ${err.message}`,
      }),
    );
  });
}

module.exports = {
  allowedSourceUrl,
  buildRecords,
  classifyBrowserResult,
  entryMatchesCompany,
  normalizeCompanyName,
  selectMatchingEntry,
  requestedQuery,
  createQueryTrace,
  isCompletedEmptyQuery,
  writeQueryCompletion,
  browserSearch,
};
