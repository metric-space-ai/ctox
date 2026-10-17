// research gegen den verbindlichen App-API-4.0-Stub des Kerns (createStubKernel, Pin 67229d2, read-only unter
// /__stub/) in echtem Chromium. Muster: modules/knowledge/tests/v4-api.test.mjs.
// Lauf: CTOX_APP_API_STUB=~/.local/state/shell-v4/stub-pins/64acc2c \
//       node --test src/apps/business-os/modules/research/tests/v4-api.test.mjs
import { test, before, after } from 'node:test';
import assert from 'node:assert/strict';
import { chromium, startServer, seedKernel, pageHtml, DOMAIN, TASK } from './v4-fixture.mjs';

let server, browser, page;
const LIVE = ['research_tasks', 'research_runs', 'research_notes', 'business_commands', 'ctox_queue_tasks', 'documents', 'knowledge_tables'];

before(async () => {
  ({ server } = await startServer(pageHtml()).then((r) => { server = r.server; return r; }).then(async (r) => {
    browser = await chromium.launch();
    page = await browser.newPage();
    const errors = [];
    page.on('pageerror', (e) => errors.push(e.message));
    page.on('console', (m) => { if (m.type() === 'error') console.error('[page]', m.text()); });
    page.on('response', (res) => { if (res.status() >= 400) console.error('[page] HTTP', res.status(), res.url()); });
    page.__errors = errors;
    await page.goto(r.base);
    await seedKernel(page);
    return r;
  }));
});

after(async () => { await browser?.close(); await new Promise((r) => server?.close(r)); });

const settle = async () => { await page.evaluate(() => window.__k.sim.settle()); await page.waitForTimeout(250); };
const text = (sel) => page.$eval(sel, (el) => el.textContent);

test('mount: rendert Live-Daten, nur api.*, gescoptes Root, Titel über api.shell', async () => {
  await page.evaluate(async () => {
    const { countingApi } = await import('/v4/tools/port/test-api.mjs');
    window.__c = countingApi(window.__api);
    window.__app = await window.__mod.mount(document.getElementById('win'), window.__c.api, {});
  });
  await settle();
  assert.match(await text('.research-task-list'), /Drohnen-Lagerlasten/);
  assert.match(await text('.research-center .ctox-pane-title'), /Drohnen-Lagerlasten/);
  assert.match(await text('.research-right'), new RegExp(DOMAIN.replace('/', '\\/')));
  assert.equal(await page.$eval('#win', (el) => el.dataset.app), 'research');
  assert.equal(await page.evaluate(() => window.__k.shell.window('research').state.get().title), 'Web Research');
  for (const c of ['research_tasks', 'knowledge_tables', 'business_commands']) {
    assert.equal(await page.evaluate((name) => window.__c.subscribers(name), c), 1, c);
  }
  assert.deepEqual(await page.evaluate(() => window.__zone()), []);
  assert.deepEqual(page.__errors, []);
});

test('Live-Änderung vom Core (sim.push) erscheint ohne Neuladen, Löschung verschwindet', async () => {
  await page.evaluate(() => window.__k.sim.push('research_tasks', {
    id: 'research_live', title: 'Live-Aufgabe Gamma', status: 'ready', knowledge_domain: 'research/gamma', payload: {}, created_at_ms: 5, updated_at_ms: 5,
  }));
  await settle();
  assert.match(await text('.research-task-list'), /Live-Aufgabe Gamma/);
  await page.evaluate(() => window.__k.sim.push('research_tasks', { id: 'research_live', _deleted: true }));
  await settle();
  assert.doesNotMatch(await text('.research-task-list'), /Live-Aufgabe Gamma/);
  // Laufzustand kommt live aus research_runs (kein Wartetext, Zustand aus der Collection).
  await page.evaluate(() => window.__k.sim.push('research_runs', {
    id: 'run_live', task_id: 'research_drone_bearings', status: 'running', command_id: 'cmd_x', payload: {}, created_at_ms: 6, updated_at_ms: 6,
  }));
  await settle();
  assert.equal(await page.$eval('.research-run-state', (el) => el.className.includes('research-run-running')), true);
  await page.evaluate(() => window.__k.sim.push('research_runs', { id: 'run_live', _deleted: true }));
  await settle();
  assert.equal(await page.$('.research-run-state'), null);
});

