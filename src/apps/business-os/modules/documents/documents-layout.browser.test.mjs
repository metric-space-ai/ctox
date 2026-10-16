import test from 'node:test';
import assert from 'node:assert/strict';
import { createServer } from 'node:http';
import { readFile } from 'node:fs/promises';

import { createRequire } from 'node:module';
import { homedir } from 'node:os';
import { join as joinPath } from 'node:path';
// Lader (OLDTEST): playwright aus dem Klon, sonst PLAYWRIGHT_MODULE, sonst aus dem Haupt-Checkout
// ~/Documents/ctox/src/apps/business-os/node_modules (wie die v4-api-Tests mit PLAYWRIGHT_MODULE). Nicht installieren.
const requireDev = createRequire(import.meta.url);
function loadDev(name, envVar) {
  try { return requireDev(name); } catch {
    return requireDev(process.env[envVar] || joinPath(homedir(), 'Documents/ctox/src/apps/business-os/node_modules', name));
  }
}
const { chromium } = loadDev('playwright', 'PLAYWRIGHT_MODULE');

const MODULE_HTML = new URL('./index.html', import.meta.url);
const MODULE_CSS = new URL('./index.css', import.meta.url);
const RESIZER_JS = new URL('../../shared/resizer.js', import.meta.url);
const OFFICE_CSS = new URL('../../shared/office-workspace.css', import.meta.url);

async function startFixtureServer() {
  const [moduleHtml, moduleCss, resizerSource, officeCss] = await Promise.all([
    readFile(MODULE_HTML, 'utf8'),
    readFile(MODULE_CSS, 'utf8'),
    readFile(RESIZER_JS, 'utf8'),
    readFile(OFFICE_CSS, 'utf8'),
  ]);
  const pageHtml = `<!doctype html>
    <html>
      <head>
        <meta charset="utf-8">
        <style>
          :root {
            --bg: #f4f7f8;
            --surface: #eef3f4;
            --surface-2: #e5ecee;
            --text: #172126;
            --muted: #66747b;
            --line: #b7c2c7;
            --line-strong: #809097;
            --accent: #087f8c;
            --control-radius: 4px;
            --surface-radius: 6px;
            --shadow: 0 8px 24px rgb(10 25 30 / 18%);
          }
          html, body, #host { width: 100%; height: 100%; margin: 0; overflow: hidden; }
        </style>
        <link rel="stylesheet" href="/index.css">
      </head>
      <body>
        <!-- v4: die Shell setzt data-app am Fenster-Root; index.css ist darauf gescoped (scope-css). -->
        <div id="host" data-app="documents">${moduleHtml}</div>
        <script type="module">
          import { CtoxResizer } from '/resizer.js';
          const root = document.querySelector('[data-resize-frame]');
          for (const handle of root.querySelectorAll('[data-resizer-var]')) {
            new CtoxResizer({
              resizerEl: handle,
              containerEl: root,
              cssVar: handle.dataset.resizerVar,
              side: handle.dataset.resizer,
              minWidth: Number(handle.dataset.resizerMin),
              maxWidth: Number(handle.dataset.resizerMax),
            });
          }
          window.setCompact = (compact) => root.classList.toggle('is-compact', compact);
          window.setLibraryOpen = (open) => root.classList.toggle('is-library-open', open);
        </script>
      </body>
    </html>`;
  const server = createServer((request, response) => {
    const path = new URL(request.url, 'http://127.0.0.1').pathname;
    const payload = path === '/index.css'
      ? moduleCss
      : path === '/resizer.js'
        ? resizerSource
        : path === '/shared/office-workspace.css'
          ? officeCss
          : pageHtml;
    response.writeHead(200, {
      'content-type': path.endsWith('.css')
        ? 'text/css; charset=utf-8'
        : path.endsWith('.js')
          ? 'text/javascript; charset=utf-8'
          : 'text/html; charset=utf-8',
    });
    response.end(payload);
  });
  await new Promise((resolve, reject) => {
    server.once('error', reject);
    server.listen(0, '127.0.0.1', resolve);
  });
  const address = server.address();
  return {
    url: `http://127.0.0.1:${address.port}/`,
    close: () => new Promise((resolve, reject) => {
      server.closeAllConnections();
      server.close((error) => error ? reject(error) : resolve());
    }),
  };
}

