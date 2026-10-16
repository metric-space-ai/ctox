// Abnahme Quelltext-Modus (Strang Q): node src/apps/business-os/v4/source/lab/accept.mjs [--editor auto|textarea]
// Playwright aus ~/Documents/ctox/src/apps/business-os/node_modules, Chromium headless-shell 1223.
// Stub: CTOX_APP_API_STUB (Default Pin ~/.local/state/shell-v4/stub-pins/68b83a3), Quelltext-API des Kerns ohne Attrappe.
// Reihenfolge: 1–5, 8 (getrennte Entwürfe), 9 (rebase/discard), 10 (Dateien), 11 (Manifest-Schutzfeld),
// 12 (umbenannter Handler über zwei Veröffentlichungen), 13 (Verweis auf gelöschte Datei), 6 (Upload).
// Ergebnis: results.json (+ shots/*.png). Ein Tab-Verbund (ein Browser-Kontext, eine Stub-DB = „ein Core“):
//   A = ada (admin, Leiter), B = ada (zweiter Tab), C = nina (user), D = bert (admin).
import { writeFile } from 'node:fs/promises';
import { fileURLToPath } from 'node:url';
import path from 'node:path';
import os from 'node:os';
import { startServer, STUB } from './server.mjs';

const HERE = path.dirname(fileURLToPath(import.meta.url));
const SHOTS = path.join(HERE, 'shots');
const { chromium } = await import(path.join(os.homedir(), 'Documents/ctox/src/apps/business-os/node_modules/playwright/index.mjs'));
const EXE = path.join(os.homedir(), 'Library/Caches/ms-playwright/chromium_headless_shell-1223/chrome-headless-shell-mac-arm64/chrome-headless-shell');
const EDITOR = process.argv.includes('--editor') ? process.argv[process.argv.indexOf('--editor') + 1] : 'auto';

const out = { startedAt: new Date().toISOString(), stub: STUB, editor: EDITOR, checks: [], errors: [] };
const check = (id, pass, data) => { out.checks.push({ id, pass: !!pass, data }); console.log(pass ? 'PASS' : 'FAIL', id, JSON.stringify(data).slice(0, 300)); };

const { server, url } = await startServer();
const browser = await chromium.launch({ executablePath: EXE, headless: true });
const ctx = await browser.newContext({ viewport: { width: 1280, height: 800 } });
async function tab(name, user, role) {
  const p = await ctx.newPage();
  p.on('pageerror', (e) => out.errors.push(`${name}: ${e.message}`));
  p.on('console', (m) => { if (m.type() === 'error') out.errors.push(`${name}: ${m.text()}`); });
  await p.goto(`${url}/?user=${user}&role=${role}&editor=${EDITOR}`);
  await p.waitForFunction(() => document.documentElement.dataset.labReady === '1', null, { timeout: 20000 });
  return p;
}
const info = (p, id) => p.evaluate((id) => window.__lab.winInfo(id), id);
const W = (id, s) => `[data-window-id="${id}"] ${s}`;
// Programmatisches Ersetzen eines ganzen Dateitexts (wie Einfügen über alles) über die Editor-Fläche des Quelltext-
// Modus; löst denselben onChange-Weg aus wie Tippen. Getippt wird in Abnahme 2 echt (Tastatur).
async function setText(p, id, text) {
  await p.evaluate(({ id, text }) => window.__lab.toggles.get(id).handle.editor.replaceAll(text), { id, text });
  await p.waitForTimeout(350);
  await p.evaluate((id) => window.__lab.toggles.get(id).handle.flush(), id);
}
const getText = (p, id) => p.evaluate((id) => window.__lab.toggles.get(id).handle.editor.getValue(), id);
async function openPath(p, id, path) {
  await p.click(W(id, `.v4-src__item[data-path="${path}"]`));
  await p.waitForFunction(({ id, path }) => {
    const el = document.querySelector(`[data-window-id="${id}"] .v4-src`);
    return el?.dataset.currentPath === path && el.querySelector('.v4-src__editor > .v4-src-ed');
  }, { id, path });
}
async function shot(p, name, id) {
  for (const w of [1280, 420]) {
    await p.setViewportSize({ width: w, height: 800 });
    await p.evaluate((id) => window.__lab.fit(id), id);
    await p.waitForTimeout(250);
    await p.screenshot({ path: path.join(SHOTS, `${name}-${w}.png`) });
  }
  await p.setViewportSize({ width: 1280, height: 800 });
  await p.evaluate((id) => window.__lab.fit(id), id);
}
// Playwright-waitForFunction wartet nicht auf Promises (ein Promise ist truthy) – asynchrone Bedingungen hier pollen.
async function until(p, fn, arg, timeout = 10000) {
  const end = Date.now() + timeout;
  for (;;) {
    if (await p.evaluate(fn, arg)) return;
    if (Date.now() > end) throw new Error(`until: nach ${timeout} ms nicht erfüllt: ${fn.toString().slice(0, 120)}`);
    await p.waitForTimeout(100);
  }
}
async function publish(p, id) {
  const before = await p.evaluate((id) => window.__lab.winInfo(id).source.base, id);
  await p.click(W(id, '[data-publish]'));
  await p.waitForFunction(({ id, before }) => {
    const i = window.__lab.winInfo(id);
    return (i.source && i.source.base !== before) || i.zone.some((z) => z.code.startsWith('source.'));
  }, { id, before }, { timeout: 20000 });
  return info(p, id);
}

