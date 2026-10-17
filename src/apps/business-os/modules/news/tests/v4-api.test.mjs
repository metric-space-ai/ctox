// news gegen den App-API-4.0-Kern-Stub (createStubKernel, read-only unter /__stub/) in echtem Chromium.
// Stub: CTOX_APP_API_STUB (Pflicht-Pin ~/.local/state/shell-v4/stub-pins/4b0d0ac), sonst ~/.local/state/ctox-sync-v4/stub.
// Lauf: CTOX_APP_API_STUB=~/.local/state/shell-v4/stub-pins/4b0d0ac node --test src/apps/business-os/modules/news/tests/v4-api.test.mjs
// Screenshots (1280/420) nach tests/shots/ mit NEWS_SHOTS=1.
import { test, before, after } from 'node:test';
import assert from 'node:assert/strict';
import { createServer } from 'node:http';
import { readFile } from 'node:fs/promises';
import { createRequire } from 'node:module';
import { fileURLToPath } from 'node:url';
import { dirname, join, normalize, extname } from 'node:path';
import { homedir } from 'node:os';
import { realManifestRoutes } from '../../../v4/tools/port/real-manifest.mjs';

// Gemeinsamer Pin 4b0d0acb (Kern M7-FINISH): der Stub veröffentlicht das ctox-platform-Manifest selbst (aus der Repo-Datei).
const STUB = process.env.CTOX_APP_API_STUB || join(homedir(), '.local/state/shell-v4/stub-pins/4b0d0ac');
const HERE = dirname(fileURLToPath(import.meta.url));
const BOS = normalize(join(HERE, '../../..'));
const require = createRequire(import.meta.url);
const { chromium } = require(process.env.PLAYWRIGHT_MODULE || join(homedir(), 'Documents/ctox/src/apps/business-os/node_modules/playwright'));
const TYPES = { '.js': 'text/javascript', '.mjs': 'text/javascript', '.css': 'text/css', '.html': 'text/html', '.json': 'application/json', '.svg': 'image/svg+xml' };
const APP = 'news';

const PAGE = `<!doctype html><html><head><meta charset="utf-8">
<link rel="stylesheet" href="/ui-contract/v1/workjet-ui-contract.css"><link rel="stylesheet" href="/app.css"><link rel="stylesheet" href="/shared/base.css">
<link rel="stylesheet" href="/modules/${APP}/index.css">
<style>html,body{margin:0;height:100%;background:var(--bg,#f6f6f4)}#win{width:1280px;height:760px;position:relative;overflow:clip;contain:layout paint;container-type:inline-size;container-name:business-app-window}</style>
</head><body><div id="win"></div></body></html>`;

let server, browser, page, base;

