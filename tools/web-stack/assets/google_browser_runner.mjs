// CTOX Google search runner via Playwright.
//
// Reads a single line of JSON from stdin:
//   { "query": "...", "language": "de-DE", "region": "DE",
//     "stateDir": "/path", "maxResults": 10, "timeoutMs": 25000,
//     "headless": true }
//
// Emits a single JSON object on stdout:
//   { "ok": bool, "provider": "playwright_google",
//     "results": [ { "title", "url", "snippet" }, ... ],
//     "finalUrl": "...", "title": "...", "elapsedMs": n,
//     "error": null|string, "log": [string, ...] }
//
// Stealth measures and consent dismissal are distilled from the publicly
// documented web-agent-master/google-search project (ISC, MIT-compatible).
// Launch-arg hygiene (ignoreDefaultArgs, viewport, platform-aware UA) is
// adopted from CloakHQ/CloakBrowser (MIT) — only the open-source wrapper
// rules, not the patched binary.

import { chromium } from 'playwright';
import fs from 'node:fs';

const STEALTH_LAUNCH_ARGS = [
  '--disable-blink-features=AutomationControlled',
  '--disable-features=IsolateOrigins,site-per-process,TranslateUI',
  '--disable-site-isolation-trials',
  '--no-first-run',
  '--no-default-browser-check',
  '--disable-background-networking',
  '--disable-background-timer-throttling',
  '--disable-backgrounding-occluded-windows',
  '--disable-renderer-backgrounding',
  '--disable-ipc-flooding-protection',
  '--enable-features=NetworkService,NetworkServiceInProcess',
  '--force-color-profile=srgb',
  '--metrics-recording-only',
  '--hide-scrollbars',
  '--mute-audio',
];

const STEALTH_INIT_SCRIPT = () => {
  Object.defineProperty(navigator, 'webdriver', { get: () => undefined });
  Object.defineProperty(navigator, 'plugins', { get: () => [1, 2, 3, 4, 5] });
  Object.defineProperty(navigator, 'languages', { get: () => ['de-DE', 'de', 'en-US', 'en'] });
  // @ts-ignore
  window.chrome = { runtime: {}, loadTimes: () => {}, csi: () => {}, app: {} };
  if (typeof WebGLRenderingContext !== 'undefined') {
    const orig = WebGLRenderingContext.prototype.getParameter;
    WebGLRenderingContext.prototype.getParameter = function (p) {
      if (p === 37445) return 'Intel Inc.';
      if (p === 37446) return 'Intel Iris OpenGL Engine';
      return orig.call(this, p);
    };
  }
};

async function dismissConsent(page, log) {
  const selectors = [
    "button#L2AGLb",
    "button[aria-label*='Alle akzeptieren']",
    "button[aria-label*='Accept all']",
    "button[aria-label*='Alle ablehnen']",
    "button[aria-label*='Reject all']",
    "form[action*='consent'] button[type='submit']",
  ];
  for (const sel of selectors) {
    const btn = await page.$(sel);
    if (btn) {
      try {
        await btn.click({ timeout: 4000 });
        await page.waitForTimeout(400);
        log.push(`consent dismissed via ${sel}`);
        return true;
      } catch (e) {
        log.push(`consent click failed for ${sel}: ${e.message}`);
      }
    }
  }
  return false;
}

async function extractResults(page, maxResults) {
  return await page.evaluate((max) => {
    const skipHost = (host) =>
      host === 'www.google.com' ||
      host === 'maps.google.com' ||
      host === 'policies.google.com' ||
      host === 'support.google.com' ||
      host === 'accounts.google.com' ||
      host === 'webcache.googleusercontent.com' ||
      host === 'translate.google.com';

    const canonKey = (u) => `${u.protocol}//${u.host}${u.pathname}`;

    const search = document.querySelector('div#search') || document;
    const headings = search.querySelectorAll('h3');
    const out = [];
    const seen = new Set();
    for (const h3 of headings) {
      const a = h3.closest('a[href]');
      if (!a) continue;
      let href = a.getAttribute('href') ?? '';
      if (href.startsWith('/url?q=')) {
        try {
          href = new URL(href, location.origin).searchParams.get('q') || '';
        } catch {}
      }
      if (!href.startsWith('http')) continue;
      let url;
      try { url = new URL(href); } catch { continue; }
      if (skipHost(url.host)) continue;
      const key = canonKey(url);
      if (seen.has(key)) continue;
      seen.add(key);
      const title = (h3.textContent || '').trim();
      let block = a.closest('div[data-snhf], div.MjjYud, div.g, div.tF2Cxc, div[data-hveid]');
      if (!block) block = a.parentElement?.parentElement ?? null;
      let snippet = '';
      if (block) {
        const txt = (block.textContent || '').replace(/\s+/g, ' ').trim();
        const titleIdx = txt.indexOf(title);
        const tail = titleIdx >= 0 ? txt.slice(titleIdx + title.length) : txt;
        snippet = tail.replace(/^\s*[›·–|]\s*/, '').trim().slice(0, 300);
      }
      out.push({
        title,
        url: `${url.origin}${url.pathname}${url.search}${url.hash}`,
        snippet,
      });
      if (out.length >= max) break;
    }
    return out;
  }, maxResults);
}