test('leere Collections rendern keinen Zustandstext und keine Zone', async () => {
  const empty = await page.evaluate(async () => {
    const { createStubKernel } = await import('/__stub/ctox-app-api-stub.mjs');
    const k2 = createStubKernel({ storage: 'memory', latencyMs: 5, multiTab: false, instanceId: 'empty' });
    await k2.ready;
    // E48: auch der leere Kern braucht die veröffentlichte Version mit Deklarationen (window.__routes aus seedKernel).
    for (const o of window.__routes?._fixture_owners ?? []) if (!(await k2.sim.inspect('business_apps', o.app))?.data?.active_ver) await k2.sim.publishAppVersion(o.app, o.ver, o.publish);
    await k2.sim.publishAppVersion('research', '1.0.0', { title: 'Web Research', manifestExtra: window.__routes });
    const host = document.createElement('div');
    host.style.cssText = 'width:900px;height:500px';
    document.body.append(host);
    const app = await window.__mod.mount(host, k2.appApi('research', '1.0.0'), {});
    await new Promise((r) => setTimeout(r, 300));
    const out = {
      tasks: host.querySelector('.research-task-list').textContent.trim(),
      ranking: host.querySelector('.research-ranking-list')?.textContent.trim() ?? '', // FIX9b: Abschnitt fehlt ohne Daten
      stateBox: Boolean(host.querySelector('.ctox-syncing, .ctox-empty, .research-empty, [role="status"], [role="alert"], .research-status-line')),
      zone: k2.statusZone.forApp('research').get().map((e) => e.code),
    };
    await app.unmount();
    host.remove();
    await k2.close();
    return out;
  });
  assert.equal(empty.tasks, '');
  assert.equal(empty.ranking, '');
  assert.equal(empty.stateBox, false);
  assert.deepEqual(empty.zone, []);
  // Modulzustand ist modulweit (PORTING §4 Falle 1): Hauptfenster neu montieren.
  await page.evaluate(async () => {
    await window.__app.unmount();
    window.__app = await window.__mod.mount(document.getElementById('win'), window.__c.api, {});
  });
  await settle();
});

test('Schreiben: Methode & Scoring speichern → api.put auf research_tasks, vom Core angenommen', async () => {
  await page.evaluate(() => document.querySelector('.research-right [data-action="edit-task"]').click());
  await page.fill('[data-research-task-form] textarea[name="criteria"]', 'Nur Primärquellen');
  await page.evaluate(() => document.querySelector('[data-research-task-form] button[type="submit"]').click());
  await settle();
  const saved = await page.evaluate(() => window.__api.get('research_tasks', 'research_drone_bearings').get());
  assert.equal(saved.criteria, 'Nur Primärquellen');
  assert.equal(saved.title, 'Drohnen-Lagerlasten');
  assert.equal(await page.$('.research-task-dialog'), null, 'Dialog schließt nach lokalem Commit');
  assert.deepEqual(await page.evaluate(() => window.__zone()), [], 'angenommener Write meldet nichts');
  const result = await page.evaluate(async () => {
    const h = window.__api.put('research_tasks', { id: 'research_put', title: 'Put-Aufgabe Delta', status: 'ready', knowledge_domain: 'research/delta', payload: {}, created_at_ms: 9, updated_at_ms: 9 });
    await h.local;
    return h.result;
  });
  assert.equal(result.status, 'accepted');
  await settle();
  assert.match(await text('.research-task-list'), /Put-Aufgabe Delta/);
  await page.evaluate(() => window.__api.remove('research_tasks', 'research_put').result);
  await settle();
});