try {
  const A = await tab('A', 'ada', 'admin');
  await A.waitForFunction(() => window.__lab.isLeader(), null, { timeout: 10000 });
  check('seed', (await A.evaluate(() => window.__lab.seed())) === 'activated', {});
  const B = await tab('B', 'ada', 'admin');
  const C = await tab('C', 'nina', 'user');
  const D = await tab('D', 'bert', 'admin');
  for (const p of [B, C, D]) await p.waitForFunction(() => window.__lab.apps().some((a) => a.id === 'demo' && a.activated_ver === '1'), null, { timeout: 15000 });
  const wA = await A.evaluate(() => window.__lab.openApp('demo'));
  const wB = await B.evaluate(() => window.__lab.openApp('demo'));
  const wC = await C.evaluate(() => window.__lab.openApp('demo'));
  const wD = await D.evaluate(() => window.__lab.openApp('demo'));

  // (1) Nicht-Admin sieht keinen Umschalter; Admin schon.
  const iC = await info(C, wC);
  const iA0 = await info(A, wA);
  const cBtn = await C.locator(W(wC, '[data-v4-source-toggle]')).count();
  check('1-nonadmin-no-toggle', !iC.toggleVisible && iA0.toggleVisible && !(await C.isVisible(W(wC, '[data-v4-source-toggle]'))), { nonAdmin: iC.toggleVisible, admin: iA0.toggleVisible, cButtonsInDom: cBtn });
  await shot(C, '1-nonadmin', wC);

  // (2) Admin öffnet Quelltext, ändert index.js per Tastatur, veröffentlicht → B montiert live.
  await A.click(W(wA, '[data-v4-source-toggle]'));
  await A.waitForFunction((id) => document.querySelector(`[data-window-id="${id}"] .v4-src`)?.dataset.currentPath === 'index.js' && document.querySelector(`[data-window-id="${id}"] .v4-src-ed`), wA, { timeout: 20000 });
  const editorKind = await A.getAttribute(W(wA, '.v4-src'), 'data-editor');
  await A.click(W(wA, editorKind === 'monaco' ? '.monaco-editor .view-lines' : '.v4-src-ed__text'));
  await A.keyboard.press('Meta+ArrowDown');
  await A.keyboard.type("\nlabel = 'Version zwei';");
  await A.waitForTimeout(350);
  await A.evaluate((id) => window.__lab.toggles.get(id).handle.flush(), wA);
  const draft2 = await A.evaluate(() => window.__lab.draftInfo('demo'));
  // §16.26: manifest.json der Basis (Version 1 enthält sie als Datei) ist im Entwurf keine Datei – nicht im Dateibaum,
  // nicht in der Dateiauswahl, nicht als „gelöscht“ im Zustand.
  const tree2 = await A.locator(W(wA, '.v4-src__item')).evaluateAll((els) => els.map((e) => e.dataset.path));
  const options2 = await A.locator(W(wA, '.v4-src__select option')).evaluateAll((els) => els.map((e) => e.value));
  const st2 = (await info(A, wA)).source;
  const noManifestFile2 = !tree2.includes('manifest.json') && !options2.includes('manifest.json') && !st2.paths.includes('manifest.json') && !st2.deleted.includes('manifest.json') && !st2.draft.includes('manifest.json');
  await shot(A, '2-editor', wA);
  const t0 = Date.now();
  const iA2 = await publish(A, wA);
  await B.waitForFunction((id) => window.__lab.winInfo(id).label === 'Version zwei', wB, { timeout: 15000 });
  const iB2 = await info(B, wB);
  check('2-publish-live-remount', iA2.source.base === iB2.ver && iB2.label === 'Version zwei' && iA2.sourceOpen && iA2.zoneHidden && noManifestFile2, {
    editorKind, draftFiles: draft2?.files ? Object.fromEntries(Object.entries(draft2.files).map(([k, v]) => [k, v === null ? null : 'text'])) : null, tree: tree2, noManifestFile: noManifestFile2, newVer: iA2.source.base, bVer: iB2.ver, bLabel: iB2.label, editorStillOpen: iA2.sourceOpen, ms: Date.now() - t0,
  });

  // (3) Collection um optionales Feld mit Default → „kompatibel“, keine Migration.
  await openPath(A, wA, 'collections.schema.json');
  const t0s = await getText(A, wA);
  let schema0;
  try { schema0 = JSON.parse(t0s); } catch (e) { throw new Error(`Schema-Text nicht lesbar: ${JSON.stringify(t0s)}`); }
  const schema3 = structuredClone(schema0);
  schema3.collections.items.properties.note = { type: 'string', default: '' };
  await setText(A, wA, `${JSON.stringify(schema3, null, 2)}\n`);
  await A.waitForSelector(W(wA, '.v4-src__coll[data-collection="items"][data-kind="compatible"]'));
  const r3 = await A.evaluate((id) => ({ s: window.__lab.winInfo(id).source, disabled: document.querySelector(`[data-window-id="${id}"] [data-publish]`).disabled, mig: !!document.querySelector(`[data-window-id="${id}"] [data-add-migration]`) }), wA);
  await shot(A, '3-compatible', wA);
  const iA3 = await publish(A, wA);
  const m3 = await A.evaluate((v) => window.__lab.sk.artifacts.manifest('demo', v), iA3.source.base);
  check('3-compatible-no-migration', r3.s.diffSource === 'kernel' && r3.s.deviations.length === 0 && !r3.disabled && !r3.mig && r3.s.blockers.length === 0 && iA3.source.base !== iA2.source.base && (m3.generations?.items ?? 1) === 1 && !!m3.collections.items.schema.properties.note, {
    diff: r3.s.diff, diffSource: r3.s.diffSource, deviations: r3.s.deviations, blockers: r3.s.blockers, publishDisabled: r3.disabled, newVer: iA3.source.base, generation: m3.generations?.items ?? 1,
  });

  // (4) Feldtyp ändern → „neue Generation“; ohne migrate_fn gesperrt (Shell und Core), mit migrate_fn geht es.
  await openPath(A, wA, 'collections.schema.json');
  const t4 = await getText(A, wA);
  let schema4;
  try { schema4 = JSON.parse(t4); } catch (e) { throw new Error(`Schema-Text nicht lesbar: ${JSON.stringify(t4)}`); }
  schema4.collections.items.properties.count = { type: 'string' };
  await setText(A, wA, `${JSON.stringify(schema4, null, 2)}\n`);
  await A.waitForSelector(W(wA, '.v4-src__coll[data-collection="items"][data-kind="generation"]'));
  const r4 = await A.evaluate((id) => ({ s: window.__lab.winInfo(id).source, disabled: document.querySelector(`[data-window-id="${id}"] [data-publish]`).disabled, reasons: document.querySelector(`[data-window-id="${id}"] .v4-src__coll[data-collection="items"] .v4-src__coll-reasons`)?.textContent }), wA);
  const raw4 = await A.evaluate((base) => window.__lab.publishRaw('demo', base), iA3.source.base);
  await shot(A, '4-generation-blocked', wA);
  await A.click(W(wA, '[data-add-migration="items"]'));
  await A.waitForSelector(W(wA, '[data-open-migration="items"]'));
  await A.click(W(wA, '[data-open-migration="items"]'));
  await A.waitForFunction((id) => document.querySelector(`[data-window-id="${id}"] .v4-src`)?.dataset.currentPath === 'migrations/items.js' && document.querySelector(`[data-window-id="${id}"] .v4-src-ed`), wA);
  const tmpl = await getText(A, wA);
  await setText(A, wA, tmpl.replace('return { ...doc };', 'return { ...doc, count: String(doc.count ?? \'\') };'));
  const enabled4 = await A.evaluate((id) => !document.querySelector(`[data-window-id="${id}"] [data-publish]`).disabled, wA);
  await openPath(A, wA, 'collections.schema.json');
  await shot(A, '4-generation-migration', wA);
  const iA4 = await publish(A, wA);
  const m4 = await A.evaluate((v) => window.__lab.sk.artifacts.manifest('demo', v), iA4.source.base);
  const g2 = await A.evaluate(() => window.__lab.stub.sim.inspect('items@g2', 'i1'));
  check('4-generation-requires-migration', r4.s.diffSource === 'kernel' && r4.s.deviations.length === 0 && r4.disabled && r4.s.blockers.some((b) => b.name === 'items') && raw4.status === 'rejected' && raw4.reason === 'invalid' && enabled4 && m4.generations?.items === 2 && g2?.base?.data?.count === '3', {
    reasons: r4.reasons, diffSource: r4.s.diffSource, deviations: r4.s.deviations, publishDisabledWithout: r4.disabled, coreWithout: raw4, enabledWith: enabled4, newVer: iA4.source.base, generation: m4.generations?.items, migratedDoc: g2?.base?.data ?? null,
    migrationEntry: JSON.parse(await A.evaluate((v) => window.__lab.sk.source('demo').read('collections.schema.json', v), iA4.source.base)).migrations,
  });

  // (5) Zwei Admins (ada in A, bert in D) auf derselben Basis → zweiter bekommt conflict, Entwurf bleibt.
  await D.click(W(wD, '[data-v4-source-toggle]'));
  await D.waitForFunction(({ id, ver }) => document.querySelector(`[data-window-id="${id}"] .v4-src`)?.dataset.baseVer === ver && document.querySelector(`[data-window-id="${id}"] .v4-src-ed`), { id: wD, ver: iA4.source.base }, { timeout: 20000 });
  await openPath(A, wA, 'index.js');
  await openPath(D, wD, 'index.js');
  const baseText = await getText(A, wA);
  await setText(A, wA, baseText.replace("label = 'Version zwei';", "label = 'Version drei (ada)';"));
  const bertText = baseText.replace("label = 'Version zwei';", "label = 'Version drei (bert)';");
  await setText(D, wD, bertText);
  const baseBoth = [await A.evaluate((id) => window.__lab.winInfo(id).source.base, wA), await D.evaluate((id) => window.__lab.winInfo(id).source.base, wD)];
  // (8) vorher: zwei Entwürfe je (App, Nutzer) nebeneinander
  const pre8 = {
    adaInfo: await A.evaluate(() => window.__lab.draftInfo('demo')), bertInfo: await D.evaluate(() => window.__lab.draftInfo('demo')),
    adaRow: await A.evaluate(() => window.__lab.draftRow('ada/demo')), bertRow: await A.evaluate(() => window.__lab.draftRow('bert/demo')),
  };
  const iA5 = await publish(A, wA);
  await D.waitForFunction((v) => window.__lab.apps().find((a) => a.id === 'demo').active_ver === v, iA5.source.base);
  const iD5 = await publish(D, wD);
  const draftD = await D.evaluate(() => window.__lab.draftInfo('demo'));
  const dText = await getText(D, wD);
  await D.waitForSelector(W(wD, '[data-zone-action]:not([hidden])'));
  const iD5b = await info(D, wD);
  await shot(D, '5-conflict', wD);
  check('5-second-admin-conflict', baseBoth[0] === baseBoth[1] && iA5.source.base !== baseBoth[0] && iD5.zone.some((z) => z.code === 'source.conflict') && !iD5.zoneHidden && iD5b.zoneAction === 'Auf aktuelle Version setzen' && draftD?.files?.['index.js'] === bertText && draftD?.base_app_ver === baseBoth[0] && dText === bertText && iD5.source.base === baseBoth[0], {
    sameBase: baseBoth, adaPublished: iA5.source.base, bertZone: iD5b.zone, zoneAction: iD5b.zoneAction, bertDraftKept: draftD?.files?.['index.js'] === bertText, bertEditorKept: dText === bertText,
  });

  // (8) Getrennte Entwürfe zweier Admins (A1): jeder sieht nur seinen; publish von ada löscht nur adas Entwurf.
  const post8 = { adaRow: await A.evaluate(() => window.__lab.draftRow('ada/demo')), bertRow: await A.evaluate(() => window.__lab.draftRow('bert/demo')) };
  const has = (d, t) => (d?.files?.['index.js'] ?? '').includes(t);
  check('8-separate-drafts', has(pre8.adaInfo, '(ada)') && !has(pre8.adaInfo, '(bert)') && has(pre8.bertInfo, '(bert)') && !has(pre8.bertInfo, '(ada)')
    && has(pre8.adaRow, '(ada)') && has(pre8.bertRow, '(bert)') && post8.adaRow === null && has(post8.bertRow, '(bert)'), {
    before: { ada: pre8.adaRow?.id, adaBase: pre8.adaInfo?.base_app_ver, bert: pre8.bertRow?.id, bertBase: pre8.bertInfo?.base_app_ver },
    afterAdaPublish: { adaRow: post8.adaRow, bertRow: post8.bertRow?.id ?? null, bertKeepsText: has(post8.bertRow, '(bert)') },
  });

  // (9) Nach stale_base: Aktion in der Zone = rebase (Änderungen bleiben); danach „Verwerfen“ = discard.
  await D.click(W(wD, '[data-zone-action]'));
  await D.waitForFunction(({ id, ver }) => window.__lab.winInfo(id).source?.base === ver && !window.__lab.winInfo(id).zone.some((z) => z.code === 'source.conflict'), { id: wD, ver: iA5.source.base }, { timeout: 15000 });
  await openPath(D, wD, 'index.js');
  const rebased = await D.evaluate(() => window.__lab.draftInfo('demo'));
  const rebasedText = await getText(D, wD);
  const iD9 = await info(D, wD);
  await shot(D, '9-rebase', wD);
  await D.click(W(wD, '[data-discard]'));
  await until(D, async () => (await window.__lab.draftInfo('demo')) === null, null, 15000);
  await D.waitForFunction((id) => document.querySelector(`[data-window-id="${id}"] [data-discard]`).hidden && window.__lab.toggles.get(id).handle.editor, wD);
  const discarded = await D.evaluate(() => window.__lab.draftInfo('demo'));
  const afterDiscardText = await getText(D, wD);
  const activeText = await D.evaluate((v) => window.__lab.sk.source('demo').read('index.js', v), iA5.source.base);
  check('9-rebase-and-discard', rebased?.base_app_ver === iA5.source.base && rebased?.files?.['index.js'] === bertText && rebasedText === bertText && !iD9.zone.length && iD9.source.base === iA5.source.base
    && discarded === null && afterDiscardText === activeText && afterDiscardText.includes('(ada)'), {
    rebasedBase: rebased?.base_app_ver, keptBertText: rebased?.files?.['index.js'] === bertText, zoneAfterRebase: iD9.zone, discardedInfo: discarded, editorAfterDiscard: afterDiscardText.includes('(ada)') ? 'aktive Version (ada)' : afterDiscardText.slice(0, 80),
  });

  // (10) Datei umbenennen (Konflikt → Zone, dann gültig) und löschen, im Dateibaum; veröffentlichen.
  async function renameVia(p, id, from, to) {
    await p.click(W(id, `[data-rename="${from}"]`));
    const inp = W(id, `[data-rename-input="${from}"]`);
    await p.fill(inp, to);
    await p.press(inp, 'Enter');
  }
  await renameVia(A, wA, 'index.css', 'handlers/recount.js');
  await A.waitForFunction((id) => window.__lab.winInfo(id).zone.some((z) => z.code === 'source.file'), wA);
  const zone10 = (await info(A, wA)).zone.find((z) => z.code === 'source.file');
  await renameVia(A, wA, 'handlers/recount.js', 'handlers/zaehlen.js');
  await A.waitForFunction((id) => window.__lab.winInfo(id).source.paths.includes('handlers/zaehlen.js') && !window.__lab.winInfo(id).zone.length, wA);
  await A.click(W(wA, '[data-remove="index.css"]'));
  await A.waitForFunction((id) => !window.__lab.winInfo(id).source.paths.includes('index.css'), wA);
  await A.evaluate((id) => window.__lab.toggles.get(id).handle.flush(), wA);
  const draft10 = await A.evaluate(() => window.__lab.draftInfo('demo'));
  const tree10 = await A.locator(W(wA, '.v4-src__item')).allTextContents();
  await shot(A, '10-files', wA);
  const iA10 = await publish(A, wA);
  const list10 = await A.evaluate((v) => window.__lab.sk.source('demo').list(v), iA10.source.base);
  const m10 = await A.evaluate((v) => window.__lab.sk.artifacts.manifest('demo', v), iA10.source.base);
  await B.waitForFunction(({ id, v }) => window.__lab.winInfo(id).ver === v, { id: wB, v: iA10.source.base }, { timeout: 15000 });
  const iB10 = await info(B, wB);
  check('10-remove-rename', !!zone10 && /existiert bereits/.test(zone10.text) && draft10?.files?.['index.css'] === null && draft10?.files?.['handlers/recount.js'] === null && typeof draft10?.files?.['handlers/zaehlen.js'] === 'string'
    && !list10.includes('index.css') && !list10.includes('handlers/recount.js') && list10.includes('handlers/zaehlen.js') && m10.handlers?.['demo.recount'] === 'handlers/zaehlen.js' && iB10.label === 'Version drei (ada)', {
    conflictZone: zone10?.text, draftFiles: draft10?.files ? Object.fromEntries(Object.entries(draft10.files).map(([k, v]) => [k, v === null ? null : 'text'])) : null, draftManifest: draft10?.manifest,
    tree: tree10, newVer: iA10.source.base, files: list10, handlers: m10.handlers, bVer: iB10.ver, bLabel: iB10.label,
  });

  // (11) Manifest über setManifest: Schutzfeld → Ablehnung in der Zone; gültige Felder → veröffentlicht.
  await A.click(W(wA, '.v4-src__item[data-path="::manifest"]'));
  await A.waitForSelector(W(wA, '[data-v4-source-manifest] textarea[name="extras"]'));
  await A.fill(W(wA, 'textarea[name="extras"]'), '{ "app_id": "anders" }');
  await A.waitForFunction((id) => window.__lab.winInfo(id).zone.some((z) => z.code === 'source.manifest'), wA, { timeout: 10000 });
  const zone11 = (await info(A, wA)).zone.find((z) => z.code === 'source.manifest');
  const info11 = await A.evaluate(() => window.__lab.draftInfo('demo'));
  // Umgezogener Handler aus (10) muss im Formular stehen (Basis-Manifest; manifest.json gibt es im Entwurf nicht mehr).
  const handlerField11 = await A.inputValue(W(wA, 'textarea[name="handlers"]'));
  const raw11 = await A.evaluate(() => window.__lab.setManifestRaw('demo', { generations: { items: 9 } }));
  await shot(A, '11-manifest-protected', wA);
  await A.fill(W(wA, 'textarea[name="extras"]'), '{ "triggers": [{ "type": "schedule", "every": "1d", "command": "demo.recount" }] }');
  await A.fill(W(wA, 'input[name="title"]'), 'Demo (neu)');
  await until(A, async (id) => { const d = await window.__lab.draftInfo('demo'); return d?.manifest?.title === 'Demo (neu)' && Array.isArray(d?.manifest?.triggers) && !window.__lab.winInfo(id).zone.length; }, wA);
  const info11b = await A.evaluate(() => window.__lab.draftInfo('demo'));
  const iA11 = await publish(A, wA);
  const m11 = await A.evaluate((v) => window.__lab.sk.artifacts.manifest('demo', v), iA11.source.base);
  await B.waitForFunction(() => window.__lab.apps().find((a) => a.id === 'demo')?.title === 'Demo (neu)', null, { timeout: 15000 });
  check('11-setmanifest-protected', !!zone11 && /app_id/.test(zone11.text) && !('app_id' in (info11?.manifest ?? {})) && raw11.ok === false && raw11.code === 'invalid'
    && !('manifest.json' in (info11b?.files ?? {})) && Array.isArray(m11.triggers) && m11.app_id === 'demo' && m11.handlers?.['demo.recount'] === 'handlers/zaehlen.js' && handlerField11 === 'demo.recount = handlers/zaehlen.js' && iA11.source.base !== iA10.source.base, {
    zone: zone11?.text, handlerField: handlerField11, draftManifestAfterReject: info11?.manifest, rawGenerations: raw11, draftManifest: info11b?.manifest, draftFiles: Object.keys(info11b?.files ?? {}),
    newVer: iA11.source.base, published: { app_id: m11.app_id, triggers: m11.triggers, handlers: m11.handlers }, titleInB: 'Demo (neu)',
  });

  // (12) Umbenannter Handler bleibt über zwei Veröffentlichungen verdrahtet (B1): Umbenennen im Dateibaum, der Kern zieht
  // handlers mit (keine Manifest-Festschreibung der Shell); danach ein Veröffentlichen ohne jeden Manifest-Eingriff.
  await renameVia(A, wA, 'handlers/zaehlen.js', 'handlers/neu.js');
  await A.waitForFunction((id) => window.__lab.winInfo(id).source.paths.includes('handlers/neu.js') && !window.__lab.winInfo(id).zone.length, wA);
  await A.evaluate((id) => window.__lab.toggles.get(id).handle.flush(), wA);
  const draft12 = await A.evaluate(() => window.__lab.draftInfo('demo'));
  await A.click(W(wA, '.v4-src__item[data-path="::manifest"]'));
  await A.waitForSelector(W(wA, '[data-v4-source-manifest] textarea[name="handlers"]'));
  const handlerField12 = await A.inputValue(W(wA, 'textarea[name="handlers"]'));
  const iA12 = await publish(A, wA);
  const m12 = await A.evaluate((v) => window.__lab.sk.artifacts.manifest('demo', v), iA12.source.base);
  await openPath(A, wA, 'index.js');
  await setText(A, wA, (await getText(A, wA)).replace("label = 'Version drei (ada)';", "label = 'Version vier';"));
  const draft12b = await A.evaluate(() => window.__lab.draftInfo('demo'));
  const iA12b = await publish(A, wA);
  const m12b = await A.evaluate((v) => window.__lab.sk.artifacts.manifest('demo', v), iA12b.source.base);
  const list12b = await A.evaluate((v) => window.__lab.sk.source('demo').list(v), iA12b.source.base);
  await B.waitForFunction(({ id, v }) => window.__lab.winInfo(id).ver === v && window.__lab.winInfo(id).label === 'Version vier', { id: wB, v: iA12b.source.base }, { timeout: 15000 });
  await A.click(W(wA, '.v4-src__item[data-path="::manifest"]'));
  await A.waitForSelector(W(wA, '[data-v4-source-manifest] textarea[name="handlers"]'));
  const handlerField12b = await A.inputValue(W(wA, 'textarea[name="handlers"]'));
  await shot(A, '12-handler-republish', wA);
  const H = 'demo.recount';
  check('12-renamed-handler-republish', draft12?.manifest?.handlers?.[H] === 'handlers/neu.js' && draft12?.files?.['handlers/zaehlen.js'] === null && handlerField12 === `${H} = handlers/neu.js`
    && m12.handlers?.[H] === 'handlers/neu.js' && !('handlers' in (draft12b?.manifest ?? {})) && m12b.handlers?.[H] === 'handlers/neu.js' && list12b.includes('handlers/neu.js')
    && !list12b.includes('handlers/zaehlen.js') && !list12b.includes('manifest.json') && handlerField12b === `${H} = handlers/neu.js` && !iA12b.zone.length, {
    draftManifestAfterRename: draft12?.manifest, handlerField: handlerField12, firstPublish: { ver: iA12.source.base, handlers: m12.handlers },
    secondDraftManifest: draft12b?.manifest ?? null, secondPublish: { ver: iA12b.source.base, handlers: m12b.handlers, files: list12b }, handlerFieldAfter: handlerField12b,
    triggers: { afterStep11: !!m11.triggers, first: m12.triggers ?? null, second: m12b.triggers ?? null }, bLabel: 'Version vier',
  });

  // (13) Verweis auf gelöschte Datei: Handler-Datei im Dateibaum löschen → publish → Kern lehnt invalid mit detail ab,
  // Zone zeigt es, Entwurf bleibt. Danach Handler im Formular entfernen (setManifest null entfernt den Basis-Handler) → geht.
  await A.click(W(wA, '[data-remove="handlers/neu.js"]'));
  await A.waitForFunction((id) => !window.__lab.winInfo(id).source.paths.includes('handlers/neu.js'), wA);
  await A.evaluate((id) => window.__lab.toggles.get(id).handle.flush(), wA);
  const active13 = await A.evaluate(() => window.__lab.apps().find((a) => a.id === 'demo').active_ver);
  const iA13 = await publish(A, wA);
  const zone13 = iA13.zone.find((z) => z.code === 'source.publish');
  const draft13 = await A.evaluate(() => window.__lab.draftInfo('demo'));
  const activeAfter13 = await A.evaluate(() => window.__lab.apps().find((a) => a.id === 'demo').active_ver);
  await shot(A, '13-dangling-ref', wA);
  await A.click(W(wA, '.v4-src__item[data-path="::manifest"]'));
  await A.waitForSelector(W(wA, '[data-v4-source-manifest] textarea[name="handlers"]'));
  await A.fill(W(wA, 'textarea[name="handlers"]'), '');
  await until(A, async () => { const d = await window.__lab.draftInfo('demo'); return d && d.manifest?.handlers && Object.keys(d.manifest.handlers).length === 0; }, null);
  const draft13b = await A.evaluate(() => window.__lab.draftInfo('demo'));
  await A.click(W(wA, '[data-publish]'));
  await A.waitForFunction(({ id, before }) => window.__lab.winInfo(id).source.base !== before, { id: wA, before: active13 }, { timeout: 20000 });
  const iA13b = await info(A, wA);
  const m13 = await A.evaluate((v) => window.__lab.sk.artifacts.manifest('demo', v), iA13b.source.base);
  check('13-dangling-ref-rejected', !!zone13 && /Verweis auf fehlende Datei/.test(zone13.text) && zone13.text.includes('handlers.demo.recount → handlers/neu.js') && !iA13.zoneHidden
    && draft13?.files?.['handlers/neu.js'] === null && draft13?.base_app_ver === iA12b.source.base && iA13.source.base === iA12b.source.base && activeAfter13 === active13
    && Object.keys(draft13b?.manifest?.handlers ?? { x: 1 }).length === 0 && iA13b.source.base !== active13 && Object.keys(m13.handlers ?? {}).length === 0 && !iA13b.zone.length, {
    zone: zone13?.text, zoneVisible: !iA13.zoneHidden, draftKept: draft13 ? { base: draft13.base_app_ver, removed: draft13.files?.['handlers/neu.js'] === null } : null, activeUnchanged: activeAfter13 === active13,
    fix: { draftHandlers: draft13b?.manifest?.handlers, newVer: iA13b.source.base, publishedHandlers: m13.handlers, zoneAfter: iA13b.zone },
  });

  // (6) App-Upload (Ordner) → neue App → erscheint in kernel.apps (alle Tabs).
  const uid = await A.evaluate(() => window.__lab.openUpload());
  await A.setInputFiles('[data-v4-source-upload] input[type=file]', path.join(HERE, 'apps/zettel'));
  await A.waitForFunction(() => document.querySelector('[data-app-id]')?.value === 'zettel');
  const listed = await A.locator('[data-v4-source-upload] [data-files] li').allTextContents();
  await shot(A, '6-upload', uid);
  await A.click('[data-v4-source-upload] [data-publish]');
  await B.waitForFunction(() => window.__lab.apps().some((a) => a.id === 'zettel' && a.state === 'activated'), null, { timeout: 15000 });
  const appsB = await B.evaluate(() => window.__lab.apps());
  const wZ = await B.evaluate(() => window.__lab.openApp('zettel'));
  const zText = await B.evaluate((id) => window.__lab.win(id).content.textContent, wZ);
  const zApp = appsB.find((a) => a.id === 'zettel');
  const zFiles = zApp ? await B.evaluate((v) => window.__lab.sk.source('zettel').list(v), zApp.active_ver) : [];
  // manifest.json des Ordners → setManifest (Titel aus dem Manifest), keine Datei in der veröffentlichten Version
  check('6-upload-new-app', !!zApp && zApp.title === 'Zettel' && !zFiles.includes('manifest.json') && zText.includes('Zettel-App montiert'), { files: listed, published: zFiles, appsInB: appsB, mounted: zText.trim() });
} catch (e) {
  out.errors.push(`abort: ${e.stack || e.message}`);
  console.error(e);
} finally {
  out.pass = out.checks.length === 13 && out.checks.every((c) => c.pass);
  out.finishedAt = new Date().toISOString();
  await writeFile(path.join(HERE, `results${EDITOR === 'auto' ? '' : `-${EDITOR}`}.json`), JSON.stringify(out, null, 2));
  await browser.close();
  server.close();
  console.log(out.pass ? 'ALLE BESTANDEN' : 'NICHT BESTANDEN', out.errors.slice(0, 5));
}
