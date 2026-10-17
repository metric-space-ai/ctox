#!/usr/bin/env node
// Real Shell v4 status zone + actual extracted Office error handlers, controlled lifecycle.
// This is not an Office SDK, transport, or persistence acceptance test.
//
// Shell v4 (Release-Reihe 0.2.x): v4-Gegenstück zu office-notifications.browser.mjs (V2, bleibt unverändert, 0.1.x).
// App-API 4.0 hat keine Toast-Benachrichtigungen mehr; Fehler gehen
// ausschließlich über api.status.report in die Zustandszone des Fensters (V4-PORT.md documents Nr. 8, §16.45).
// Die Regeln bleiben: genau eine sichtbare Fehlermeldung, sie bleibt bis zum Ausblenden stehen, der Editor wird
// wieder bearbeitbar und behält den ungespeicherten Stand; ein inaktiver Editor meldet nichts.
import assert from 'node:assert/strict';
import { readFileSync, mkdirSync, writeFileSync } from 'node:fs';
import http from 'node:http';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { chromium } from 'playwright';

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');
const outputIndex = process.argv.indexOf('--output-dir');
assert(outputIndex >= 0, 'Pass --output-dir on the disposable development volume');
const output = path.resolve(process.argv[outputIndex + 1]);
mkdirSync(output, { recursive: true });
const sources = Object.fromEntries(['documents', 'spreadsheets'].map(app => [app,
  readFileSync(path.join(root, 'modules', app, 'index.js'), 'utf8').replace(/\r\n/g, '\n')]));
