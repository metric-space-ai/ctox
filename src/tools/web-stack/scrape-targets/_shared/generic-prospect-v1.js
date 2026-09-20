"use strict";

const { execFileSync } = require("child_process");
const { writeFileSync, unlinkSync } = require("fs");
const path = require("path");

const COMMAND_ERRORS = [];
const BLOCKED_DETECTIONS = [];
const RECORD_FIELDS = new Set([
  "firma_name", "firma_anschrift", "firma_plz", "firma_ort", "firma_email",
  "firma_domain", "firma_telefon", "wz_code", "umsatz", "mitarbeiter",
  "crm_record_number", "person_titel", "person_vorname", "person_nachname",
  "person_funktion", "person_position", "person_email", "person_email_validation",
  "person_telefon", "person_linkedin", "person_xing",
]);
const CONFIDENCE_LEVELS = new Set(["low", "medium", "high", "user_provided"]);

const PROTECTED_SOURCE_CONFIG = Object.freeze({
  "dnbhoovers.com": {
    login_url: "https://app.dnbhoovers.com/login",
    allowed_domains: ["dnbhoovers.com", "app.dnbhoovers.com", "plus.dnb.com"],
    credential_ref: "ctox-secret://credentials/DNB_HOOVERS_BROWSER_LOGIN",
    capture_supported: true,
  },
  "leadfeeder.com": {
    login_url: "https://app.leadfeeder.com/f/sign/in",
    allowed_domains: ["leadfeeder.com", "app.leadfeeder.com", "api.leadfeeder.com"],
    credential_ref: "ctox-secret://credentials/LEADFEEDER_BROWSER_LOGIN",
    capture_supported: true,
  },
  "rocketreach.com": {
    login_url: "https://rocketreach.co/login",
    allowed_domains: ["rocketreach.com", "rocketreach.co"],
    credential_ref: "ctox-secret://credentials/ROCKETREACH_BROWSER_LOGIN",
    capture_supported: true,
    public_fields: ["firma_name"],
  },
  "linkedin.com": {
    login_url: "https://www.linkedin.com/login",
    allowed_domains: ["linkedin.com", "www.linkedin.com", "login.linkedin.com", "api.linkedin.com"],
    credential_ref: "ctox-secret://credentials/LINKEDIN_BROWSER_LOGIN",
    capture_supported: true,
  },
  "xing.com": {
    login_url: "https://login.xing.com/",
    allowed_domains: ["xing.com", "www.xing.com", "login.xing.com", "api.xing.com"],
    credential_ref: "ctox-secret://credentials/XING_BROWSER_LOGIN",
    capture_supported: true,
  },
});

function commandErrorsIndicateBlocking() {
  return COMMAND_ERRORS.some((error) =>
    /captcha|anti-bot|interstitial|cloudflare|turnstile|verify (that )?you are human|access denied|request blocked|rate.?limit|too many requests/i.test(error)
  );
}

function rememberCommandError(command, detail) {
  const text = String(detail || "unknown error")
    .replace(/([a-z][a-z0-9+.-]*:\/\/)[^\s/@:]+:[^\s/@]+@/gi, "$1[redacted]@")
    .replace(/\b(authorization|password|passwd|token|api[_-]?key)\s*[:=]\s*[^\s,;]+/gi, "$1=[redacted]")
    .replace(/[\u0000-\u001f]+/g, " ")
    .replace(/\s+/g, " ")
    .trim()
    .slice(0, 1200);
  COMMAND_ERRORS.push(`${command}: ${text}`);
}

const SOURCE_CONFIG = Object.freeze({
  "bundesanzeiger.de": { native: true, domains: ["bundesanzeiger.de"] },
  "companyhouse.de": { native: true, domains: ["companyhouse.de"] },
  "dnbhoovers.com": { native: true, native_only: true, domains: ["dnbhoovers.com", "dnb.com", "app.dnbhoovers.com", "plus.dnb.com"] },
  "firmenabc.at": { native: true, domains: ["firmenabc.at"] },
  "handelsregister.de": { native: true, domains: ["handelsregister.de"] },
  "leadfeeder.com": { native: true, native_only: true, domains: ["leadfeeder.com", "app.leadfeeder.com", "api.leadfeeder.com"] },
  "linkedin.com": { native: true, native_only: true, domains: ["linkedin.com", "www.linkedin.com", "login.linkedin.com", "api.linkedin.com"] },
  "moneyhouse.ch": { native: false, domains: ["moneyhouse.ch"] },
  "northdata.de": { native: true, domains: ["northdata.de"] },
  "xing.com": { native: true, native_only: true, domains: ["xing.com", "api.xing.com"] },
  "zefix.ch": { native: true, native_only: true, domains: ["zefix.ch", "zefix.admin.ch"] },
  "google.de": { native: true, native_only: true, domains: [] },
  "maps.google.com": { native: false, domains: ["google.com", "google.de"] },
  "rocketreach.com": { native: false, domains: ["rocketreach.com", "rocketreach.co"] },
  "experte.de": { native: false, domains: ["experte.de"] },
  // The Impressum is published on the researched company's OWN host, so this
  // source has no fixed domain list. Its adapter derives the candidate host
  // from the company name and verifies the company identity per fetch
  // (impressum/scripts/v1.js), which is what google.de does here too.
  "impressum": { native: true, native_only: true, domains: [] },
});

function isPortalOrLoginTitle(title) {
  const normalized = String(title || "").replace(/\s+/g, " ").trim();
  if (!normalized) return false;
  return /\b(?:log[ -]?in|sign[ -]?in|anmeld(?:en|ung)|authentication|authentifizierung|kundenportal|customer portal)\b/i.test(normalized)
    || /^(?:portal|startseite|home|willkommen)(?:\s*[-|:]\s*.*)?$/i.test(normalized);
}

function readInput() {
  try {
    return JSON.parse(process.env.CTOX_SCRAPE_INPUT_JSON || "{}");
  } catch (error) {
    return {};
  }
}

function runCtox(args) {
  try {
    return JSON.parse(execFileSync(process.env.CTOX_BIN || "ctox", args, {
      encoding: "utf8",
      stdio: ["ignore", "pipe", "pipe"],
      maxBuffer: 32 * 1024 * 1024,
    }));
  } catch (error) {
    rememberCommandError(
      args.slice(0, 2).join(" "),
      error?.stderr?.toString?.() || error?.stdout?.toString?.() || error?.message,
    );
    return null;
  }
}