before(async () => {
  server = createServer(async (req, res) => {
    const path = decodeURIComponent(new URL(req.url, 'http://x').pathname);
    if (path === '/') { res.writeHead(200, { 'content-type': 'text/html' }); res.end(PAGE); return; }
    const [rootDir, rel] = path.startsWith('/__stub/') ? [STUB, path.slice('/__stub'.length)] : [BOS, path];
    const file = normalize(join(rootDir, rel));
    if (!file.startsWith(rootDir)) { res.writeHead(403); res.end(); return; }
    try {
      let body = await readFile(file);
      // Wie der Service Worker (§16.15): ctox:shared/x.js → Shell-Pfad.
      if (/\.m?js$/.test(file)) body = Buffer.from(body.toString('utf8').replace(/(['"])ctox:shared\//g, '$1/shared/'));
      res.writeHead(200, { 'content-type': TYPES[extname(file)] || 'application/octet-stream' }); res.end(body);
    } catch { res.writeHead(404); res.end(); }
  });
  await new Promise((r) => server.listen(0, '127.0.0.1', r));
  base = `http://127.0.0.1:${server.address().port}/`;
  browser = await chromium.launch();
  page = await browser.newPage({ viewport: { width: 1280, height: 760 } });
  const errors = [];
  page.on('pageerror', (e) => errors.push(e.message));
  page.on('console', (m) => { if (m.type() === 'error') console.error('[page]', m.text()); });
  page.__errors = errors;
  await page.goto(base);
  // Befehlsrouten der echten news-Version (v4/manifests/news.manifest.json); ohne aktive Version lehnt der Kern
  // seit E26/E27 news.issue.generate beim Schreiben mit unknown_command ab.
  await page.evaluate((routes) => { window.__routes = routes; }, realManifestRoutes(APP));
  await page.evaluate(async (APP) => {
    const { createStubKernel } = await import('/__stub/ctox-app-api-stub.mjs');
    window.__mod = await import(`/modules/${APP}/index.js`);
    const kernel = createStubKernel({ storage: 'memory', latencyMs: 5, multiTab: false });
    await kernel.ready;
    // FIXT-R: Eigentümer der Fremd-Collections (echte Manifeste, inkl. ctox-platform) vor der eigenen Version.
    for (const o of window.__routes?._fixture_owners ?? []) if (!(await kernel.sim.inspect('business_apps', o.app))?.data?.active_ver) await kernel.sim.publishAppVersion(o.app, o.ver, o.publish);
    await kernel.sim.publishAppVersion(APP, '1', { title: 'News', manifestExtra: window.__routes });
    // Stub 4b0d0acb: der App-Handler news.issue.generate ist veröffentlicht, sein Artefakt nicht – ein expliziter Simulator
    // liefert den Erfolg (E42.1: Simulator vor Artefakt; failCommand bleibt für den Fehlerlauf zuerst).
    kernel.sim.onCommand('news.issue.generate', (payload) => ({ ok: true, issue_id: payload?.issue_id ?? null }));
    const ts = Date.now();
    await kernel.sim.push('news_topics', [
      { id: 'topic_ai', title: 'KI-Labs', prompt: 'Neue Modelle', cadence: 'daily', run_time: '07:00', cron_expr: '0 7 * * *', enabled: true, language: 'de', max_posts: 12, categories: [], payload: {}, created_at_ms: ts, updated_at_ms: ts },
    ]);
    await kernel.sim.push('news_sources', [
      { id: 'src_1', topic_id: 'topic_ai', kind: 'lab', label: 'OpenAI', url: 'https://openai.com/news/', enabled: true, status: 'active', payload: {}, created_at_ms: ts, updated_at_ms: ts },
    ]);
    await kernel.sim.push('news_issues', [
      { id: 'issue_1', topic_id: 'topic_ai', title: 'KI-Labs', issue_date: '2026-10-15', status: 'ready', command_id: '', task_id: '', post_count: 1, generated_at_ms: ts, error: '', payload: {}, created_at_ms: ts, updated_at_ms: ts },
    ]);
    await kernel.sim.push('news_posts', [
      { id: 'issue_1_p01', issue_id: 'issue_1', topic_id: 'topic_ai', rank: 1, category: 'release', intro: 'Alpha-Modell veröffentlicht', body: 'Details zum Alpha-Modell', author_name: 'Lab Alpha', author_handle: '@alpha', hype_score: 3, discussion_count: 2, bookmarked: false, sources: [], payload: {}, created_at_ms: ts, updated_at_ms: ts },
    ]);
    await kernel.sim.settle();
    const api = kernel.appApi(APP, '1');
    window.__k = kernel;
    window.__api = api;
    window.__zone = () => kernel.statusZone.forApp(APP).get().filter((e) => e.source === 'app');
    window.__rows = (c) => api.query(c, {}).get();
    window.__commands = () => api.query('business_commands', { sort: [{ created_at: 'asc' }] }).get();
  }, APP);
});

after(async () => { await browser?.close(); await new Promise((r) => server?.close(r)); });

const settle = async () => { await page.evaluate(() => window.__k.sim.settle()); await page.waitForTimeout(250); };
const topicText = () => page.$eval('[data-topic-list]', (el) => el.textContent);
const feedText = () => page.$eval('[data-feed]', (el) => el.textContent);
const SUBS = ['news_topics', 'news_sources', 'news_issues', 'news_posts'];

test('mount: rendert Live-Daten, nur api.*, gescoptes Root, Titel über api.shell', async () => {
  await page.evaluate(async () => {
    const { countingApi } = await import('/v4/tools/port/test-api.mjs');
    window.__c = countingApi(window.__api);
    window.__app = await window.__mod.mount(document.getElementById('win'), window.__c.api, {});
  });
  await settle();
  assert.match(await topicText(), /KI-Labs/);
  assert.match(await feedText(), /Alpha-Modell veröffentlicht/);
  assert.equal(await page.$eval('#win', (el) => el.dataset.app), 'news');
  assert.equal(await page.evaluate(() => window.__k.shell.window('news').state.get().title), 'News');
  assert.deepEqual(await page.evaluate((subs) => subs.map((c) => window.__c.subscribers(c)), SUBS), [1, 1, 1, 1]);
  assert.deepEqual(page.__errors, []);
});

test('Live-Änderung vom Core (sim.push) erscheint ohne Neuladen, Löschung verschwindet', async () => {
  const ts = Date.now();
  await page.evaluate((ts) => window.__k.sim.push('news_posts', { id: 'issue_1_p02', issue_id: 'issue_1', topic_id: 'topic_ai', rank: 2, category: 'release', intro: 'Beta Live-Beitrag', author_name: 'Lab Beta', payload: {}, created_at_ms: ts, updated_at_ms: ts }), ts);
  await settle();
  assert.match(await feedText(), /Beta Live-Beitrag/);
  await page.evaluate(() => window.__k.sim.push('news_posts', { id: 'issue_1_p02', _deleted: true }));
  await settle();
  assert.doesNotMatch(await feedText(), /Beta Live-Beitrag/);
});

test('leere Collections rendern keinen Zustandstext und keine Zone', async () => {
  const res = await page.evaluate(async () => {
    const { createStubKernel } = await import('/__stub/ctox-app-api-stub.mjs');
    const k2 = createStubKernel({ storage: 'memory', latencyMs: 5, multiTab: false, instanceId: 'empty' });
    await k2.ready;
    const host = document.createElement('div');
    host.style.cssText = 'width:900px;height:600px';
    document.body.append(host);
    const app = await window.__mod.mount(host, k2.appApi('news', '1'), {});
    await new Promise((r) => setTimeout(r, 300));
    const texts = ['[data-topic-list]', '[data-feed]', '[data-feed-title]'].map((sel) => host.querySelector(sel)?.textContent.trim() ?? null);
    const hasStateBox = Boolean(host.querySelector('.ctox-syncing, .ctox-empty, [role="status"]'));
    const zone = k2.statusZone.forApp('news').get().map((e) => e.code);
    await app.unmount();
    host.remove();
    await k2.close();
    return { texts, hasStateBox, zone };
  });
  assert.deepEqual(res.texts, ['', '', '']);
  assert.equal(res.hasStateBox, false);
  assert.deepEqual(res.zone, []);
  // Modulzustand ist modulweit (V4-PORT.md OFFEN): Hauptfenster neu montieren.
  await page.evaluate(async () => {
    await window.__app.unmount();
    window.__app = await window.__mod.mount(document.getElementById('win'), window.__c.api, {});
  });
  await settle();
});

test('Befehl: Ausgabe erzeugen → api.command ohne Warten; failed → genau ein Zoneneintrag + Ausgabe failed', async () => {
  await page.evaluate(() => window.__k.sim.failCommand('news.issue.generate', 'Queue nimmt nicht an'));
  const before = await page.evaluate(() => window.__rows('news_issues').length);
  await page.click('[data-action="generate-issue"]');
  await settle();
  await settle();
  const cmd = await page.evaluate(() => window.__commands().at(-1));
  assert.equal(cmd.type, 'news.issue.generate');
  assert.equal(cmd.payload.topic_id, 'topic_ai');
  assert.equal(cmd.payload.record_id, 'topic_ai');
  assert.equal(cmd.status, 'failed');
  const issues = await page.evaluate(() => window.__rows('news_issues'));
  assert.equal(issues.length, before + 1);
  const created = issues.find((i) => i.id === cmd.payload.issue_id);
  assert.equal(created.status, 'failed');
  assert.equal(created.command_id, cmd.id);
  const zone = await page.evaluate(() => window.__zone());
  assert.equal(zone.filter((e) => e.code === 'news.command.news.issue.generate').length, 1);
  assert.match(zone.find((e) => e.code === 'news.command.news.issue.generate').text, /Queue nimmt nicht an/);
  // Feed zeigt keinen Fehlerkasten; der Fehler der Ausgabe steht in der Zone.
  assert.equal(await page.evaluate(() => Boolean(document.querySelector('#win .ctox-empty'))), false);
  assert.ok(zone.some((e) => e.code === 'news.issue.failed'));
  // Zweiter Lauf ohne Fehler: done räumt den Befehlseintrag
  await page.click('[data-action="generate-issue"]');
  await settle();
  await settle();
  const last = await page.evaluate(() => window.__commands().at(-1));
  assert.equal(last.status, 'done');
  assert.equal((await page.evaluate(() => window.__zone())).filter((e) => e.code === 'news.command.news.issue.generate').length, 0);
});

test('Schreiben: Merken per api.patch, neues Thema über den Drawer per api.put', async () => {
  await page.evaluate(() => window.__k.statusZone.clear('news', 'news.issue.running'));
  await page.evaluate(() => {
    const sel = '[data-issue-id="issue_1"]';
    document.querySelector(sel)?.click();
  });
  await settle();
  await page.evaluate(() => document.querySelector('[data-action="bookmark-post"][data-post-id="issue_1_p01"]')?.click());
  await settle();
  const post = await page.evaluate(() => window.__rows('news_posts').find((p) => p.id === 'issue_1_p01'));
  assert.equal(post.bookmarked, true);
  await page.click('[data-action="create-topic"]');
  await settle();
  await page.fill('[data-topic-form] [name="title"]', 'Robotik');
  await page.evaluate(() => document.querySelector('[data-action="save-topic"]').click());
  await settle();
  const topics = await page.evaluate(() => window.__rows('news_topics'));
  assert.ok(topics.some((tp) => tp.title === 'Robotik'));
  assert.match(await topicText(), /Robotik/);
});

test('snapshot/restored: offener Themen-Drawer mit Eingabe und Auswahl überleben den Remount', async () => {
  await page.evaluate(() => document.querySelector('[data-topic-id="topic_ai"]')?.click());
  await settle();
  await page.click('[data-action="configure-topic"]');
  await settle();
  await page.fill('[data-topic-form] [name="prompt"]', 'halb fertige Frage');
  const snap = await page.evaluate(async () => {
    const cloned = structuredClone(window.__app.snapshot());
    await window.__app.unmount();
    window.__app = await window.__mod.mount(document.getElementById('win'), window.__c.api, { restored: cloned });
    return cloned;
  });
  assert.equal(snap.selectedTopicId, 'topic_ai');
  assert.equal(snap.drawer.topicId, 'topic_ai');
  assert.equal(snap.drawer.values.prompt, 'halb fertige Frage');
  assert.ok(JSON.stringify(snap).length < 1024 * 1024);
  await settle();
  const after = await page.evaluate(() => ({
    prompt: document.querySelector('[data-topic-form] [name="prompt"]')?.value,
    open: !document.querySelector('[data-topic-drawer]').hidden,
    selected: window.__app.snapshot().selectedTopicId,
  }));
  assert.equal(after.open, true);
  assert.equal(after.prompt, 'halb fertige Frage');
  assert.equal(after.selected, 'topic_ai');
  assert.deepEqual(await page.evaluate((subs) => subs.map((c) => window.__c.subscribers(c)), SUBS), [1, 1, 1, 1], 'alte Abos sind abgemeldet');
  await page.evaluate(() => document.querySelector('[data-action="close-drawer"]')?.click());
  await settle();
  assert.deepEqual(page.__errors, []);
});

test('FIX9a: laufende Ausgabe zeigt die letzte fertige (Status nur Zone + Punkt); leere Kategoriereiter und leere Trend-Spalte ausgeblendet', async () => {
  const ts = Date.now();
  await page.evaluate((ts) => window.__k.sim.push('news_issues', { id: 'issue_run', topic_id: 'topic_ai', title: 'KI-Labs', issue_date: '2099-01-01', status: 'running', command_id: 'cmd-run', task_id: '', post_count: 0, generated_at_ms: 0, error: '', payload: {}, created_at_ms: ts + 5000, updated_at_ms: ts + 5000 }), ts);
  await settle();
  await page.evaluate(() => document.querySelector('[data-issue-id="issue_run"]').click());
  await settle();
  assert.match(await feedText(), /Alpha-Modell veröffentlicht/, 'Feed zeigt die letzte fertige Ausgabe');
  assert.doesNotMatch(await page.$eval('#win', (el) => el.innerText), /wird erstellt|liest gerade/);
  assert.ok((await page.evaluate(() => window.__zone())).some((e) => e.code === 'news.issue.running'), 'Status in der Zone');
  assert.ok(await page.$('[data-issue-id="issue_run"] .news-dot.is-running'), 'dezenter Punkt am Ausgabeknoten');
  const right = await page.$eval('.news-right', (el) => ({ shown: getComputedStyle(el).display !== 'none', text: el.innerText }));
  assert.equal(right.shown, true);
  assert.match(right.text, /Alpha-Modell/, 'Trends der angezeigten Ausgabe');
  const hiddenTabs = await page.$$eval('.news-center [data-pg-band]', (tabs) => tabs.filter((t) => t.hidden || getComputedStyle(t).display === 'none').map((t) => t.dataset.pgBand));
  assert.deepEqual(hiddenTabs, ['model', 'tool', 'research', 'debate'], 'Reiter mit (0) ausgeblendet, „Alle“ bleibt');

  // Ausgabe ohne Beiträge und ohne Trends: rechte Spalte weg, der Feed nimmt die Breite.
  await page.evaluate((ts) => window.__k.sim.push('news_issues', { id: 'issue_run', status: 'ready', topic_id: 'topic_ai', title: 'KI-Labs', issue_date: '2099-01-01', post_count: 0, payload: {}, created_at_ms: ts + 5000, updated_at_ms: ts + 6000 }), ts);
  await settle();
  const geo = await page.evaluate(() => {
    const right = document.querySelector('.news-right');
    const center = document.querySelector('.news-center').getBoundingClientRect();
    const mod = document.querySelector('.news-module').getBoundingClientRect();
    return { rightShown: getComputedStyle(right).display !== 'none', gap: mod.right - center.right };
  });
  assert.equal(geo.rightShown, false, 'leere Trend-Spalte ausgeblendet');
  assert.ok(geo.gap < 40, `Feed reicht bis an den Rand (${geo.gap})`);
  await page.evaluate(() => window.__k.sim.push('news_issues', { id: 'issue_run', _deleted: true }));
  await settle();
  await page.evaluate(() => document.querySelector('[data-issue-id="issue_1"]')?.click());
  await settle();
  assert.deepEqual(page.__errors, []);
});

test('Screenshots 1280 und 420', { skip: !process.env.NEWS_SHOTS && !process.env.V4_SHOTS }, async () => {
  const shots = join(HERE, 'shots');
  await page.evaluate(() => { const d = document.querySelector('[data-topic-drawer]'); if (d) d.hidden = true; const s = document.querySelector('[data-drawer-scrim]'); if (s) s.hidden = true; });
  await page.evaluate(() => document.querySelector('[data-issue-id="issue_1"]')?.click());
  await page.waitForTimeout(200);
  await page.screenshot({ path: join(shots, `${APP}-1280.png`) });
  await page.evaluate(() => { const w = document.getElementById('win'); w.style.width = '420px'; });
  await page.setViewportSize({ width: 420, height: 760 });
  await page.waitForTimeout(300);
  await page.screenshot({ path: join(shots, `${APP}-420.png`) });
  await page.setViewportSize({ width: 1280, height: 760 });
  await page.evaluate(() => { const w = document.getElementById('win'); w.style.width = '1280px'; });
});

test('unmount meldet alle Abos ab und leert root', async () => {
  const res = await page.evaluate(async (subs) => {
    await window.__app.unmount();
    await window.__k.sim.push('news_topics', { id: 'late', title: 'Late', cadence: 'manual', enabled: true, payload: {}, created_at_ms: 1, updated_at_ms: 1 });
    await window.__k.sim.settle();
    return { subs: subs.map((c) => window.__c.subscribers(c)), children: document.getElementById('win').children.length };
  }, SUBS);
  assert.deepEqual(res.subs, [0, 0, 0, 0]);
  assert.equal(res.children, 0);
  assert.deepEqual(page.__errors, []);
});