const handlers = {
  spreadsheets: sources.spreadsheets.match(/  function onError\(error\) \{([\s\S]*?)\n  \}\n  state\.editorHandle = handle;/)?.[1],
  documents: sources.documents.match(/const removeErrorListener = editor\.on\('error', \(payload\) => \{([\s\S]*?)\n  \}\);/)?.[1],
};
const reporters = {
  documents: sources.documents.match(/\nfunction reportDocumentError\(state, code, error, prefix = ''\) \{[\s\S]*?\n\}\n/)?.[0],
};
for (const [app, handler] of Object.entries(handlers)) assert(handler, `${app} handler extraction must remain exact`);
assert(reporters.documents, 'documents reporter extraction must remain exact');
const CODES = { documents: 'documents.editor.office', spreadsheets: 'spreadsheets.editor.save' };
const zoneFiles = { '/status-zone.js': 'status-zone.js', '/sync-state.js': 'sync-state.js' };
const server = http.createServer((req, res) => {
  if (zoneFiles[req.url]) {
    res.writeHead(200, { 'content-type': 'text/javascript' });
    res.end(readFileSync(path.join(root, 'v4', 'shell', zoneFiles[req.url])));
  } else {
    res.writeHead(200, { 'content-type': 'text/html' });
    res.end('<!doctype html><html><body><section class="shell-window" data-window-app="office-qa"><div id="slot"></div></section></body></html>');
  }
});
await new Promise(resolve => server.listen(0, '127.0.0.1', resolve));
const report = { scope: 'Actual error handlers + actual Shell v4 status zone DOM; controlled editor lifecycle', cases: [] };
let browser;
try {
  browser = await chromium.launch({ headless: true });
  const page = await browser.newPage({ reducedMotion: 'reduce' });
  for (const app of ['documents', 'spreadsheets']) {
    for (const scenario of ['direct', 'nested', 'inactive']) {
      const result = { name: `${app}-${scenario}` };
      try {
        await page.goto(`http://127.0.0.1:${server.address().port}/`);
        const observed = await page.evaluate(async ({ app, scenario, body, reporter }) => {
          const { createStatusZone } = await import('/status-zone.js');
          // Kurze info-Frist: belegt, dass ein Fehler nicht wie info von selbst verschwindet.
          const zone = createStatusZone(document.querySelector('#slot'), document.querySelector('.shell-window'), { infoTtlMs: 50, ackScope: null });
          window.__zone = zone;
          const shown = [];
          const api = { status: { report: (item) => { shown.push({ ...item }); zone.report({ ...item, source: 'app' }); }, clear: (code) => zone.clear(code) } };
          const state = { api, t: (_key, fallback) => fallback, disposed: false,
            dirty: false, saving: true, needsFinalSave: false, superdocSaveTimer: 0 };
          const handle = { saving: true, activity: 0 };
          let timerClears = 0;
          const active = () => scenario !== 'inactive';
          // v4-Fehlerbild (api.files statt Chunk-Push); Wortlaut ohne Wächter-Vokabular, der Inhalt ist Testeingabe.
          const failure = { code: 'file_commit_rejected', message: 'CTOX file commit rejected: owned-original-blob' };
          const payload = scenario === 'nested' ? { error: failure } : failure;
          if (app === 'spreadsheets') {
            const run = new Function('state', 'handle', 'isCurrent', 'clearSaveTimer', 'markSpreadsheetAsDirty', 'error',
              `let errorReported = false; ${body}; return errorReported;`);
            run(state, handle, active, () => { timerClears += 1; }, s => { s.dirty = true; }, payload);
          } else {
            new Function('state', 'handle', 'isActive', 'payload', `${reporter}\n${body}`)(state, handle, active, payload);
          }
          await new Promise(resolve => setTimeout(resolve, 200));
          return { shown, dirty: state.dirty, saving: handle.saving, needsFinalSave: state.needsFinalSave,
            activity: handle.activity, timerClears, entries: zone.entries().map(({ level, code, text }) => ({ level, code, text })),
            zoneHidden: zone.el.hidden, badge: zone.badge.textContent };
        }, { app, scenario, body: handlers[app], reporter: reporters[app] || '' });
        if (scenario === 'inactive') {
          assert.equal(observed.shown.length, 0);
          assert.equal(observed.entries.length, 0);
          assert.equal(observed.zoneHidden, true, 'empty zone stays invisible');
          assert.equal(observed.dirty, false);
          assert.equal(observed.saving, true);
        } else {
          assert.equal(observed.shown.length, 1, 'exactly one status report must be emitted');
          assert.equal(observed.shown[0].level, 'error');
          assert.equal(observed.shown[0].code, CODES[app]);
          assert.match(observed.shown[0].text, /CTOX file commit rejected/);
          assert.deepEqual(observed.entries.map(e => e.code), [CODES[app]], 'save failure must remain visible until dismissed');
          assert.equal(observed.zoneHidden, false);
          assert.equal(observed.badge, '1');
          assert.equal(observed.dirty, true);
          assert.equal(observed.saving, false);
          if (app === 'documents') assert.equal(observed.needsFinalSave, true);
          else { assert.equal(observed.activity, 1); assert.equal(observed.timerClears, 1); }
          await page.locator('.v4-status-zone__badge').click();
          assert.equal(await page.getByRole('log').locator('.v4-status-zone__item').count(), 1);
          await page.screenshot({ path: path.join(output, `${app}-${scenario}.png`) });
          await page.getByRole('button', { name: 'Ausblenden', exact: true }).click();
          await page.waitForFunction(() => window.__zone.entries().length === 0 && window.__zone.el.hidden, undefined, { timeout: 3000 });
        }
        result.status = 'passed';
      } catch (error) { result.status = 'failed'; result.error = error.message; }
      report.cases.push(result);
      console.log(`${result.status}: ${result.name}${result.error ? `: ${result.error}` : ''}`);
    }
  }
  for (const [app, source] of Object.entries(sources)) {
    // v2-Benachrichtigungen dürfen nicht zurückkehren: kein ctx.notifications, keine optionalen Aufrufe.
    const invalid = /notifications\?\.\s*(error|success)\?\./.test(source) || /\bctx\.notifications\b/.test(source);
    report.cases.push({ name: `${app}-notification-contract-inventory`, status: invalid ? 'failed' : 'passed' });
  }
} finally {
  await browser?.close();
  await new Promise(resolve => server.close(resolve));
  writeFileSync(path.join(output, 'report.json'), JSON.stringify(report, null, 2));
}
const failed = report.cases.filter(result => result.status === 'failed').length;
console.log(`Office notification contract: ${report.cases.length - failed} passed, ${failed} failed`);
if (failed) process.exitCode = 1;
