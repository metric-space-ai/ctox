import fs from "node:fs/promises";
import path from "node:path";
import { createRequire } from "node:module";

const require = createRequire(path.join(process.cwd(), "package.json"));
const { chromium } = require("playwright");

function usage() {
  console.error(
    "usage: browser_profile_probe.mjs <cdp-url> <target-url> <out-dir> <interactive-unlock:0|1> <wait-timeout-secs>",
  );
  process.exit(2);
}

function normalizeLowercaseHeaders(headers) {
  const out = {};
  for (const [key, value] of Object.entries(headers || {})) {
    if (typeof value !== "string") continue;
    const trimmed = value.trim();
    if (!trimmed) continue;
    out[key.toLowerCase()] = trimmed;
  }
  return out;
}

function looksLikeSearchRequest(url) {
  try {
    const parsed = new URL(url);
    return parsed.hostname.endsWith("google.com") && parsed.pathname.startsWith("/search");
  } catch {
    return false;
  }
}

function buildCookieHeader(cookies) {
  const byName = new Map();
  for (const cookie of cookies || []) {
    if (!cookie || typeof cookie.name !== "string" || typeof cookie.value !== "string") {
      continue;
    }
    const name = cookie.name.trim();
    if (!name) continue;
    if (!byName.has(name)) {
      byName.set(name, cookie.value);
    }
  }
  return [...byName.entries()].map(([name, value]) => `${name}=${value}`).join("; ");
}

async function inspectPage(page) {
  const html = await page.content();
  const lower = html.toLowerCase();
  const finalUrl = page.url();
  return {
    title: await page.title(),
    finalUrl,
    dataVed: /data-ved=|["']data-ved["']/.test(html),
    sorry: finalUrl.includes("/sorry/") || lower.includes("/sorry/"),
    captcha:
      lower.includes("captcha") ||
      lower.includes("unusual traffic from your computer network"),
    enablejs:
      lower.includes("enablejs") ||
      lower.includes("please click here if you are not redirected"),
    html,
  };
}

function usableSummary(summary) {
  return summary.dataVed && !summary.sorry && !summary.captcha;
}

const [cdpUrl, targetUrl, outDir, interactiveUnlockRaw, waitTimeoutSecsRaw] =
  process.argv.slice(2);
if (!cdpUrl || !targetUrl || !outDir || !interactiveUnlockRaw || !waitTimeoutSecsRaw) {
  usage();
}

const interactiveUnlock = interactiveUnlockRaw === "1";
const waitTimeoutMs = Number.parseInt(waitTimeoutSecsRaw, 10) * 1000;
if (!Number.isFinite(waitTimeoutMs) || waitTimeoutMs <= 0) {
  usage();
}

const browser = await chromium.connectOverCDP(cdpUrl);
const context = browser.contexts()[0] || (await browser.newContext());
const page = context.pages()[0] || (await context.newPage());
const mainRequestHeaders = {};
const mainCdpRequestHeaders = {};
const mainCdpRequestExtraHeaders = {};
let matchedRequestId = null;

page.on("request", (request) => {
  if (Object.keys(mainRequestHeaders).length > 0) {
    return;
  }
  if (looksLikeSearchRequest(request.url())) {
    Object.assign(mainRequestHeaders, normalizeLowercaseHeaders(request.headers()));
  }
});

const cdp = await context.newCDPSession(page);
await cdp.send("Network.enable");
cdp.on("Network.requestWillBeSent", (event) => {
  if (matchedRequestId !== null) {
    return;
  }
  if (looksLikeSearchRequest(event.request?.url || "")) {
    matchedRequestId = event.requestId;
    Object.assign(mainCdpRequestHeaders, event.request?.headers || {});
  }
});
cdp.on("Network.requestWillBeSentExtraInfo", (event) => {
  if (matchedRequestId === null || event.requestId !== matchedRequestId) {
    return;
  }
  Object.assign(
    mainCdpRequestExtraHeaders,
    normalizeLowercaseHeaders(event.headers || {}),
  );
});

await page.goto(targetUrl, { waitUntil: "domcontentloaded", timeout: 45000 });
try {
  await page.waitForLoadState("networkidle", { timeout: 10000 });
} catch {}

let summary = await inspectPage(page);
if (interactiveUnlock && !usableSummary(summary)) {
  const deadline = Date.now() + waitTimeoutMs;
  while (Date.now() < deadline && !usableSummary(summary)) {
    await page.waitForTimeout(1000);
    summary = await inspectPage(page);
  }
}

const capturedCookies = await context.cookies([
  targetUrl,
  "https://www.google.com/",
  "https://www.google.com/search",
]);
const capturedCookieHeader = buildCookieHeader(
  capturedCookies.filter((cookie) => {
    const domain = String(cookie?.domain || "").toLowerCase();
    return domain.includes("google.");
  }),
);

await fs.mkdir(outDir, { recursive: true });
await fs.writeFile(path.join(outDir, "page.html"), summary.html, "utf8");

delete summary.html;
console.log(
  JSON.stringify({
    ...summary,
    capturedCookieHeader,
    mainRequestHeaders,
    mainCdpRequestHeaders,
    mainCdpRequestExtraHeaders,
  }),
);

await cdp.detach().catch(() => {});
await browser.close().catch(() => {});
