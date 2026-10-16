// FIX9b (Shell v4): Web Research ohne Rohcodes, Fachjargon und Null-Kennzahlen; Leerzustand mit Aktion.
// Lauf: CTOX_APP_API_STUB=~/.local/state/shell-v4/stub-pins/8265bd7 \
//       node --import ./v4/tools/node/ctox-shared.mjs --test --test-force-exit modules/research/tests/fix9b.test.mjs
import { test, before, after } from 'node:test';
import assert from 'node:assert/strict';
import { chromium, startServer, seedKernel, pageHtml } from './v4-fixture.mjs';

let server, browser, page;
before(async () => {
  const r = await startServer(pageHtml());
  server = r.server;
  browser = await chromium.launch();
  page = await browser.newPage();
  page.__errors = [];
  page.on('pageerror', (e) => page.__errors.push(e.message));
  await page.goto(r.base);
  await seedKernel(page);
  await page.evaluate(async () => { window.__app = await window.__mod.mount(document.getElementById('win'), window.__api, {}); });
  await page.evaluate(() => window.__k.sim.settle());
  await page.waitForTimeout(300);
});
after(async () => { await browser?.close(); await new Promise((r) => server?.close(r)); });

const visibleText = () => page.evaluate(() => document.getElementById('win').innerText);

test('FIX9b: Kopf ohne „Dashboards“, englische Fachbegriffe und Bedienhinweis; keine Befehlskennungen', async () => {
  const txt = await visibleText();
  assert.match(await page.$eval('.research-left .ctox-pane-title', (el) => el.textContent), /^Recherchen$/);
  for (const bad of [/Dashboards/, /Evidence-Ranking/, /Not verified/, /LIVE/, /Semantic Research Graph/, /Insights/, /\bcmd_[0-9a-f]/i, /invalid_graph_contract/]) {
    assert.doesNotMatch(txt, bad, String(bad));
  }
  const titles = await page.$$eval('[title]', (els) => els.map((el) => el.getAttribute('title')).join(' | '));
  assert.doesNotMatch(titles, /Ziehen: drehen|Scrollen: zoomen/);
  assert.deepEqual(page.__errors, []);
});

test('FIX9b: Kennzahlen erst mit Daten – keine „0 Elemente“, kein „0 verifiziert“, keine „0 verifizierte Quellen“', async () => {
  const txt = await visibleText();
  assert.doesNotMatch(txt, /(^|\s)0 (Elemente|Verknüpfungen|Themenfelder|verifiziert|verifizierte Quellen|Quellen\b)/);
  const html = await page.evaluate(() => window.__mod.__researchTestHooks.renderSemanticGraph(null, { nodes: [], links: [], metrics: { nodeCount: 0 } }));
  assert.doesNotMatch(html, /data-graph-summary/);
  const withData = await page.evaluate(() => window.__mod.__researchTestHooks.renderSemanticGraph(null, { nodes: [{ id: 'a' }], links: [], metrics: { nodeCount: 3, linkCount: 2, clusterCount: 1 } }));
  assert.match(withData, /data-graph-summary[^>]*>3 Elemente · 2 Verknüpfungen · 1 Themenfeld/);
});

test('FIX9b: ungültige Graphdaten → verständliche Zonenmeldung, kein Rohcode im Inhalt; gültig → Zone geräumt', async () => {
  const html = await page.evaluate(() => window.__mod.__researchTestHooks.renderSemanticGraph(null, { status: 'invalid_graph_contract', errors: ['x'], nodes: [], links: [], metrics: {} }));
  const visible = html.replace(/<[^>]*>/g, ' ');
  assert.doesNotMatch(visible, /invalid_graph_contract/);
  await page.evaluate(() => window.__mod.__researchTestHooks.noteGraphContractStatus({ status: 'invalid_graph_contract' }, { id: 't1' }));
  await page.evaluate(() => window.__k.sim.settle());
  const zone = await page.evaluate(() => window.__zone());
  const entry = zone.find((z) => z.code === 'research.graph.contract');
  assert.ok(entry, JSON.stringify(zone));
  assert.match(entry.text, /Graphdaten .* unvollständig/);
  assert.doesNotMatch(entry.text, /invalid_graph_contract|_/);
  await page.evaluate(() => window.__mod.__researchTestHooks.noteGraphContractStatus({ status: 'ok' }, { id: 't1' }));
  await page.evaluate(() => window.__k.sim.settle());
  assert.equal((await page.evaluate(() => window.__zone())).filter((z) => z.code === 'research.graph.contract').length, 0);
});

test('FIX9b: keine Quellen → kurzer Leerzustand mit Aktion „Nachrecherche“; Suche ohne Treffer → nur ein Satz', async () => {
  const html = await page.evaluate(() => window.__mod.__researchTestHooks.renderSourcesWorkbench([]));
  assert.match(html, /data-research-empty="sources"/);
  assert.match(html, /Noch keine Quellen\./);
  assert.match(html, /data-action="graph-ai" data-graph-ai="research"[^>]*>Nachrecherche</);
  assert.doesNotMatch(html, /werden geladen|wird geladen/);
});