test('Documents has only a resizable file library and editor, with a left library overlay on narrow screens', async () => {
  const fixture = await startFixtureServer();
  let browser;
  try {
    browser = await chromium.launch({
      headless: true,
      ...(process.env.PLAYWRIGHT_CHROMIUM_EXECUTABLE_PATH
        ? { executablePath: process.env.PLAYWRIGHT_CHROMIUM_EXECUTABLE_PATH }
        : {}),
    });
    const page = await browser.newPage({ viewport: { width: 1800, height: 800 } });
    await page.goto(fixture.url, { waitUntil: 'networkidle' });
    const root = page.locator('[data-documents-module]');
    const leftResizer = page.locator('[data-resizer="left"]');
    const library = page.locator('.documents-library-pane');
    const editor = page.locator('.documents-workbench');
    const assertNoRightColumn = async () => {
      assert.equal(await page.locator('[data-documents-actions-drawer], [data-documents-actions-resizer], [data-documents-actions-toggle], [data-resizer="right"]').count(), 0,
        'Documents must not render any right actions column, drawer, resizer or toggle');
    };

    await assertNoRightColumn();
    assert.equal(await root.evaluate((element) => getComputedStyle(element).gridTemplateColumns.split(' ').length), 3);
    await leftResizer.focus();
    await leftResizer.press('Home');
    assert.equal(await root.evaluate((element) => element.style.getPropertyValue('--shell-col-left')), '300px');
    await leftResizer.press('ArrowRight');
    assert.equal(await root.evaluate((element) => element.style.getPropertyValue('--shell-col-left')), '324px');
    await leftResizer.press('End');
    assert.equal(await root.evaluate((element) => element.style.getPropertyValue('--shell-col-left')), '560px');
    assert.equal(Math.round((await library.boundingBox()).width), 560);
    assert.ok(
      Math.round((await editor.boundingBox()).width) >= 480,
      'the editor must retain usable width with the library maximized',
    );

    await page.setViewportSize({ width: 1180, height: 800 });
    await assertNoRightColumn();
    assert.ok(
      Math.round((await editor.boundingBox()).width) >= 480,
      'the editor must retain usable width at the narrower desktop size',
    );

    await page.setViewportSize({ width: 720, height: 800 });
    await page.evaluate(() => window.setCompact(true));
    await assertNoRightColumn();
    assert.equal(await root.evaluate((element) => getComputedStyle(element).gridTemplateColumns.split(' ').length), 1);
    assert.equal(await library.evaluate((element) => getComputedStyle(element).display), 'none');
    assert.equal(await leftResizer.evaluate((element) => getComputedStyle(element).display), 'none');
    assert.equal(Math.round((await editor.boundingBox()).width), 720);
    await page.evaluate(() => window.setLibraryOpen(true));
    assert.equal(await library.evaluate((element) => getComputedStyle(element).position), 'absolute');
    const libraryBox = await library.boundingBox();
    assert.equal(Math.round(libraryBox.x), 0, 'the compact library opens from the left');
    assert.ok(libraryBox.width >= 300 && libraryBox.width <= 720 - 32);
    assert.equal(Math.round((await editor.boundingBox()).width), 720, 'the overlay does not squeeze the editor');
    await assertNoRightColumn();
    await page.evaluate(() => window.setLibraryOpen(false));
    assert.equal(await library.evaluate((element) => getComputedStyle(element).display), 'none');
  } finally {
    await browser?.close();
    await fixture.close();
  }
});
