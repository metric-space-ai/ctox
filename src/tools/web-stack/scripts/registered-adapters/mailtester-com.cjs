// mailtester.com — prospect.v1 adapter (person_email_validation only).
//
// The free provider checker is browser-backed.  This adapter accepts a
// verdict only when the provider page itself renders it for the exact input
// address on the MailTester origin.  It never sends a message and never
// attempts to bypass CAPTCHA, Cloudflare, or other access controls.

"use strict";

const { execFileSync } = require("child_process");

const SOURCE_ID = "mailtester.com";
const START_URL = "https://mailtester.com/email-checker/";
const ALLOWED_HOSTS = new Set(["mailtester.com", "www.mailtester.com"]);
const CONCLUSIVE_STATUSES = new Set(["valid", "invalid", "unknown"]);

function readInput() {
  const raw = process.env.CTOX_SCRAPE_INPUT_JSON;
  if (!raw) return {};
  try { return JSON.parse(raw); } catch (_err) { return {}; }
}

function ctoxBin() { return process.env.CTOX_BIN || "ctox"; }

function runCtox(args, input, timeout = 150_000) {
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
    return null;
  }
}

function normalized(value) {
  return String(value || "").normalize("NFKD")
    .replace(/[\u0300-\u036f]/g, "").toLowerCase()
    .replace(/\s+/g, " ").trim();
}

function hostOf(value) {
  try { return new URL(value).hostname.toLowerCase(); } catch (_err) { return ""; }
}

function isAllowedUrl(value) {
  try {
    const url = new URL(value);
    return url.protocol === "https:" && ALLOWED_HOSTS.has(url.hostname.toLowerCase());
  } catch (_err) { return false; }
}

function isBlockedPage(page) {
  const corpus = normalized([page?.title, page?.body, page?.body_head]
    .filter(Boolean).join(" "));
  return /captcha|cloudflare|verify you are human|access denied|zugriff verweigert|security check|challenge/.test(corpus);
}

function hasBlockedDetection(page) {
  const markers = Array.isArray(page?.detection?.markers)
    ? page.detection.markers.join(" ") : "";
  return /captcha|cloudflare|challenge|turnstile|access[_ -]?denied|request[_ -]?blocked|rate[_ -]?limit/i.test(markers);
}

function isPortalOrLoginTitle(title) {
  return /\b(?:log[ -]?in|sign[ -]?in|anmeld(?:en|ung)|authentication|kundenportal|customer portal)\b/i
    .test(String(title || ""));
}

function browserSource(email) {
  return `
    const email = ${JSON.stringify(email)};
    const startUrl = ${JSON.stringify(START_URL)};
    await ctoxBrowser.goto(startUrl, { timeoutMs: 45000 });
    await page.waitForLoadState("networkidle", { timeout: 12000 }).catch(() => null);

    const title = await page.title().catch(() => "");
    const body = await page.locator("body").innerText().catch(() => "");
    if (/captcha|cloudflare|verify you are human|access denied|zugriff verweigert|security check/i.test(body.slice(0, 8000))) {
      return { email, status: "blocked", body: body.slice(0, 500), url: page.url(), title };
    }
    if (/\\b(?:log[ -]?in|sign[ -]?in|authentication|kundenportal|customer portal)\\b/i.test(title)) {
      return { email, status: "auth_required", url: page.url(), title };
    }

    const field = page.locator('input#hero-email-check[type="email"]:visible').first();
    if ((await field.count()) < 1) {
      return { email, status: "failed", reason: "selector_drift: MailTester email input not found", url: page.url(), title };
    }
    await field.fill(email);
    const submit = page.getByRole("button", { name: /verify free/i }).first();
    if ((await submit.count()) < 1 || !(await submit.isVisible().catch(() => false))) {
      return { email, status: "failed", reason: "selector_drift: MailTester verify button not found", url: page.url(), title };
    }
    await submit.click();

    const resultReady = await page.waitForFunction((needle) => {
      const text = Array.from(document.querySelectorAll('[aria-live="polite"]'))
        .map((node) => (node.innerText || "").replace(/\\s+/g, " ").trim()).join(" ");
      return text.toLowerCase().includes(needle)
        && /\\b(?:deliverable|undeliverable|risky|unknown)\\b/i.test(text);
    }, email.toLowerCase(), { timeout: 90000 }).then(() => true).catch(() => false);
    if (!resultReady) {
      return { email, status: "failed", reason: "verdict_timeout: no MailTester verdict rendered within 90s", url: page.url(), title };
    }

    const evidence = await page.locator('[aria-live="polite"]').allInnerTexts();
    const text = evidence.map((part) => (part || "").replace(/\\s+/g, " ").trim()).join(" | ");
    const status = /\\bundeliverable\\b/i.test(text) ? "invalid"
      : /\\bdeliverable\\b/i.test(text) ? "valid"
      : /\\b(?:risky|unknown)\\b/i.test(text) ? "unknown" : null;
    if (!status) {
      return { email, status: "failed", reason: "verdict_unparseable: MailTester verdict label not recognized", evidence: text.slice(0, 700), url: page.url(), title };
    }
    const emailSubjects = ${emailSubjects.toString()};
    const subjects = emailSubjects(text);
    if (subjects.length !== 1 || subjects[0] !== email) {
      return { email, status: "failed", reason: "verdict_unparseable: result address is missing, mismatched or ambiguous", url: page.url(), title };
    }
    return { email: subjects[0], subject_email: subjects[0], status, evidence: text.slice(0, 700), url: page.url(), title };
  `;
}

