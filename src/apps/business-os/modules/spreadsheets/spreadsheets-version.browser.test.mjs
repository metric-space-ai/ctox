import test from 'node:test';
import assert from 'node:assert/strict';
import { readFile } from 'node:fs/promises';
import { fileURLToPath } from 'node:url';
import { createServer } from 'node:http';
import { createRequire } from 'node:module';
import { homedir } from 'node:os';
import { join as joinPath } from 'node:path';
// Lader (OLDTEST): esbuild / playwright aus dem Klon, sonst ESBUILD_MODULE / PLAYWRIGHT_MODULE, sonst aus dem Haupt-Checkout
// ~/Documents/ctox/src/apps/business-os/node_modules (wie die v4-api-Tests mit PLAYWRIGHT_MODULE). Nicht installieren.
const requireDev = createRequire(import.meta.url);
function loadDev(name, envVar) {
  try { return requireDev(name); } catch {
    return requireDev(process.env[envVar] || joinPath(homedir(), 'Documents/ctox/src/apps/business-os/node_modules', name));
  }
}
const { build } = loadDev('esbuild', 'ESBUILD_MODULE');
const { chromium } = loadDev('playwright', 'PLAYWRIGHT_MODULE');
// Lader (OLDTEST): wie der Service Worker – ctox:shared/x → shared/x (§16.15); ctox:lib/<name>@<major>/ ist eine
// Shell-Standardbibliothek, die erst zur Laufzeit gebunden wird (dynamischer Import) – im Bündel extern lassen.
const CTOX_SPECIFIERS = {
  name: 'ctox-specifiers',
  setup(b) {
    b.onResolve({ filter: /^ctox:shared\// }, (args) => ({ path: fileURLToPath(new URL(`../../shared/${args.path.slice('ctox:shared/'.length)}`, import.meta.url)) }));
    b.onResolve({ filter: /^ctox:lib\// }, (args) => ({ path: args.path, external: true }));
  },
};

// Isolated UI regression, not a substitute for native WebRTC production E2E.
test('A missing spreadsheet version goes to the status zone, never as text or a saved badge in the content', async () => {
  // App-API 4.0 (R4/R11): Versionen liegen lokal vor (api.get/api.query); es gibt keinen Transportfehler und damit
  // keinen Wiederholen-Knopf mehr. Fehlt die Version, meldet die App das in der Shell-Zone (api.status) – der Inhalt
  // zeigt weder Fehlertext noch ein „gespeichert“-Abzeichen, und es gibt keine rechte Spalte.
  const [html, css, officeCss, translations, bundle] = await Promise.all([
    readFile(new URL('./index.html', import.meta.url), 'utf8'),
    readFile(new URL('./index.css', import.meta.url), 'utf8'),
    readFile(new URL('../../shared/office-workspace.css', import.meta.url), 'utf8'),
    readFile(new URL('./locales/en.json', import.meta.url), 'utf8'),
    build({ entryPoints: [fileURLToPath(new URL('./index.js', import.meta.url))],
      bundle: true, platform: 'browser', format: 'esm', write: false, plugins: [CTOX_SPECIFIERS] }),
  ]);
  const fixture = `<!doctype html><html><head><link rel="stylesheet" href="/index.css"></head>
    <body><div id="host" data-app="spreadsheets">${html}</div><script type="module">
      import { __spreadsheetsTestHooks as hooks } from '/module.js';
      const translations = ${translations};
      const t = (key, fallback) => translations[key] || fallback || key;
      window.__reports = [];
      const state = {
        selectedId: 'sheet', selectedVersion: null, editorHandle: null,
        spreadsheets: [{ id: 'sheet', title: 'Load regression', filename: 'example.xlsx', current_version_id: 'version' }],
        dirty: false, saving: false, t,
        root: document.querySelector('#host'),
        api: {
          get: () => ({ get: () => null }),
          query: () => ({ get: () => [], subscribe: () => () => {} }),
          status: { report: (entry) => window.__reports.push(entry), clear() {} },
          metrics: { get: () => ({ backlog: 0 }) },
        },
      };
      await hooks.loadSelectedVersion(state);
      await hooks.renderCenter(state);
      document.body.dataset.ready = 'true';
    </script></body></html>`;
  const server = createServer((request, response) => {
    const pathname = new URL(request.url, 'http://127.0.0.1').pathname;
    const route = pathname === '/module.js' ? ['text/javascript', bundle.outputFiles[0].text]
      : pathname === '/index.css' ? ['text/css', css]
        : pathname === '/shared/office-workspace.css' ? ['text/css', officeCss]
          : pathname === '/' ? ['text/html', fixture] : null;
    response.writeHead(route ? 200 : 404, { 'content-type': route?.[0] || 'text/plain' });
    response.end(route?.[1] || 'Not found');
  });
  let browser;
  try {
    await new Promise(resolve => server.listen(0, '127.0.0.1', resolve));
    browser = await chromium.launch({ headless: true,
      ...(process.env.PLAYWRIGHT_CHROMIUM_EXECUTABLE_PATH
        ? { executablePath: process.env.PLAYWRIGHT_CHROMIUM_EXECUTABLE_PATH } : {}) });
    const page = await browser.newPage();
    const errors = [];
    page.on('pageerror', error => errors.push(error.message));
    await page.goto(`http://127.0.0.1:${server.address().port}/`);
    await page.waitForSelector('body[data-ready="true"]');
    const reports = await page.evaluate(() => window.__reports);
    assert.deepEqual(reports.map((entry) => entry.code), ['spreadsheets.version.missing']);
    const canvasText = await page.locator('[data-spreadsheets-canvas]').count()
      ? await page.locator('[data-spreadsheets-canvas]').innerText() : '';
    assert.doesNotMatch(canvasText, /No saved version|Retry|Timed out/);
    assert.equal(await page.locator('[data-spreadsheets-retry-version]').count(), 0);
    assert.equal(await page.locator('[data-spreadsheets-dirty-indicator]').isVisible(), false);
    assert.equal(await page.locator('[data-resizer="right"], [data-spreadsheets-head="runbooks"]').count(), 0);
    assert.deepEqual(errors, []);
  } finally {
    await browser?.close();
    server.closeAllConnections();
    await new Promise(resolve => server.close(resolve));
  }
});
