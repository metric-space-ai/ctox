// Editor-Bibliothek createCodeEditor in echtem Chromium (Shell v4). Kein Kern-Stub nötig: die Bibliothek hat keinen
// Datenweg; der Test prüft Schnittstelle, onChange, snapshot/restored, readOnly, Diff und destroy.
// Lauf: node --test src/apps/business-os/desktop-apps/code-editor/tests/v4-api.test.mjs
import { test, before, after } from 'node:test';
import assert from 'node:assert/strict';
import { createServer } from 'node:http';
import { readFile, mkdir } from 'node:fs/promises';
import { createRequire } from 'node:module';
import { fileURLToPath } from 'node:url';
import { dirname, join, normalize, extname } from 'node:path';
import { homedir } from 'node:os';

const HERE = dirname(fileURLToPath(import.meta.url));
const BOS = normalize(join(HERE, '../../..'));
const SHOTS = join(HERE, 'shots');
const require = createRequire(import.meta.url);
const { chromium } = require(process.env.PLAYWRIGHT_MODULE || join(homedir(), 'Documents/ctox/src/apps/business-os/node_modules/playwright'));
const TYPES = { '.js': 'text/javascript', '.mjs': 'text/javascript', '.css': 'text/css', '.html': 'text/html', '.json': 'application/json' };
const PAGE = `<!doctype html><html><head><meta charset="utf-8"><link rel="stylesheet" href="/app.css">
<style>html,body{margin:0;height:100%;overflow:hidden;background:var(--bg)} #win{width:100vw;height:100vh}</style></head><body><div id="win"></div></body></html>`;

let server, browser, page;
before(async () => {
  server = createServer(async (req, res) => {
    const path = decodeURIComponent(new URL(req.url, 'http://x').pathname);
    if (path === '/') { res.writeHead(200, { 'content-type': 'text/html' }); res.end(PAGE); return; }
    const file = normalize(join(BOS, path));
    if (!file.startsWith(BOS)) { res.writeHead(403); res.end(); return; }
    try { const body = await readFile(file); res.writeHead(200, { 'content-type': TYPES[extname(file)] || 'application/octet-stream' }); res.end(req.method === 'HEAD' ? undefined : body); }
    catch { res.writeHead(404); res.end(); }
  });
  await new Promise((r) => server.listen(0, '127.0.0.1', r));
  browser = await chromium.launch();
  page = await browser.newPage({ viewport: { width: 1280, height: 720 } });
  page.__errors = [];
  page.on('pageerror', (e) => page.__errors.push(e.message));
  await page.goto(`http://127.0.0.1:${server.address().port}/`);
  await page.evaluate(async () => { window.__lib = await import('/desktop-apps/code-editor/app.js'); window.__changes = []; });
});
after(async () => { await browser?.close(); await new Promise((r) => server?.close(r)); });

const FILES = [
  { path: 'index.js', content: 'export function mount(root) {\n  root.textContent = "hi";\n}\n' },
  { path: 'index.css', content: '.a { color: red; }\n' },
  { path: 'module.json', content: '{"id":"demo"}' },
];

test('createCodeEditor (textarea): Dateiliste, Startdatei index.js, onChange je Eingabe, dirty, kein Zustandstext', async () => {
  await page.evaluate(async (FILES) => {
    window.__ed = await window.__lib.createCodeEditor({ root: document.getElementById('win'), files: FILES, prefer: 'textarea', onChange: (path, content, meta) => window.__changes.push({ path, content, dirty: meta.dirty }) });
  }, FILES);
  assert.equal(await page.evaluate(() => window.__ed.kind), 'textarea');
  assert.equal(await page.evaluate(() => window.__ed.activePath()), 'index.js');
  assert.deepEqual(await page.$$eval('[data-ce-files] button', (b) => b.map((x) => x.dataset.path)), ['index.css', 'index.js', 'module.json']);
  await page.fill('.ctox-code-editor__text', 'export const x = 1;\n');
  const last = await page.evaluate(() => window.__changes.at(-1));
  assert.deepEqual(last, { path: 'index.js', content: 'export const x = 1;\n', dirty: true });
  assert.equal(await page.evaluate(() => window.__ed.isDirty('index.js')), true);
  assert.equal(await page.evaluate(() => document.querySelector('[data-ce-files] .is-active').classList.contains('is-dirty')), true);
  assert.equal(await page.evaluate(() => Boolean(document.querySelector('[role="status"], .ctox-syncing'))), false);
  assert.deepEqual(page.__errors, []);
});