function validateEmail(email) {
  const payload = runCtox(["web", "browser-automation", "--timeout-ms", "180000"], browserSource(email));
  if (!payload) return null;
  return { ...(payload.result || {}), ok: payload.ok === true, detection: payload.detection };
}

function recordUnlockSignal(url, markers) {
  // This is deliberately best-effort and only records typed evidence in the
  // existing CTOX unlock-signal path; the adapter never attempts an unlock.
  return runCtox([
    "web", "unlock", "signals", "record",
    "--source", "scrape-target:mailtester.com",
    "--url", isAllowedUrl(url) ? url : START_URL,
    "--evidence", JSON.stringify({ source_id: SOURCE_ID, detection: "access_challenge", markers: [...new Set((markers || []).map(String))].slice(0, 12), secret_value_in_payload: false }),
  ], undefined, 20_000);
}

function emailSubjects(text) {
  const matches = String(text || "").match(/[a-z0-9.!#$%&'*+/=?^_\x60{|}~-]+@[a-z0-9](?:[a-z0-9.-]*[a-z0-9])?/gi) || [];
  return [...new Set(matches.map(value => value.toLowerCase()))];
}

function providerSubjectEmail(validation, requestedEmail) {
  if (typeof validation?.subject_email !== "string" || typeof validation?.email !== "string") return null;
  const subject = validation.subject_email.trim().toLowerCase();
  const responseEmail = validation.email.trim().toLowerCase();
  const evidenceEmails = emailSubjects(validation.evidence);
  return subject === requestedEmail && responseEmail === subject
    && evidenceEmails.length === 1 && evidenceEmails[0] === subject ? subject : null;
}

function main() {
  const input = readInput();
  const email = String(input.email || "").trim().toLowerCase();
  if (!email) {
    process.stdout.write(JSON.stringify({ records: [], failure_mode: "portal_drift", detail: "CTOX_SCRAPE_INPUT_JSON.email missing" }));
    return;
  }

  const validation = validateEmail(email);
  const subjectEmail = providerSubjectEmail(validation, email);
  const blocked = validation && (validation.status === "blocked" || isBlockedPage(validation)
    || (!CONCLUSIVE_STATUSES.has(validation.status) && hasBlockedDetection(validation)));
  if (blocked) {
    recordUnlockSignal(isAllowedUrl(validation?.url) ? validation.url : START_URL, validation?.detection?.markers || ["access_challenge"]);
    process.stdout.write(JSON.stringify({ records: [], failure_mode: "blocked", detail: "mailtester.com access challenge recorded; no bypass attempted" }));
    return;
  }
  if (validation?.status === "auth_required") {
    process.stdout.write(JSON.stringify({ records: [], failure_mode: "auth_required", detail: "mailtester.com requires provider authentication; no credentials supplied" }));
    return;
  }

  const accepted = validation?.ok && subjectEmail !== null && validation?.email === email
    && CONCLUSIVE_STATUSES.has(validation?.status)
    && isAllowedUrl(validation?.url) && !isPortalOrLoginTitle(validation?.title)
    && normalized(validation?.evidence).includes(normalized(email));
  if (accepted) {
    process.stdout.write(JSON.stringify({ records: [{
      field: "person_email_validation",
      value: validation.status,
      subject_email: subjectEmail,
      confidence: validation.status === "unknown" ? "medium" : "high",
      source_url: new URL(validation.url).href,
      note: `MailTester provider verdict: ${String(validation.evidence || `${email} ${validation.status}`).slice(0, 300)}`,
    }] }));
    return;
  }

  const reason = String(validation?.reason || "");
  const drift = reason.startsWith("selector_drift") || reason.startsWith("verdict_unparseable");
  process.stdout.write(JSON.stringify({ records: [], failure_mode: drift ? "portal_drift" : "temporary_unreachable", detail: reason || "MailTester did not return conclusive provider evidence" }));
}

if (require.main === module) main();

module.exports = { emailSubjects, providerSubjectEmail, browserSource, isAllowedUrl, isBlockedPage, hasBlockedDetection };
