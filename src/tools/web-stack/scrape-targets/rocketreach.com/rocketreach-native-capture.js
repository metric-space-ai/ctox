"use strict";

const { execFileSync } = require("child_process");
const { writeFileSync, unlinkSync } = require("fs");
const path = require("path");

const COMMAND_ERRORS = [];
const BLOCKED_DETECTIONS = [];
// One bounded adapter invocation, including native login, capture and assist.
const ADAPTER_DEADLINE_MS = Date.now() + 180_000;
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
    public_fields: false,
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
    const value = JSON.parse(process.env.CTOX_SCRAPE_INPUT_JSON || "{}");
    return value && typeof value === "object" && !Array.isArray(value) ? value : {};
  } catch (error) {
    return {};
  }
}

function runCtox(args) {
  const remainingMs = ADAPTER_DEADLINE_MS - Date.now();
  if (remainingMs <= 0) {
    rememberCommandError(args.slice(0, 2).join(" "), "adapter deadline exceeded");
    return null;
  }
  try {
    return JSON.parse(execFileSync(process.env.CTOX_BIN || "ctox", args, {
      encoding: "utf8",
      stdio: ["ignore", "pipe", "pipe"],
      timeout: Math.min(65_000, remainingMs),
      killSignal: "SIGKILL",
      maxBuffer: 4 * 1024 * 1024,
    }));
  } catch (error) {
    rememberCommandError(
      args.slice(0, 2).join(" "),
      error?.code === "ETIMEDOUT" ? "native command timed out"
        : error?.stderr?.toString?.() || error?.stdout?.toString?.() || error?.message,
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
  return String(input?.task_id || input?.research_command_id || input?.command_id || input?.thread_key || "")
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
  // Ohne Auftragsbezug findet CTOX den Besitzer der Anmeldung nicht ("auth assist
  // owner unresolved") und die hinterlegten Zugangsdaten werden nie benutzt
  // (gemessen 22.09.2026). Login und Anfrage reichten die Kennung schon weiter,
  // die eigentliche Erfassung nicht.
  const taskId = safeTaskId(readInput());
  if (taskId) args.push("--task-id", taskId);
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
  if (sourceId === "rocketreach.com") {
    const parsed = new URL(url);
    if (parsed.protocol !== "https:" || (parsed.port && parsed.port !== "443")) return false;
  }
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
  const field = typeof record?.field === "string" ? record.field.trim() : "";
  const value = typeof record?.value === "string" ? record.value.trim() : "";
  const rawUrl = typeof record?.source_url === "string" ? record.source_url
    : typeof fallbackUrl === "string" ? fallbackUrl : "";
  const quote = [record?.source_quote, record?.quote, record?.note]
    .find((item) => typeof item === "string" && item.trim());
  if (!RECORD_FIELDS.has(field) || !value || !quote || !quote.includes(value)) return;
  let url;
  try { url = new URL(rawUrl); } catch { return; }
  if (url.protocol !== "https:" || url.username || url.password
      || (url.port && url.port !== "443") || !sourceUrlIsProvider("rocketreach.com", url.href)) return;
  url.search = ""; url.hash = "";
  const confidence = typeof record?.confidence === "string" ? record.confidence.toLowerCase() : "medium";
  if (!CONFIDENCE_LEVELS.has(confidence) || confidence === "user_provided") return;
  let personKey = "";
  if (field.startsWith("person_")) {
    const path = url.pathname.replace(/\/$/, "");
    const identity = path.match(/\/[^/]+-email_([a-z0-9_-]+)$/i)?.[1]
      || path.match(/\/(?:people|person)\/([^/]+)$/i)?.[1];
    personKey = typeof record.person_key === "string" ? record.person_key.trim() : "";
    if (!identity || personKey !== "rocketreach-person-" + identity.toLowerCase()) return;
  } else if (!/(?:\/company\/[^/]+|\/companies\/[^/]+|\/[^/]+-profile_[a-z0-9_-]+)\/?$/i.test(url.pathname)) return;
  const sourceUrl = url.href;
  const key = field + "\u0000" + value + "\u0000" + sourceUrl + "\u0000" + personKey;
  if (records.some((item) => item.__key === key)) return;
  records.push({ __key: key, field, value, confidence, source_url: sourceUrl,
    note: quote.trim(), source_quote: quote.trim(), ...(personKey ? {person_key: personKey} : {}) });
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
  if (sourceId !== "rocketreach.com") return [];
  return personRecordsOfCompany(company,
    finalizeRecords(Array.isArray(records) ? records : [], sourceId));
}

// Native RocketReach records retain their observed quote and provider person key.
// A company prefix or another contact's value never establishes ownership.
function personRecordsOfCompany(company, records) {
  const legalForms = new Set(["ag", "se", "gmbh", "kg", "ohg", "gbr", "mbh", "inc",
    "ltd", "llc", "gesellschaft", "aktiengesellschaft", "holding", "group", "gruppe", "company"]);
  const normalize = (value) => typeof value === "string" ? value.toLocaleLowerCase("de-DE")
    .normalize("NFKD").replace(/[\u0300-\u036f]/g, "").replace(/ß/g, "ss")
    .replace(/[^\p{L}\p{N}]+/gu, " ").trim() : "";
  const required = [...new Set(normalize(company).split(/\s+/)
    .filter((word) => word.length >= 2 && !legalForms.has(word)))];
  const matched = (quote) => {
    const words = new Set(normalize(quote).split(/\s+/));
    return required.length > 0 && required.every((word) => words.has(word));
  };
  if (!required.length) return [];
  const groups = new Map(); const accepted = [];
  for (const record of records) {
    if (record.field === "firma_name" && matched(record.value)) accepted.push(record);
    if (!record.field.startsWith("person_") || !record.person_key) continue;
    if (!groups.has(record.person_key)) groups.set(record.person_key, []);
    groups.get(record.person_key).push(record);
  }
  if (!accepted.some((record) => record.field === "firma_name")) return [];
  for (const group of groups.values()) {
    const first = new Set(group.filter((record) => record.field === "person_vorname").map((record) => record.value));
    const last = new Set(group.filter((record) => record.field === "person_nachname").map((record) => record.value));
    if (first.size !== 1 || last.size !== 1) continue;
    const owner = normalize(`${[...first][0]} ${[...last][0]}`);
    for (const record of group) if (matched(record.source_quote)
        && (` ${normalize(record.source_quote)} `).includes(` ${owner} `)
        && !/\b(?:former|previous|formerly|ehemalig\w*)\b/i.test(record.source_quote)) accepted.push(record);
  }
  return accepted;
}

function extractedFields(page) {
  const fields = page?.extracted_fields?.fields;
  return Array.isArray(fields) ? fields : [];
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
  const sourceId = typeof input.source_id === "string" ? input.source_id.trim().toLowerCase() : "rocketreach.com";
  const company = typeof input.company === "string" ? input.company.trim() : "";
  if ((input.source_id != null && (typeof input.source_id !== "string" || sourceId !== "rocketreach.com"))
      || !company || (input.country != null && typeof input.country !== "string")) {
    process.stdout.write(JSON.stringify({ records: [], failure_mode: "invalid_input",
      detail: "RocketReach requires a scalar company and a RocketReach source; country must be text" }));
    return;
  }
  const country = typeof input.country === "string" ? input.country.trim().toUpperCase() : "DE";
  const config = protectedSourceConfig(sourceId);
  const credentialRef = credentialReference(input, config);
  let captured = runProtectedCapture(sourceId, company, country, safeSessionId(input));
  let status = typeof captured?.source_status === "string" ? captured.source_status : "";
  let accepted = acceptedProviderRecords(sourceId, company, captured?.records);
  const emit = (value) => process.stdout.write(JSON.stringify(value));
  if (captured?.ok === true && status === "succeeded" && accepted.length) {
    emit({ records: accepted }); return;
  }
  // A transport error is not an expired login and must not spawn more work.
  const requiresLogin = (value) => ["auth_required", "authorization_required",
    "credential_missing", "session_expired", "wrong_origin"].includes(value);
  let browserAssist = null;
  if (requiresLogin(status)) {
    const login = runProtectedLogin(sourceId, config, credentialRef, input);
    if (login && typeof login.session_id === "string" && login.session_id.trim()) {
      captured = runProtectedCapture(sourceId, company, country, login.session_id.trim());
      status = typeof captured?.source_status === "string" ? captured.source_status : "";
      accepted = acceptedProviderRecords(sourceId, company, captured?.records);
      if (captured?.ok === true && status === "succeeded" && accepted.length) {
        emit({ records: accepted }); return;
      }
    }
  }
  const blocked = ["blocked", "access_challenge"].includes(status)
    || BLOCKED_DETECTIONS.length > 0 || commandErrorsIndicateBlocking();
  if (blocked) {
    recordUnlockSignal(sourceId, config.login_url, [status || "access_challenge"]);
    browserAssist = requestBrowserAuthorization(sourceId, config, credentialRef, input);
    emit({ records: [], failure_mode: "blocked",
      detail: "RocketReach requires a verified browser unblock before evidence is available",
      browser_assist_requested: Boolean(browserAssist) }); return;
  }
  if (requiresLogin(status)) {
    browserAssist = requestBrowserAuthorization(sourceId, config, credentialRef, input);
    emit({ records: [], failure_mode: "authorization_required",
      detail: "RocketReach requires an authenticated provider session; missing credentials must be entered in source settings",
      browser_assist_requested: Boolean(browserAssist),
      reauthorization: reauthorizationAction(sourceId, config, credentialRef) }); return;
  }
  emit({ records: [], failure_mode: "temporary_unreachable",
    detail: COMMAND_ERRORS.length ? COMMAND_ERRORS.join(" | ")
      : "RocketReach returned no accepted, quoted company/person evidence; no completed negative query is established",
    browser_assist_requested: false });
})();