test('Befehl: Research starten → business_commands-Dokument, nie warten; failed → genau ein Zoneneintrag', async () => {
  await page.evaluate(() => window.__k.sim.failCommand('research.systematic.run', 'kaputt'));
  await page.evaluate(() => document.querySelector('.research-task-item[data-task-id="research_drone_bearings"]').click());
  await settle();
  const disabled = await page.$eval('[data-action="run-research"]', (el) => el.disabled);
  assert.equal(disabled, false, 'Lauf ist startbar (Knowledge Base lokal vorhanden)');
  const before = await page.evaluate(() => window.__commands().length);
  await page.evaluate(() => document.querySelector('[data-action="run-research"]').click());
  await settle();
  const cmds = await page.evaluate(() => window.__commands());
  assert.equal(cmds.length, before + 1);
  const cmd = cmds.at(-1);
  assert.equal(cmd.type, 'research.systematic.run');
  assert.equal(cmd.payload.record_id, 'research_drone_bearings');
  assert.equal(cmd.payload.module, 'research');
  assert.equal(cmd.payload.knowledge_domain, DOMAIN);
  assert.match(cmd.payload.research_command_id, /^cmd_/);
  assert.equal(cmd.payload.instruction, undefined, 'Anweisung füllt der Handler aus prompts/systematic-run.md');
  assert.equal(cmd.payload.research_mode, 'library+living_dashboard');
  assert.ok(cmd.payload.research_brief.task_title, 'research_brief trägt die Template-Werte');
  assert.equal(cmd.status, 'failed');
  const run = await page.evaluate((id) => window.__api.query('research_runs', { selector: { command_id: id } }).get()[0], cmd.id);
  assert.ok(run, 'Lauf-Dokument verweist auf die Befehls-ID aus api.command');
  assert.equal(run.research_command_id, cmd.payload.research_command_id);
  const zone = await page.evaluate(() => window.__zone());
  assert.equal(zone.length, 1);
  assert.equal(zone[0].code, 'research.command.research.systematic.run');
  assert.match(zone[0].text, /kaputt/);
  assert.equal(zone[0].ref, cmd.id);
  // Navigation in die CTOX-App über die Shell (kein location.hash/sessionStorage).
  const nav = await page.evaluate(() => window.__k.sim.navigations.at(-1));
  assert.equal(nav.to, 'ctox');
  assert.equal(nav.params.command_id, cmd.id);
  assert.equal(await page.evaluate(() => location.hash), '');
  assert.deepEqual(page.__errors, []);
});

test('snapshot/restored: offener Aufgaben-Dialog und Auswahl überleben den Remount', async () => {
  await page.evaluate(() => document.querySelector('.research-left [data-action="new-task"]').click());
  await page.fill('[data-research-task-form] input[name="title"]', 'halb fertige Aufgabe');
  await page.fill('[data-research-task-form] textarea[name="prompt"]', 'Entwurf des Auftrags');
  const snap = await page.evaluate(async () => {
    window.__app.snapshot();
    const cloned = structuredClone(window.__app.snapshot()); // Shell-Ablauf Entwurf §5
    await window.__app.unmount();
    window.__app = await window.__mod.mount(document.getElementById('win'), window.__c.api, { restored: cloned });
    return cloned;
  });
  assert.equal(snap.selectedTaskId, 'research_drone_bearings');
  assert.equal(snap.taskDialog.values.title, 'halb fertige Aufgabe');
  assert.ok(JSON.stringify(snap).length < 1024 * 1024);
  await settle();
  const after = await page.evaluate(() => ({
    title: document.querySelector('[data-research-task-form] input[name="title"]')?.value,
    prompt: document.querySelector('[data-research-task-form] textarea[name="prompt"]')?.value,
    selected: window.__app.snapshot().selectedTaskId,
  }));
  assert.equal(after.title, 'halb fertige Aufgabe');
  assert.equal(after.prompt, 'Entwurf des Auftrags');
  assert.equal(after.selected, 'research_drone_bearings');
  assert.equal(await page.evaluate(() => window.__c.subscribers('research_tasks')), 1, 'alte Abos sind abgemeldet');
  await page.evaluate(() => document.querySelector('.research-task-dialog [data-close]')?.click());
  assert.deepEqual(page.__errors, []);
});