async function readStdinJson() {
  const chunks = [];
  for await (const chunk of process.stdin) chunks.push(chunk);
  return JSON.parse(Buffer.concat(chunks).toString('utf8'));
}

function defaultUserAgent() {
  switch (process.platform) {
    case 'darwin':
      return 'Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/146.0.0.0 Safari/537.36';
    case 'win32':
      return 'Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/146.0.0.0 Safari/537.36';
    default:
      return 'Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/146.0.0.0 Safari/537.36';
  }
}

(async () => {
  const cfg = await readStdinJson();
  const log = [];
  const startedAt = Date.now();
  const maxResults = Math.min(Math.max(cfg.maxResults ?? 10, 1), 20);
  const timeoutMs = Math.min(Math.max(cfg.timeoutMs ?? 25000, 5000), 120000);
  fs.mkdirSync(cfg.stateDir, { recursive: true });

  const language = cfg.language || 'de-DE';
  const region = cfg.region || 'DE';
  const acceptLanguageLang = language.split('-')[0].toLowerCase();
  const langs = [language, acceptLanguageLang, 'en-US', 'en'];

  const ctx = await chromium.launchPersistentContext(cfg.stateDir, {
    headless: cfg.headless !== false,
    args: STEALTH_LAUNCH_ARGS,
    ignoreDefaultArgs: ['--enable-automation', '--enable-unsafe-swiftshader'],
    locale: language,
    timezoneId: cfg.timezoneId || 'Europe/Berlin',
    colorScheme: 'dark',
    viewport: { width: 1920, height: 947 },
    userAgent: cfg.userAgent || defaultUserAgent(),
    permissions: ['geolocation', 'notifications'],
    isMobile: false,
    hasTouch: false,
    javaScriptEnabled: true,
  });

  await ctx.addInitScript(`(${STEALTH_INIT_SCRIPT.toString()})()`);

  const page = await ctx.newPage();
  const outcome = {
    ok: false,
    provider: 'playwright_google',
    results: [],
    finalUrl: null,
    title: null,
    error: null,
    log,
    elapsedMs: 0,
  };

  try {
    const url = `https://www.google.com/search?q=${encodeURIComponent(cfg.query)}&hl=${acceptLanguageLang}&gl=${region}`;
    log.push(`navigating to ${url}`);
    const resp = await page.goto(url, { waitUntil: 'domcontentloaded', timeout: timeoutMs });
    log.push(`http=${resp ? resp.status() : 'null'} url=${page.url()}`);

    if (/\/sorry\/index/.test(page.url())) {
      outcome.error = 'google CAPTCHA: /sorry/index';
      outcome.finalUrl = page.url();
    } else {
      await dismissConsent(page, log);
      try {
        await page.waitForSelector('div#search a, a[jsname="UWckNb"]', { timeout: 10000 });
      } catch (e) {
        log.push(`waitForSelector timed out: ${e.message}`);
      }
      outcome.finalUrl = page.url();
      outcome.title = await page.title();
      outcome.results = await extractResults(page, maxResults);
      outcome.ok = outcome.results.length > 0;
      if (!outcome.ok) outcome.error = 'no result anchors matched';
    }
  } catch (e) {
    outcome.error = e.message;
  } finally {
    outcome.elapsedMs = Date.now() - startedAt;
    try { await ctx.close(); } catch {}
  }

  process.stdout.write(JSON.stringify(outcome));
})().catch((e) => {
  process.stdout.write(
    JSON.stringify({ ok: false, error: e.message, stack: e.stack, provider: 'playwright_google' })
  );
  process.exit(1);
});