function runBrowserAutomation(name, source, timeoutMs = 60000) {
  const outputDir = process.env.CTOX_SCRAPE_OUTPUT_DIR || process.cwd();
  const scriptPath = path.join(outputDir, `${name}-${process.pid}.js`);
  try {
    writeFileSync(scriptPath, source, { mode: 0o600 });
    const payload = runCtox([
      "web", "browser-automation",
      "--script-file", scriptPath,
      "--timeout-ms", String(timeoutMs),
    ]);
    if (payload && !payload.ok) {
      rememberCommandError(
        `browser-automation ${name}`,
        payload.error || payload.reason || JSON.stringify(payload),
      );
    }
    const markers = payload?.detection?.markers;
    if (Array.isArray(markers) && markers.length > 0) {
      BLOCKED_DETECTIONS.push(...markers.map(String));
    }
    return payload?.ok ? payload.result : null;
  } finally {
    try { unlinkSync(scriptPath); } catch {}
  }
}

function validateEmailWithExperte(email) {
  const source = `
const email = ${JSON.stringify(email)};
await ctoxBrowser.goto("https://www.experte.de/email-pruefen", { timeoutMs: 30000 });
await page.waitForLoadState("networkidle", { timeout: 8000 }).catch(() => null);
const consent = page.getByRole("button", { name: /akzeptieren|zustimmen/i }).first();
if (await consent.count()) await consent.click({ timeout: 3000 }).catch(() => null);
const field = page.locator('input[type="url"], input[type="email"], input[placeholder*="E-Mail" i], input').first();
if ((await field.count()) < 1) throw new Error("EXPERTE email field not found");
await field.fill(email);
const submit = page.getByRole("button", { name: /E-Mail prüfen/i }).first();
if ((await submit.count()) < 1) throw new Error("EXPERTE submit button not found");
await submit.click();
await page.waitForFunction(
  (value) => document.body && document.body.innerText.includes(value)
    && /Gültig|Ungültig|Unbekannt|Fehlgeschlagen/i.test(document.body.innerText),
  email,
  { timeout: 45000 }
);
const text = await page.locator("body").innerText();
const title = await page.title().catch(() => "");
const lines = text.split(/\\n+/).map((line) => line.trim()).filter(Boolean);
const index = lines.findIndex((line) => line.toLowerCase().includes(email.toLowerCase()));
const evidence = lines.slice(Math.max(0, index), index < 0 ? 0 : index + 10).join(" | ");
const status = /Ungültig/i.test(evidence) ? "invalid"
  : /Unbekannt/i.test(evidence) ? "unknown"
  : /Gültig/i.test(evidence) ? "valid" : "failed";
return { email, status, evidence: evidence.slice(0, 700), url: page.url(), title };
`;
  const result = runBrowserAutomation("experte-email", source);
  return result?.email === email && !isPortalOrLoginTitle(result?.title) ? result : null;
}

function hostOf(url) {
  try {
    const parsed = new URL(url);
    if (!["http:", "https:"].includes(parsed.protocol) || parsed.username || parsed.password) {
      return "";
    }
    return parsed.hostname.replace(/^www\./, "").toLowerCase();
  } catch (error) {
    return "";
  }
}