test('Sprung in Knowledge: Tabelle öffnen → api.shell.navigate(knowledge, {id, domain})', async () => {
  await page.evaluate(() => document.querySelector('[data-action="tab"][data-tab="knowledge"]')?.click());
  await settle();
  const button = await page.$('[data-action="open-knowledge"]');
  assert.ok(button, 'Knowledge-Tabellen sichtbar');
  await button.click();
  const nav = await page.evaluate(() => window.__k.sim.navigations.at(-1));
  assert.equal(nav.to, 'knowledge');
  assert.equal(nav.params.domain, 'research/drone-bearings');
  assert.match(nav.params.id, /^table:research\/drone-bearings:/);
  assert.equal(await page.evaluate(() => location.hash), '');
});

test('Katalog-Tabelle: Zeilen live aus knowledge_table_rows, unvollständig → Zone, vollständig → still', async () => {
  const logicalId = 'table:research/drone-bearings:claims';
  await page.evaluate((id) => window.__k.sim.push('knowledge_tables', {
    id, domain: 'research/drone-bearings', table_key: 'claims', title: 'Claims', table_id: 'cat_claims',
    projection_version: 2, rows_source: 'rxdb.rows.fetch', row_count: 2, content_hash: 'h1', updated_at_ms: 3,
  }), logicalId);
  await settle();
  // Der fehlgeschlagene Befehl aus dem Befehlstest bleibt bewusst in der Zone; hier zählen die Tabellen-Codes.
  const tableZone = () => page.evaluate(() => window.__zone().filter((e) => e.code.startsWith('research.table.')));
  let zone = await tableZone();
  assert.deepEqual(zone.map((e) => e.code), [`research.table.rows.${logicalId}`]);
  assert.match(zone[0].text, /0 von 2 Zeilen/);
  assert.equal(await page.evaluate(() => window.__c.subscribers('knowledge_table_rows')), 1);
  await page.evaluate(() => window.__k.sim.push('knowledge_table_rows', [
    { id: 'cat_claims:0', table_id: 'cat_claims', row_index: 0, values: { claim_id: 'C1', claim_text: 'Axiallast steigt mit Schub', source_ids: 'src-a', claim_type: 'mechanism' } },
    { id: 'cat_claims:1', table_id: 'cat_claims', row_index: 1, values: { claim_id: 'C2', claim_text: 'Radiallast durch Unwucht', source_ids: 'src-b', claim_type: 'mechanism' } },
  ]));
  await settle();
  zone = await tableZone();
  assert.deepEqual(zone, [], 'vollständige Tabelle räumt ihren Zoneneintrag');
  assert.deepEqual(page.__errors, []);
});

test('unmount meldet alle Abos ab und leert root', async () => {
  const res = await page.evaluate(async (live) => {
    await window.__app.unmount();
    await window.__k.sim.push('research_tasks', { id: 'late', title: 'Late', status: 'ready', knowledge_domain: 'research/late', payload: {}, created_at_ms: 10, updated_at_ms: 10 });
    await window.__k.sim.settle();
    await new Promise((r) => setTimeout(r, 200));
    return {
      subs: [...live, 'knowledge_table_rows'].map((c) => window.__c.subscribers(c)),
      children: document.getElementById('win').children.length,
    };
  }, LIVE);
  assert.deepEqual(res.subs, new Array(LIVE.length + 1).fill(0));
  assert.equal(res.children, 0);
  assert.deepEqual(page.__errors, []);
});
