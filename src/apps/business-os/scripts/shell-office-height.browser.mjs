#!/usr/bin/env node
// Real stylesheet/loaded-wrapper geometry, not a live Office or persistence test.
import assert from 'node:assert/strict';
import { createHash } from 'node:crypto';
import { readFileSync, existsSync, mkdirSync, writeFileSync } from 'node:fs';
import http from 'node:http';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { chromium } from 'playwright';

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');
const outputIndex = process.argv.indexOf('--output-dir');
assert(outputIndex >= 0, 'Pass --output-dir on the disposable development volume');
const output = path.resolve(process.argv[outputIndex + 1]);
mkdirSync(output, { recursive: true });
const report = { stylesheetSha256: createHash('sha256').update(readFileSync(path.join(root, 'app.css'))).digest('hex'), cases: [] };
const fixture = (app, sheetOnly, panes) => {
  const markup = app === 'generic' ? '<article><button>Start center</button>'
    + '<p>Ordinary scrolling application content.</p>'.repeat(80) + '<button>End center</button></article>'
    : readFileSync(path.join(root, 'modules', app, 'index.html'), 'utf8');
  const side = (name) => panes ? `<div>${'<p>Stacked pane content</p>'.repeat(panes === 'tall' ? 25 : 1)}<button>End ${name}</button></div>` : '';
  return `<!doctype html><html data-theme="dark" data-shell-style="ctox"><head>
    <link rel="stylesheet" href="/app.css"><link rel="stylesheet" href="/shared/base.css">
    ${app === 'generic' ? '' : `<link rel="stylesheet" href="/modules/${app}/index.css">`}
    </head><body><div class="${sheetOnly ? 'lab-desk' : 'shell-window-layer'}">
    <section class="shell-window is-mobile-sheet is-focused" data-shell-contract="v2"
      data-shell-window-chrome="shared-v2" data-shell-header-rows="2" data-shell-icon-rows="2"
      style="position:absolute;left:0;top:56px;width:100%;height:654px">
      <header class="shell-window-header" data-window-header></header>
      <div class="shell-window-content"><div class="module-root shell-window-module-root" data-module-root="${app}">
        <aside class="module-context shell-window-module-pane shell-window-module-pane--left">${side('left')}</aside>
        <button class="ctox-column-resizer shell-window-module-column-resizer shell-window-module-column-resizer--left"></button>
        <main class="module-content" tabindex="0">${markup}</main>
        <button class="ctox-column-resizer shell-window-module-column-resizer shell-window-module-column-resizer--right"></button>
        <aside class="module-context shell-window-module-pane shell-window-module-pane--right">${side('right')}</aside>
      </div></div>
    </section></div></body></html>`;
};
const mime = { '.css': 'text/css', '.js': 'text/javascript', '.mjs': 'text/javascript', '.svg': 'image/svg+xml', '.woff2': 'font/woff2' };
const server = http.createServer((req, res) => {
  const url = new URL(req.url, 'http://localhost');
  if (url.pathname === '/fixture') {
    res.writeHead(200, { 'content-type': 'text/html' });
    return res.end(fixture(url.searchParams.get('app'), url.searchParams.has('sheet-only'), url.searchParams.get('panes')));
  }
  const file = path.resolve(root, `.${decodeURIComponent(url.pathname)}`);
  if (!file.startsWith(`${root}${path.sep}`) || !existsSync(file)) { res.writeHead(404); return res.end(); }
  res.writeHead(200, { 'content-type': mime[path.extname(file)] || 'application/octet-stream' });
  res.end(readFileSync(file));
});
await new Promise(resolve => server.listen(0, '127.0.0.1', resolve));
let browser;
try {
  browser = await chromium.launch({ headless: true, ...(process.env.PLAYWRIGHT_CHROMIUM_EXECUTABLE_PATH ? { executablePath: process.env.PLAYWRIGHT_CHROMIUM_EXECUTABLE_PATH } : {}) });
  const page = await browser.newPage({ viewport: { width: 516, height: 710 }, deviceScaleFactor: 1 });
  const errors = [];
  page.on('pageerror', error => errors.push(error.message));
  async function run(app, width, { sheetOnly = false, panes = '' } = {}) {
    const name = `${app}-${width}-${sheetOnly ? 'sheet-only' : 'layer'}-${panes || 'empty'}`;
    const result = { name };
    try {
      await page.setViewportSize({ width, height: 710 });
      await page.goto(`http://127.0.0.1:${server.address().port}/fixture?app=${app}${sheetOnly ? '&sheet-only' : ''}&panes=${panes}`, { waitUntil: 'load' });
      if (app !== 'generic') await page.evaluate(async app => {
        let host, mount;
        if (app === 'documents') {
          const module = document.querySelector('.documents-module');
          const width = module.getBoundingClientRect().width;
          module.classList.toggle('is-compact', width <= 768);
          module.classList.toggle('is-narrow', width <= 620);
          module.classList.toggle('is-phone', width <= 440);
          module.classList.toggle('is-actions-overlay', width < 1616);
          host = document.querySelector('[data-documents-editor]');
          mount = document.createElement('div');
          mount.className = 'documents-superdoc-frame documents-ctox-documents-frame';
        } else {
          const editor = document.querySelector('[data-spreadsheets-editor]');
          const header = document.querySelector('template[data-spreadsheets-head="editor"]').content.cloneNode(true);
          host = document.createElement('div');
          host.className = 'spreadsheets-editor-canvas';
          editor.replaceChildren(header, host);
          mount = document.createElement('div');
          mount.className = 'spreadsheets-ctox-spreadsheets-frame';
          mount.style.cssText = 'width:100%;height:100%;min-height:0';
        }
        host.replaceChildren(mount);
        const frame = document.createElement('iframe');
        frame.title = 'Loaded editor geometry fixture';
        // Same outer frame style as createOfficeEditor; no override of ancestors.
        frame.style.cssText = 'display:block;width:100%;height:100%;border:0;background:transparent';
        const loaded = new Promise(resolve => frame.addEventListener('load', resolve, { once: true }));
        frame.srcdoc = '<!doctype html><html><body>Editor geometry fixture</body></html>';
        mount.append(frame);
        await loaded;
      }, app);
      await page.evaluate(async () => {
        await document.fonts.ready;
        await new Promise(resolve => requestAnimationFrame(() => requestAnimationFrame(resolve)));
      });
      const geometry = await page.evaluate(() => {
        const root = document.querySelector('.shell-window-module-root');
        const center = root.querySelector(':scope > .module-content');
        const style = getComputedStyle(root);
        const height = n => n?.getBoundingClientRect().height || 0;
        const left = root.querySelector(':scope > .shell-window-module-pane--left');
        const right = root.querySelector(':scope > .shell-window-module-pane--right');
        const header = center.querySelector('.documents-document-strip, .spreadsheets-editor-header');
        return {
          root: height(root), center: height(center), frame: height(center.querySelector('iframe')),
          header: height(header), left: height(left), right: height(right),
          leftTop: left.getBoundingClientRect().top, centerTop: center.getBoundingClientRect().top, rightTop: right.getBoundingClientRect().top,
          inner: root.clientHeight - parseFloat(style.paddingTop) - parseFloat(style.paddingBottom),
          paddingBottom: parseFloat(style.paddingBottom), rowGap: parseFloat(style.rowGap) || 0,
          rows: style.gridTemplateRows, centerScrollHeight: center.scrollHeight, centerClientHeight: center.clientHeight,
          leftScrollHeight: left.scrollHeight, leftClientHeight: left.clientHeight,
          rightScrollHeight: right.scrollHeight, rightClientHeight: right.clientHeight,
          rootScrollHeight: root.scrollHeight, rootClientHeight: root.clientHeight,
        };
      });
      result.geometry = geometry;
      assert(geometry.root > 500, 'Fixture must provide a full-height shell window');
      if (width <= 767 && panes !== 'tall') {
        const available = geometry.inner - geometry.left - geometry.right - 2 * geometry.rowGap;
        assert(Math.abs(geometry.center - available) <= 1, `Center underfills: ${geometry.center}px of ${available}px available`);
        assert.equal(geometry.paddingBottom, 76, 'Keep zero-inset dock reservation');
      }
      if (app !== 'generic') {
        assert(Math.abs(geometry.frame + geometry.header - geometry.center) <= 1, 'Editor must use center minus actual toolbar');
        assert(geometry.frame > 300, `Unusable editor height: ${geometry.frame}`);
        assert.equal(await page.locator('[data-shell-column="right"]').count(), 0, 'No right Office column');
      } else {
        if (panes) {
          assert(geometry.left > 0 && geometry.right > 0);
          assert(geometry.leftTop < geometry.centerTop && geometry.centerTop < geometry.rightTop);
          assert(geometry.leftTop + geometry.left <= geometry.centerTop + 1, 'Left pane must not overlap the center');
          assert(geometry.centerTop + geometry.center <= geometry.rightTop + 1, 'Center must not overlap the right pane');
        }
        assert(geometry.centerScrollHeight > geometry.centerClientHeight, 'Long ordinary content stays scrollable');
        if (panes === 'tall') {
          // Existing panes scroll internally when the auto tracks shrink.
          // Require reachable overflow, not a change to which element scrolls.
          assert(geometry.leftScrollHeight > geometry.leftClientHeight);
          assert(geometry.rightScrollHeight > geometry.rightClientHeight);
          assert(geometry.center >= 220, 'Center keeps its declared minimum');
        }
        for (const label of panes ? ['left', 'center', 'right'] : ['center']) {
          await page.getByRole('button', { name: `End ${label}`, exact: true }).click();
          const visible = await page.getByRole('button', { name: `End ${label}`, exact: true }).evaluate(button => {
            const box = button.getBoundingClientRect();
            const pane = button.closest('.module-content, .shell-window-module-pane');
            const clip = pane.getBoundingClientRect();
            return box.top >= clip.top - 1 && box.bottom <= clip.bottom + 1 && box.top >= 0 && box.bottom <= innerHeight;
          });
          assert(visible, `End ${label} is reachable inside its scroll viewport`);
        }
      }
      assert.deepEqual(errors, []);
      result.pass = true;
    } catch (error) {
      result.pass = false;
      result.error = error.message;
    }
    await page.screenshot({ path: path.join(output, `${name}.png`) });
    report.cases.push(result);
    console.log(JSON.stringify(result));
  }
  for (const width of [390, 516, 720, 1180]) for (const app of ['documents', 'spreadsheets']) await run(app, width);
  for (const app of ['documents', 'spreadsheets']) await run(app, 516, { sheetOnly: true });
  for (const panes of ['', 'short', 'tall']) await run('generic', 516, { panes });
  writeFileSync(path.join(output, 'report.json'), JSON.stringify(report, null, 2));
  assert(report.cases.every(result => result.pass), `${report.cases.filter(result => !result.pass).length} shell height regressions failed`);
} finally {
  await browser?.close();
  await new Promise(resolve => server.close(resolve));
}