function safePublicHttpUrl(url) {
  try {
    const parsed = new URL(url);
    if (!["http:", "https:"].includes(parsed.protocol) || parsed.username || parsed.password) {
      return false;
    }
    const host = parsed.hostname.toLowerCase();
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

function sourceConfig(sourceId) {
  return SOURCE_CONFIG[sourceId] || { native: false, domains: [sourceId] };
}

function protectedSourceConfig(sourceId) {
  return PROTECTED_SOURCE_CONFIG[sourceId] || null;
}

function requestedAccessMode(input) {
  const raw = String(input?.access_mode || input?.auth_mode || "").trim().toLowerCase();
  if (["native_api", "api_key", "api"].includes(raw)) return "native_api";
  if (["authenticated_browser", "browser_session", "browser"].includes(raw)) {
    return "authenticated_browser";
  }
  return "";
}

function useNativeApiPath(sourceId, input, credentialRef) {
  if (sourceId !== "leadfeeder.com") return false;
  const mode = requestedAccessMode(input);
  if (mode === "authenticated_browser") return false;
  if (mode === "native_api") return true;
  // Real scrape execute currently forwards only company/country/source_id.
  // Native registry search is the supported default; browser capture is opt-in
  // via access_mode/auth_mode=authenticated_browser (must be forwarded from the
  // adapter/target manifest). A browser credential_ref without that mode still
  // prefers the API so a secret-store API key can take effect.
  return true;
}

function nativeApiFailure(payload) {
  const failures = Array.isArray(payload?.source_failures) ? payload.source_failures : [];
  const first = failures[0];
  if (!first) return null;
  const kind = String(first.kind || "");
  const error = String(first.error || "");
  const secretName = String(first.secret_name || "").trim();
  if (kind === "credential_missing") {
    return { mode: "credential_missing", detail: secretName || "LEADFEEDER_API_KEY" };
  }
  if (kind === "no_match") return { mode: "no_match", detail: "no matching company" };
  if (kind === "rate_limited") return { mode: "rate_limited", detail: "rate_limited" };
  if (kind === "parse_failed") return { mode: "parse_failed", detail: "parse_failed" };
  if (kind === "blocked") return { mode: "entitlement", detail: "blocked" };
  if (kind === "other" && /account_selection_required/i.test(error)) {
    return { mode: "account_selection_required", detail: "account_selection_required" };
  }
  if (kind === "other" && /entitlement/i.test(error)) {
    return { mode: "entitlement", detail: "entitlement" };
  }
  if (kind) return { mode: "temporary_unreachable", detail: kind };
  return null;
}

function emitNativeApiResult(sourceId, company, country, queryValue) {
  const payload = search(sourceId, queryValue, country);
  const nativeFailure = nativeApiFailure(payload);
  if (nativeFailure) {
    process.stdout.write(JSON.stringify({
      records: [],
      failure_mode: nativeFailure.mode,
      detail: nativeFailure.detail,
      browser_assist_requested: false,
    }));
    return;
  }
  if (!payload) {
    process.stdout.write(JSON.stringify({
      records: [],
      failure_mode: COMMAND_ERRORS.some((error) => /rate.?limit/i.test(error))
        ? "rate_limited"
        : "temporary_unreachable",
      detail: COMMAND_ERRORS.join(" | ") || `${sourceId} native search failed`,
      browser_assist_requested: false,
    }));
    return;
  }
  const records = [];
  const hits = Array.isArray(payload.results) ? payload.results.slice(0, 5) : [];
  for (const hit of hits) {
    if (!hit?.url || !isAllowedSourceUrl(sourceId, hit.url)) continue;
    if (!pageMatchesCompany(company, hit, null)) continue;
    appendSearchHitEvidence(records, sourceId, hit, company);
    for (const field of extractedFields(hit)) appendRecord(records, field, hit.url);
  }
  const clean = finalizeRecords(records, sourceId);
  if (!recordsMatchCompany(company, clean)) {
    process.stdout.write(JSON.stringify({
      records: [],
      failure_mode: "no_match",
      detail: `${sourceId} API returned no company-matched records`,
      browser_assist_requested: false,
    }));
    return;
  }
  process.stdout.write(JSON.stringify({ records: clean }));
}

function validCredentialReference(value) {
  const raw = String(value || "").trim();
  if (!raw || /\s/.test(raw)) return "";
  try {
    const parsed = new URL(raw);
    const segments = parsed.pathname.split("/").filter(Boolean);
    if (parsed.protocol !== "ctox-secret:" || parsed.username || parsed.password
        || parsed.search || parsed.hash || !parsed.hostname || segments.length !== 1) {
      return "";
    }
    return raw;
  } catch {
    return "";
  }
}

function credentialReference(input, config) {
  const requested = input?.credential_ref || input?.fallback?.credential_ref;
  return validCredentialReference(requested || config?.credential_ref);
}

function safeTaskId(input) {
  return String(input?.task_id || input?.thread_key || "")
    .replace(/[^a-zA-Z0-9._:-]+/g, "-")
    .slice(0, 160);
}

function safeSessionId(input) {
  return String(input?.browser_session_id || input?.session_id || "")
    .replace(/[^a-zA-Z0-9._:-]+/g, "-")
    .slice(0, 200);
}

function allowedDomainsAreSafe(sourceId, domains) {
  if (!Array.isArray(domains) || domains.length === 0) return false;
  const configured = protectedSourceConfig(sourceId)?.allowed_domains || sourceConfig(sourceId).domains;
  return domains.every((domain) => {
    const normalized = String(domain || "").replace(/^\.+/, "").toLowerCase();
    return configured.some((allowed) =>
      normalized === allowed || normalized.endsWith(`.${allowed}`)
    );
  });
}

function runProtectedCapture(sourceId, company, country, sessionId = "") {
  const args = [
    "business-os", "web-stack", "source-capture",
    "--source-id", sourceId,
    "--company", company,
    "--country", country || "DE",
    "--timeout-ms", "60000",
  ];
  if (sessionId) args.push("--session-id", sessionId);
  return runCtox(args);
}

function runProtectedLogin(sourceId, config, credentialRef, input) {
  if (!credentialRef || !isAllowedSourceUrl(sourceId, config.login_url)) return null;
  const args = [
    "business-os", "web-stack", "auth-assist-login",
    "--source-id", sourceId,
    "--target-url", config.login_url,
    "--credential-ref", credentialRef,
    "--timeout-ms", "60000",
  ];
  const taskId = safeTaskId(input);
  if (taskId) args.push("--task-id", taskId);
  const result = runCtox(args);
  if (!result?.ok || !isAllowedSourceUrl(sourceId, result.target_url || config.login_url)) return null;
  const allowedDomains = result?.auth_assist_request?.allowed_domains || result?.allowed_domains;
  if (allowedDomains && !allowedDomainsAreSafe(sourceId, allowedDomains)) {
    rememberCommandError("auth-assist-login", "browser session returned an unsafe domain allow-list");
    return null;
  }
  return result;
}

function requestBrowserAuthorization(sourceId, config, credentialRef, input) {
  if (!config || !isAllowedSourceUrl(sourceId, config.login_url)) return null;
  const args = [
    "business-os", "web-stack", "auth-assist-request",
    "--source-id", sourceId,
    "--target-url", config.login_url,
  ];
  if (credentialRef) args.push("--credential-ref", credentialRef);
  const taskId = safeTaskId(input);
  if (taskId) args.push("--task-id", taskId);
  const result = runCtox(args);
  if (!result?.ok || !isAllowedSourceUrl(sourceId, result.target_url || config.login_url)) return null;
  if (!allowedDomainsAreSafe(sourceId, result.allowed_domains)) {
    rememberCommandError("auth-assist-request", "browser session returned an unsafe domain allow-list");
    return null;
  }
  return result;
}

function reauthorizationAction(sourceId, config, credentialRef) {
  if (!config || !isAllowedSourceUrl(sourceId, config.login_url)) return null;
  return {
    kind: "auth-assist-request",
    source_id: sourceId,
    login_url: config.login_url,
    allowed_domains: config.allowed_domains,
    credential_ref: credentialRef || null,
    reason: "session_expired_or_invalid",
    secret_value_in_payload: false,
  };
}

function recordUnlockSignal(sourceId, url, markers) {
  const sourceUrl = isAllowedSourceUrl(sourceId, url)
    ? url
    : protectedSourceConfig(sourceId)?.login_url;
  const evidence = JSON.stringify({
    source_id: sourceId,
    detection: "access_challenge",
    markers: [...new Set((markers || []).map(String))].slice(0, 12),
    secret_value_in_payload: false,
  });
  const args = [
    "web", "unlock", "signals", "record",
    "--source", `scrape-target:${sourceId}`,
    "--evidence", evidence,
  ];
  if (sourceUrl) args.push("--url", sourceUrl);
  return runCtox(args);
}

function isAllowedSourceUrl(sourceId, url) {
  if (!safePublicHttpUrl(url)) return false;
  if (sourceId === "google.de") return true;
  const host = hostOf(url);
  return sourceConfig(sourceId).domains.some((domain) =>
    host === domain || host.endsWith(`.${domain}`)
  );
}

function search(sourceId, company, country) {
  const config = sourceConfig(sourceId);
  const query = sourceId === "maps.google.com"
    ? `${company} ${country || ""} Google Maps`.trim()
    : company;
  const payloads = [];
  if (config.native) {
    const args = ["web", "search", "--query", query, "--include-sources", "--source", sourceId];
    if (country) args.push("--country", country);
    payloads.push(runCtox(args));
  }
  if (!config.native_only) {
    for (const domain of config.domains) {
      const args = ["web", "search", "--query", query, "--include-sources", "--domain", domain];
      if (country) args.push("--country", country);
      payloads.push(runCtox(args));
    }
  }
  // Directory-only searches can be empty even though the provider has an
  // exact public company profile. Keep the fallback query provider-labelled
  // and accept only URLs that pass the source allow-list below.
  if (sourceId === "rocketreach.com") {
    const args = [
      "web", "search", "--query", `${company} RocketReach`, "--include-sources",
    ];
    if (country) args.push("--country", country);
    payloads.push(runCtox(args));
  }
  if (payloads.length === 0) {
    const args = ["web", "search", "--query", query, "--include-sources"];
    if (country) args.push("--country", country);
    payloads.push(runCtox(args));
  }
  const results = [];
  const sourceFailures = [];
  const providers = [];
  const seen = new Set();
  for (const payload of payloads.filter(Boolean)) {
    if (payload.provider) providers.push(String(payload.provider).toLowerCase());
    for (const hit of Array.isArray(payload.results) ? payload.results : []) {
      if (!hit?.url || seen.has(hit.url)) continue;
      seen.add(hit.url);
      results.push(hit);
    }
    if (Array.isArray(payload.source_failures)) sourceFailures.push(...payload.source_failures);
  }
  return { results, source_failures: sourceFailures, providers: [...new Set(providers)] };
}

function readPage(url, country) {
  const args = ["web", "read", "--url", url];
  if (country) args.push("--country", country);
  return runCtox(args);
}

function readPageWithBrowser(sourceId, url) {
  if (!isAllowedSourceUrl(sourceId, url)) return null;
  const source = `
const targetUrl = ${JSON.stringify(url)};
await ctoxBrowser.goto(targetUrl, { timeoutMs: 30000 });
await page.waitForLoadState("domcontentloaded", { timeout: 10000 }).catch(() => null);
const consentPatterns = [/nur technisch notwendige/i, /alle akzeptieren/i, /akzeptieren/i, /zustimmen/i, /verstanden/i];
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
  extracted_fields: { fields: [] },
};
`;
  return runBrowserAutomation(`source-read-${sourceId.replace(/[^a-z0-9]/gi, "-")}`, source, 50000);
}

function searchOfficialPortal(sourceId, company, country) {
  let source;
  if (sourceId === "bundesanzeiger.de") {
    source = `
const company = ${JSON.stringify(company)};
await ctoxBrowser.goto("https://www.bundesanzeiger.de/pub/de/suche?0", { timeoutMs: 30000 });
const consent = page.getByRole("button", { name: /nur technisch notwendige cookies akzeptieren/i }).first();
if (await consent.count()) await consent.click({ timeout: 3000 }).catch(() => null);
const field = page.locator('input[name="fulltext"]').first();
await field.fill(company);
await field.press("Enter");
await page.waitForLoadState("domcontentloaded", { timeout: 10000 }).catch(() => null);
await page.waitForFunction((value) => document.body?.innerText.includes(value), company, { timeout: 30000 }).catch(() => null);
const text = await page.locator("body").innerText();
return { url: page.url(), title: await page.title(), page_text_excerpt: text.replace(/\\s+/g, " ").trim().slice(0, 16000) };
`;
  } else if (sourceId === "handelsregister.de") {
    source = `
const company = ${JSON.stringify(company)};
await ctoxBrowser.goto("https://www.handelsregister.de/rp_web/welcome.xhtml", { timeoutMs: 30000 });
const understood = page.getByRole("button", { name: /verstanden|okay/i }).first();
if (await understood.count()) await understood.click({ timeout: 3000 }).catch(() => null);
const normalSearch = page.getByRole("link", { name: /normale suche|normal search/i }).first();
if (await normalSearch.count()) await normalSearch.click();
await page.waitForLoadState("domcontentloaded", { timeout: 10000 }).catch(() => null);
const field = page.locator('[id="form:schlagwoerter"]').first();
await field.fill(company);
await page.locator('[id="form:btnSuche"]').first().click();
await page.waitForLoadState("domcontentloaded", { timeout: 10000 }).catch(() => null);
await page.waitForFunction((value) => document.body?.innerText.includes(value), company, { timeout: 30000 }).catch(() => null);
const text = await page.locator("body").innerText();
return { url: page.url(), title: await page.title(), page_text_excerpt: text.replace(/\\s+/g, " ").trim().slice(0, 16000) };
`;
  } else if (sourceId === "companyhouse.de") {
    source = `
const company = ${JSON.stringify(company)};
const url = "https://www.companyhouse.de/s/" + encodeURIComponent(company);
await ctoxBrowser.goto(url, { timeoutMs: 30000 });
await page.waitForLoadState("domcontentloaded", { timeout: 10000 }).catch(() => null);
await page.waitForFunction(
  (value) => document.body?.innerText.toLowerCase().includes(value.toLowerCase()),
  company,
  { timeout: 30000 },
).catch(() => null);
const text = await page.locator("body").innerText();
return { url: page.url(), title: await page.title(), page_text_excerpt: text.replace(/\\s+/g, " ").trim().slice(0, 16000) };
`;
  } else if (sourceId === "firmenabc.at") {
    source = `
const company = ${JSON.stringify(company)};
await ctoxBrowser.goto("https://www.firmenabc.at/", { timeoutMs: 30000 }).catch(async (error) => {
  if (!/execution context was destroyed/i.test(String(error))) throw error;
  await page.waitForLoadState("domcontentloaded", { timeout: 10000 }).catch(() => null);
});
await page.locator("#CybotCookiebotDialogBodyButtonDecline").click({ timeout: 3000 }).catch(() => null);
const field = page.locator("#whatSearchField").first();
await field.fill(company);
await field.press("Enter");
await page.waitForLoadState("domcontentloaded", { timeout: 10000 }).catch(() => null);
await page.waitForFunction((value) => document.body?.innerText.toLowerCase().includes(value.toLowerCase()), company, { timeout: 30000 }).catch(() => null);
const text = await page.locator("body").innerText();
return { url: page.url(), title: await page.title(), page_text_excerpt: text.replace(/\\s+/g, " ").trim().slice(0, 16000) };
`;
  } else if (sourceId === "moneyhouse.ch") {
    source = `
const company = ${JSON.stringify(company)};
const url = "https://www.moneyhouse.ch/de/search?q=" + encodeURIComponent(company) + "&status=1&tab=companies";
await ctoxBrowser.goto(url, { timeoutMs: 30000 });
await page.waitForLoadState("domcontentloaded", { timeout: 10000 }).catch(() => null);
await page.waitForFunction((value) => document.body?.innerText.toLowerCase().includes(value.toLowerCase()), company, { timeout: 30000 }).catch(() => null);
const text = await page.locator("body").innerText();
return { url: page.url(), title: await page.title(), page_text_excerpt: text.replace(/\\s+/g, " ").trim().slice(0, 16000) };
`;
  } else if (sourceId === "northdata.de") {
    source = `
const company = ${JSON.stringify(company)};
await ctoxBrowser.goto("https://www.northdata.de/", { timeoutMs: 30000 });
const field = page.locator('input[name="query"]:visible').first();
await field.fill(company);
await field.press("Enter");
await page.waitForLoadState("domcontentloaded", { timeout: 10000 }).catch(() => null);
await page.waitForFunction((value) => document.body?.innerText.toLowerCase().includes(value.toLowerCase()), company, { timeout: 30000 }).catch(() => null);
const text = await page.locator("body").innerText();
return { url: page.url(), title: await page.title(), page_text_excerpt: text.replace(/\\s+/g, " ").trim().slice(0, 16000) };
`;
  } else if (sourceId === "google.de") {
    source = `
const company = ${JSON.stringify(company)};
const url = "https://www.google.de/search?q=" + encodeURIComponent(company);
await ctoxBrowser.goto(url, { timeoutMs: 30000 });
const reject = page.getByRole("button", { name: /alle ablehnen|reject all/i }).first();
if (await reject.count()) {
  await reject.click({ timeout: 3000 }).catch(() => null);
  await page.waitForLoadState("domcontentloaded", { timeout: 10000 }).catch(() => null);
}
await page.waitForFunction(
  (value) => document.body?.innerText.toLowerCase().includes(value.toLowerCase()),
  company,
  { timeout: 30000 },
).catch(() => null);
const text = await page.locator("body").innerText();
return {
  url: page.url(),
  title: await page.title(),
  page_text_excerpt: text.replace(/\\s+/g, " ").trim().slice(0, 16000),
};
`;
  } else if (sourceId === "maps.google.com") {
    source = `
const company = ${JSON.stringify(company)};
const country = ${JSON.stringify(country)};
const countryName = ({ DE: "Deutschland", AT: "Österreich", CH: "Schweiz" })[country] || country;
const query = [company, countryName].filter(Boolean).join(", ");
await ctoxBrowser.goto("https://www.google.com/maps/search/?api=1&query=" + encodeURIComponent(query), { timeoutMs: 30000 });
const reject = page.getByRole("button", { name: /tout refuser|alle ablehnen|reject all|alles ablehnen/i }).first();
if (await reject.count()) {
  await reject.click({ timeout: 3000 }).catch(() => null);
  await page.waitForLoadState("domcontentloaded", { timeout: 10000 }).catch(() => null);
}
await page.waitForFunction((value) => document.body?.innerText.toLowerCase().includes(value.toLowerCase()), company, { timeout: 30000 }).catch(() => null);
const exactResult = page.locator('a[href*="/maps/place/"]').filter({ hasText: company }).first();
if (await exactResult.count()) {
  await exactResult.click({ timeout: 5000 }).catch(() => null);
  await page.waitForTimeout(1500);
}
const text = await page.locator("body").innerText();
const phoneButton = page.locator('button[data-item-id^="phone:tel:"]').first();
const phone = await phoneButton.getAttribute("data-item-id").then((value) => value?.replace(/^phone:tel:/, "") || "").catch(() => "");
const addressButton = page.locator('button[data-item-id="address"]').first();
const address = await addressButton.getAttribute("aria-label").then((value) => value?.replace(/^(?:Adresse|Address):\\s*/i, "") || "").catch(() => "");
const postal = address.match(/\\b(?:D-|A-|CH-)?(\\d{4,5})\\s+([^,]+)/);
return {
  url: page.url(),
  title: await page.title(),
  page_text_excerpt: text.replace(/\\s+/g, " ").trim().slice(0, 16000),
  extracted_fields: { fields: [
    ...(phone ? [{ field: "firma_telefon", value: phone, confidence: "high", note: "Google Maps detail panel" }] : []),
    ...(address ? [{ field: "firma_anschrift", value: address, confidence: "high", note: "Google Maps detail panel" }] : []),
    ...(postal ? [{ field: "firma_plz", value: postal[1], confidence: "high", note: "Google Maps address" }] : []),
    ...(postal ? [{ field: "firma_ort", value: postal[2].trim(), confidence: "high", note: "Google Maps address" }] : []),
  ] },
};
`;
  } else {
    return null;
  }
  return runBrowserAutomation(`portal-search-${sourceId.replace(/[^a-z0-9]/gi, "-")}`, source, 70000);
}

function appendRecord(records, record, fallbackUrl) {
  const field = String(record?.field || "").trim();
  const value = String(record?.value || "").trim();
  if (!RECORD_FIELDS.has(field) || !value) return;
  const sourceUrl = String(record?.source_url || fallbackUrl || "").trim();
  if (!safePublicHttpUrl(sourceUrl)) return;
  const confidence = String(record?.confidence || "medium").toLowerCase();
  if (!CONFIDENCE_LEVELS.has(confidence)) return;
  const key = `${field}\u0000${value}\u0000${sourceUrl}`;
  if (records.some((item) => item.__key === key)) return;
  records.push({
    __key: key,
    field,
    value,
    confidence,
    source_url: sourceUrl,
    note: String(record?.note || "CTOX web-stack source adapter"),
  });
}

function finalizeRecords(records, sourceId) {
  const observedAt = new Date().toISOString();
  const clean = [];
  for (const item of records) {
    const normalized = [];
    appendRecord(normalized, item, item?.source_url);
    if (normalized.length < 1) continue;
    const { __key, ...record } = normalized[0];
    if (!isAllowedSourceUrl(sourceId, record.source_url)) continue;
    clean.push({
      ...record,
      source_id: sourceId,
      observed_at: observedAt,
    });
  }
  return clean;
}

function acceptedProviderRecords(sourceId, company, records) {
  const clean = finalizeRecords(Array.isArray(records) ? records : [], sourceId)
    .filter((record) => sourceUrlIsProvider(sourceId, record.source_url));
  return recordsMatchCompany(company, clean) ? clean : [];
}

function extractedFields(page) {
  const raw = page?.extracted_fields;
  // Native search contract: results[].extracted_fields is an array of
  // {field,value,confidence,source_url,note}. Empty array means no fields.
  // Web-read and other scrape sources still use { fields: [...] }.
  if (Array.isArray(raw)) return raw;
  const nested = raw?.fields;
  return Array.isArray(nested) ? nested : [];
}

function pageText(page) {
  return [page?.title, page?.summary, page?.page_text_excerpt]
    .filter(Boolean)
    .join("\n")
    .replace(/\s+/g, " ")
    .trim();
}

function normalizedCompanyTokens(company) {
  const legalForms = new Set([
    "ag", "gmbh", "mbh", "se", "kg", "kgaa", "ohg", "ug", "ltd", "inc",
    "sa", "sarl", "sàrl", "nv", "bv", "co", "company", "holding", "gruppe",
  ]);
  return String(company || "")
    .toLocaleLowerCase("de-DE")
    .normalize("NFKD")
    .replace(/[^a-z0-9äöüß]+/gi, " ")
    .split(/\s+/)
    .filter((token) => token.length >= 3 && !legalForms.has(token));
}

function pageMatchesCompany(company, hit, page) {
  const tokens = normalizedCompanyTokens(company);
  if (tokens.length === 0) return false;
  if (isPortalOrLoginTitle(hit?.title) || isPortalOrLoginTitle(page?.title)) return false;
  const normalizedHitTitle = String(hit?.title || "")
    .toLocaleLowerCase("de-DE")
    .normalize("NFKD");
  if (normalizedHitTitle && !tokens.every((token) => normalizedHitTitle.includes(token))) {
    return false;
  }
  const normalizedPageTitle = String(page?.title || "")
    .toLocaleLowerCase("de-DE")
    .normalize("NFKD");
  if (normalizedPageTitle && !tokens.every((token) => normalizedPageTitle.includes(token))) {
    return false;
  }
  const hitCorpus = [hit?.title, hit?.summary, hit?.snippet]
    .filter(Boolean)
    .join(" ")
    .toLocaleLowerCase("de-DE")
    .normalize("NFKD");
  if (hit && !tokens.every((token) => hitCorpus.includes(token))) return false;
  const pageCorpus = [page?.title, page?.summary, page?.page_text_excerpt]
    .filter(Boolean)
    .join(" ")
    .toLocaleLowerCase("de-DE")
    .normalize("NFKD");
  if (!page) return tokens.every((token) => hitCorpus.includes(token));
  return tokens.every((token) => pageCorpus.includes(token));
}

function recordsMatchCompany(company, records) {
  const tokens = normalizedCompanyTokens(company);
  if (tokens.length === 0 || !Array.isArray(records)) return false;
  return records.some((record) => {
    if (record?.field !== "firma_name") return false;
    const value = String(record?.value || "")
      .toLocaleLowerCase("de-DE")
      .normalize("NFKD");
    return tokens.every((token) => value.includes(token));
  });
}

function sourceUrlIsProvider(sourceId, url) {
  const host = hostOf(url);
  return sourceConfig(sourceId).domains.some((domain) =>
    host === domain || host.endsWith(`.${domain}`)
  );
}

function emailBelongsToProvider(sourceId, email) {
  const domain = String(email).split("@").pop()?.toLowerCase() || "";
  return sourceConfig(sourceId).domains.some((providerDomain) =>
    domain === providerDomain || domain.endsWith(`.${providerDomain}`)
  );
}

function appendPublicHeuristics(records, sourceId, hit, page, company) {
  const text = pageText(page);
  const sourceUrl = String(page?.url || hit?.url || "");
  if (!text || !sourceUrl) return;

  if (sourceId === "google.de") {
    const emails = [...text.matchAll(/\b[A-Z0-9._%+-]+@[A-Z0-9.-]+\.[A-Z]{2,}\b/gi)]
      .map((match) => match[0].toLowerCase())
      .filter((email) => !emailBelongsToProvider(sourceId, email))
      .slice(0, 3);
    for (const email of emails) {
      appendRecord(records, {
        field: "firma_email",
        value: email,
        confidence: "medium",
        source_url: sourceUrl,
        note: "Email published on the company page discovered by Google",
      }, sourceUrl);
    }
  }

  const mayUseGenericPhone = sourceId !== "maps.google.com"
    && !sourceUrlIsProvider(sourceId, sourceUrl);
  const phones = mayUseGenericPhone
    ? [...text.matchAll(/(?:\+|00)\d[\d\s()\/-]{7,}\d/g)]
    .map((match) => match[0].replace(/\s+/g, " ").trim())
    .slice(0, 2)
    : [];
  for (const phone of phones) {
    appendRecord(records, {
      field: "firma_telefon",
      value: phone,
      confidence: "medium",
      source_url: sourceUrl,
      note: `${sourceId} page text`,
    }, sourceUrl);
  }

  const postal = text.match(/\b(?:D-|A-|CH-)?(\d{4,5})\s+([A-ZÄÖÜ][A-Za-zÄÖÜäöüß.'-]*(?:\s+[A-ZÄÖÜ][A-Za-zÄÖÜäöüß.'-]*){0,2})/);
  if (postal && sourceId === "maps.google.com") {
    appendRecord(records, {
      field: "firma_plz",
      value: postal[1],
      confidence: "medium",
      source_url: sourceUrl,
      note: "Google Maps address text",
    }, sourceUrl);
    appendRecord(records, {
      field: "firma_ort",
      value: postal[2].trim(),
      confidence: "medium",
      source_url: sourceUrl,
      note: "Google Maps address text",
    }, sourceUrl);
  }

  if (sourceId === "google.de" && company) {
    const host = hostOf(sourceUrl);
    const excluded = [
      "google.", "linkedin.", "xing.", "facebook.", "northdata.",
      "wikipedia.", "partcommunity.", "companyhouse.", "moneyhouse.",
    ];
    const alreadyFound = records.some((record) => record.field === "firma_domain");
    if (!alreadyFound && host && !excluded.some((entry) => host.includes(entry))) {
      appendRecord(records, {
        field: "firma_domain",
        value: host,
        confidence: "medium",
        source_url: sourceUrl,
        note: `Google result for ${company}`,
      }, sourceUrl);
    }
  }
}

function appendSearchHitEvidence(records, sourceId, hit, company) {
  const sourceUrl = String(hit?.url || "").trim();
  if (!sourceUrl || !isAllowedSourceUrl(sourceId, sourceUrl)) return;
  if (!pageMatchesCompany(company, hit, null)) return;
  appendRecord(records, {
    field: "firma_name",
    value: company,
    confidence: "medium",
    source_url: sourceUrl,
    note: `${sourceId} original search result confirms the company identity`,
  }, sourceUrl);
}

(function main() {
  const input = readInput();
  const sourceId = String(input.source_id || "").trim().toLowerCase();
  const company = String(input.company || "").trim();
  const email = String(input.email || "").trim().toLowerCase();
  const queryValue = sourceId === "experte.de" ? email : company;
  const country = String(input.country || "").trim().toUpperCase();
  if (!sourceId || !SOURCE_CONFIG[sourceId]) {
    process.stdout.write(JSON.stringify({
      records: [],
      failure_mode: "portal_drift",
      detail: "unsupported or missing source_id",
    }));
    return;
  }
  if (!queryValue) {
    process.stdout.write(JSON.stringify({
      records: [],
      failure_mode: "portal_drift",
      detail: sourceId === "experte.de" ? "email input required" : "company input required",
    }));
    return;
  }

  if (sourceId === "experte.de") {
    const validation = validateEmailWithExperte(email);
    const blocked = BLOCKED_DETECTIONS.length > 0 || commandErrorsIndicateBlocking();
    if (blocked) {
      recordUnlockSignal(
        sourceId,
        validation?.url || "https://www.experte.de/email-pruefen",
        BLOCKED_DETECTIONS.length > 0 ? BLOCKED_DETECTIONS : ["access_challenge"],
      );
      process.stdout.write(JSON.stringify({
        records: [],
        failure_mode: "blocked",
        detail: "EXPERTE email validation was blocked before provider evidence was available",
      }));
      return;
    }
    if (!validation
        || !["valid", "invalid", "unknown"].includes(validation.status)
        || !isAllowedSourceUrl(sourceId, validation.url)) {
      process.stdout.write(JSON.stringify({
        records: [],
        failure_mode: "temporary_unreachable",
        detail: COMMAND_ERRORS.length > 0
          ? COMMAND_ERRORS.join(" | ")
          : "EXPERTE email validation did not return conclusive provider evidence",
      }));
      return;
    }
    process.stdout.write(JSON.stringify({ records: finalizeRecords([{
      field: "person_email_validation",
      value: validation.status,
      confidence: validation.status === "unknown" ? "medium" : "high",
      source_url: validation.url || "https://www.experte.de/email-pruefen",
      note: `EXPERTE: ${email} ${validation.status}`,
      email,
    }], sourceId) }));
    return;
  }

  const protectedConfig = protectedSourceConfig(sourceId);
  const credentialRef = credentialReference(input, protectedConfig);
  if (useNativeApiPath(sourceId, input, credentialRef)) {
    emitNativeApiResult(sourceId, company, country, queryValue);
    return;
  }
  let protectedCaptureStatus = "";
  let browserAssist = null;
  if (protectedConfig?.capture_supported) {
    let unlockRecorded = false;
    let captured = runProtectedCapture(
      sourceId,
      company,
      country,
      safeSessionId(input),
    );
    protectedCaptureStatus = String(captured?.source_status || "").trim();
    let accepted = acceptedProviderRecords(sourceId, company, captured?.records);
    if (captured?.ok && accepted.length > 0) {
      process.stdout.write(JSON.stringify({ records: accepted }));
      return;
    }
    if (["blocked", "access_challenge"].includes(protectedCaptureStatus)) {
      const captureUrl = String(captured?.source_url || protectedConfig.login_url);
      recordUnlockSignal(sourceId, captureUrl, [protectedCaptureStatus]);
      unlockRecorded = true;
    }
    const shouldRetryAfterLogin = Boolean(credentialRef)
      && protectedCaptureStatus !== "succeeded";
    if (shouldRetryAfterLogin) {
      const login = runProtectedLogin(sourceId, protectedConfig, credentialRef, input);
      const sessionId = String(login?.session_id || "").trim();
      if (sessionId) {
        captured = runProtectedCapture(sourceId, company, country, sessionId);
        protectedCaptureStatus = String(captured?.source_status || "").trim();
        accepted = acceptedProviderRecords(sourceId, company, captured?.records);
        if (captured?.ok && accepted.length > 0) {
          process.stdout.write(JSON.stringify({ records: accepted }));
          return;
        }
      }
      browserAssist = requestBrowserAuthorization(
        sourceId,
        protectedConfig,
        credentialRef,
        input,
      );
    }
    const captureBlocked = ["blocked", "access_challenge"].includes(protectedCaptureStatus)
      || BLOCKED_DETECTIONS.length > 0
      || commandErrorsIndicateBlocking();
    if (captureBlocked && !protectedConfig.public_fields) {
      if (!unlockRecorded) {
        recordUnlockSignal(
          sourceId,
          captured?.source_url || protectedConfig.login_url,
          BLOCKED_DETECTIONS.length > 0 ? BLOCKED_DETECTIONS : ["access_challenge"],
        );
      }
      if (!browserAssist) {
        browserAssist = requestBrowserAuthorization(
          sourceId,
          protectedConfig,
          credentialRef,
          input,
        );
      }
      process.stdout.write(JSON.stringify({
        records: [],
        failure_mode: "blocked",
        detail: `${sourceId} requires Web-Unlock before provider data can be accepted`,
        browser_assist_requested: Boolean(browserAssist),
      }));
      return;
    }
    if (["auth_required", "wrong_origin"].includes(protectedCaptureStatus)
        && !protectedConfig.public_fields) {
      process.stdout.write(JSON.stringify({
        records: [],
        failure_mode: "authorization_required",
        detail: `${sourceId} session expired or invalid; reauthorization required (authenticated CTOX browser session)`,
        browser_assist_requested: Boolean(browserAssist),
        reauthorization: reauthorizationAction(sourceId, protectedConfig, credentialRef),
      }));
      return;
    }
    if (!protectedConfig.public_fields) {
      process.stdout.write(JSON.stringify({
        records: [],
        failure_mode: protectedCaptureStatus === "succeeded" ? "portal_drift" : "temporary_unreachable",
        detail: protectedCaptureStatus === "succeeded"
          ? `${sourceId} capture did not return company-matched provider records`
          : `${sourceId} authenticated capture returned ${protectedCaptureStatus || "no status"}`,
        browser_assist_requested: Boolean(browserAssist),
      }));
      return;
    }
  }

  const records = [];
  const portalPage = searchOfficialPortal(sourceId, company, country);
  if (portalPage?.url && isAllowedSourceUrl(sourceId, portalPage.url)
      && pageMatchesCompany(company, null, portalPage)) {
    appendRecord(records, {
      field: "firma_name",
      value: company,
      confidence: "high",
      source_url: portalPage.url,
      note: `${sourceId} official portal search confirms the company identity`,
    }, portalPage.url);
    for (const field of extractedFields(portalPage)) {
      appendRecord(records, field, portalPage.url);
    }
    appendPublicHeuristics(records, sourceId, null, portalPage, company);
  }

  const payload = search(sourceId, queryValue, country);
  const googleProviderVerified = sourceId !== "google.de"
    || payload.providers.includes("google");
  const hits = googleProviderVerified && Array.isArray(payload?.results)
    ? payload.results.slice(0, 5)
    : [];
  for (const hit of hits) {
    if (!hit?.url || !isAllowedSourceUrl(sourceId, hit.url)) continue;
    if (protectedConfig && isPortalOrLoginTitle(hit?.title)) {
      protectedCaptureStatus = "auth_required";
      continue;
    }
    appendSearchHitEvidence(records, sourceId, hit, company);
    for (const field of extractedFields(hit)) appendRecord(records, field, hit.url);
    let page = readPage(hit.url, country);
    if (!page?.ok) page = readPageWithBrowser(sourceId, hit.url);
    if (!page?.ok || !isAllowedSourceUrl(sourceId, page.url || hit.url)) continue;
    if (protectedConfig && isPortalOrLoginTitle(page?.title)) {
      protectedCaptureStatus = "auth_required";
      continue;
    }
    if (!pageMatchesCompany(company, hit, page)) continue;
    for (const field of extractedFields(page)) appendRecord(records, field, hit.url);
    appendPublicHeuristics(records, sourceId, hit, page, company);
    if (sourceId === "google.de"
        && records.some((record) => record.field === "firma_domain")) break;
  }

  let clean = finalizeRecords(records, sourceId);
  if (protectedConfig?.public_fields) {
    const publicFields = new Set(protectedConfig.public_fields);
    clean = clean.filter((record) => publicFields.has(record.field)
      && sourceUrlIsProvider(sourceId, record.source_url));
  }
  const authFailure = Array.isArray(payload?.source_failures)
    && payload.source_failures.some((failure) => failure?.kind === "auth_required");
  const providerBlocked = Array.isArray(payload?.source_failures)
    && payload.source_failures.some((failure) => ["blocked", "access_challenge"].includes(failure?.kind));
  const protectedAuthFailure = ["auth_required", "wrong_origin"].includes(protectedCaptureStatus);
  const protectedNeedsAuth = Boolean(protectedConfig && clean.length === 0);
  const commandBlocked = commandErrorsIndicateBlocking();
  const accessBlocked = BLOCKED_DETECTIONS.length > 0
    || providerBlocked
    || commandBlocked
    || ["blocked", "access_challenge"].includes(protectedCaptureStatus);
  if (protectedConfig?.public_fields && clean.length > 0) {
    if (!browserAssist) {
      browserAssist = requestBrowserAuthorization(
        sourceId,
        protectedConfig,
        credentialRef,
        input,
      );
    }
    process.stdout.write(JSON.stringify({
      records: clean,
      partial: true,
      protected_fields_require_authorization: true,
      browser_assist_requested: Boolean(browserAssist),
    }));
    return;
  }
  if (protectedConfig
      && (accessBlocked || authFailure || protectedAuthFailure || protectedNeedsAuth)
      && !browserAssist) {
    if (accessBlocked) {
      recordUnlockSignal(
        sourceId,
        protectedConfig.login_url,
        BLOCKED_DETECTIONS.length > 0 ? BLOCKED_DETECTIONS : ["access_challenge"],
      );
    }
    browserAssist = requestBrowserAuthorization(
      sourceId,
      protectedConfig,
      credentialRef,
      input,
    );
  }
  if (protectedConfig && accessBlocked) {
    process.stdout.write(JSON.stringify({
      records: [],
      failure_mode: "blocked",
      detail: `${sourceId} requires Web-Unlock before provider data can be accepted`,
      browser_assist_requested: Boolean(browserAssist),
    }));
    return;
  }
  if (protectedConfig && (authFailure || protectedAuthFailure)) {
    process.stdout.write(JSON.stringify({
      records: [],
      failure_mode: "authorization_required",
      detail: `${sourceId} session expired or invalid; reauthorization required (authenticated CTOX browser session)`,
      browser_assist_requested: Boolean(browserAssist),
      reauthorization: reauthorizationAction(sourceId, protectedConfig, credentialRef),
    }));
    return;
  }
  if (accessBlocked) {
    if (!protectedConfig) {
      recordUnlockSignal(
        sourceId,
        portalPage?.url || hits[0]?.url,
        BLOCKED_DETECTIONS.length > 0 ? BLOCKED_DETECTIONS : ["access_challenge"],
      );
    }
    process.stdout.write(JSON.stringify({
      records: [],
      failure_mode: "blocked",
      detail: `${sourceId} access challenge prevented provider evidence`,
      browser_assist_requested: Boolean(browserAssist),
    }));
    return;
  }
  if (protectedNeedsAuth) {
    process.stdout.write(JSON.stringify({
      records: [],
      failure_mode: "auth_required",
      detail: `${sourceId} requires an authenticated CTOX browser session for protected data`,
      browser_assist_requested: Boolean(browserAssist),
    }));
    return;
  }
  if (clean.length === 0) {
    const reauthRequired = authFailure || protectedAuthFailure;
    process.stdout.write(JSON.stringify({
      records: [],
      failure_mode: accessBlocked ? "blocked"
        : reauthRequired ? (protectedConfig ? "authorization_required" : "auth_required") : "temporary_unreachable",
      detail: !googleProviderVerified
        ? "google.de target rejected results from a non-Google search provider"
        : reauthRequired
        ? `${sourceId} session expired or invalid; reauthorization required (authenticated CTOX browser session)`
        : BLOCKED_DETECTIONS.length > 0
          ? `${sourceId} browser challenge: ${[...new Set(BLOCKED_DETECTIONS)].join(", ")}`
        : commandBlocked
          ? `${sourceId} search provider challenge`
        : COMMAND_ERRORS.length > 0
          ? COMMAND_ERRORS.join(" | ")
          : `${sourceId} returned no extractable records`,
      browser_assist_requested: Boolean(browserAssist),
      ...(protectedConfig && reauthRequired
        ? { reauthorization: reauthorizationAction(sourceId, protectedConfig, credentialRef) }
        : {}),
    }));
    return;
  }
  process.stdout.write(JSON.stringify({ records: clean }));
})();