test('Diff zeigt die Änderung gegen den Ausgangsstand; Verwerfen setzt zurück und meldet dirty:false', async () => {
  await page.click('[data-ce-diff]');
  assert.match(await page.$eval('[data-ce-diff-panel]', (el) => el.textContent), /\+ export const x = 1;/);
  await page.click('[data-ce-revert]');
  assert.equal(await page.evaluate(() => window.__ed.isDirty()), false);
  assert.equal(await page.evaluate(() => window.__changes.at(-1).dirty), false);
  assert.match(await page.$eval('.ctox-code-editor__text', (el) => el.value), /root\.textContent/);
});

test('Format (json) über die Leiste; readOnly sperrt Format/Verwerfen und die Eingabe', async () => {
  await page.click('[data-ce-files] [data-path="module.json"]');
  await page.click('[data-ce-format]');
  assert.equal(await page.evaluate(() => window.__ed.getValue()), '{\n  "id": "demo"\n}\n');
  await page.evaluate(() => window.__ed.setReadOnly(true));
  assert.equal(await page.$eval('[data-ce-format]', (b) => b.disabled), true);
  assert.equal(await page.$eval('.ctox-code-editor__text', (t) => t.readOnly), true);
  await page.evaluate(() => window.__ed.setReadOnly(false));
});

test('snapshot/restored: offene Entwürfe, aktive Datei, Suche und Diff überleben destroy + Neuaufbau; setFiles behält Entwürfe', async () => {
  await page.click('[data-ce-files] [data-path="index.css"]');
  await page.fill('.ctox-code-editor__text', '.a { color: blue; }\n');
  await page.fill('[data-ce-search]', 'css');
  const snap = await page.evaluate(async (FILES) => {
    const s = structuredClone(window.__ed.snapshot());
    window.__ed.destroy();
    window.__ed = await window.__lib.createCodeEditor({ root: document.getElementById('win'), files: FILES, prefer: 'textarea', restored: s });
    return s;
  }, FILES);
  assert.equal(snap.activePath, 'index.css');
  assert.equal(snap.search, 'css');
  assert.ok(snap.drafts['index.css']);
  assert.equal(await page.evaluate(() => window.__ed.activePath()), 'index.css');
  assert.equal(await page.$eval('.ctox-code-editor__text', (el) => el.value), '.a { color: blue; }\n');
  assert.deepEqual(await page.$$eval('[data-ce-files] button', (b) => b.map((x) => x.dataset.path)), ['index.css']);
  await page.evaluate((FILES) => window.__ed.setFiles([...FILES, { path: 'neu.md', content: '# neu' }]), FILES);
  assert.equal(await page.evaluate(() => window.__ed.getValue('index.css')), '.a { color: blue; }\n', 'Live-Aktualisierung der Dateien verwirft keinen Entwurf');
  await page.fill('[data-ce-search]', '');
  await page.dispatchEvent('[data-ce-search]', 'input');
  await page.click('[data-ce-diff]');
  await mkdir(SHOTS, { recursive: true });
  await page.screenshot({ path: join(SHOTS, 'code-editor-1280.png') });
  await page.setViewportSize({ width: 420, height: 720 });
  await page.waitForTimeout(100);
  await page.screenshot({ path: join(SHOTS, 'code-editor-420.png') });
  assert.equal(await page.evaluate(() => document.getElementById('win').scrollWidth > window.innerWidth + 1), false);
  await page.setViewportSize({ width: 1280, height: 720 });
});

test('Monaco aus vendor/monaco, falls vorhanden; destroy räumt das root', async () => {
  const kind = await page.evaluate(async (FILES) => {
    window.__ed.destroy();
    const ed = await window.__lib.createCodeEditor({ root: document.getElementById('win'), files: FILES, readOnly: true });
    const k = ed.kind;
    ed.destroy();
    return { k, children: document.getElementById('win').children.length };
  }, FILES);
  assert.ok(['monaco', 'textarea'].includes(kind.k));
  assert.equal(kind.children, 0);
  assert.deepEqual(page.__errors, []);
});
