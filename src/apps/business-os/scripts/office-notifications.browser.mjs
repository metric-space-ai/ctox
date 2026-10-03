#!/usr/bin/env node
// Real Shell notifications + actual extracted Office error handlers, controlled lifecycle.
// This is not an Office SDK, transport, or persistence acceptance test.
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
for (const [app, handler] of Object.entries(handlers)) assert(handler, `${app} handler extraction must remain exact`);
const notifications = readFileSync(path.join(root, 'shared', 'notifications.js'));
const server = http.createServer((req, res) => {
  if (req.url === '/notifications.js') {
    res.writeHead(200, { 'content-type': 'text/javascript' }); res.end(notifications);
  } else {
    res.writeHead(200, { 'content-type': 'text/html' });
    res.end('<!doctype html><html><body><div id="notifications"></div></body></html>');
  }
});
await new Promise(resolve => server.listen(0, '127.0.0.1', resolve));
const report = { scope: 'Actual error handlers + actual Shell notification DOM; controlled editor lifecycle', cases: [] };
let browser;
try {
  browser = await chromium.launch({ headless: true });
  const page = await browser.newPage({ reducedMotion: 'reduce' });
  for (const app of ['documents', 'spreadsheets']) {
    for (const scenario of ['direct', 'nested', 'inactive']) {
      const result = { name: `${app}-${scenario}` };
      try {
        await page.goto(`http://127.0.0.1:${server.address().port}/`);
        const observed = await page.evaluate(async ({ app, scenario, body }) => {
          const { createNotifications } = await import('/notifications.js');
          const notifications = createNotifications({ container: document.querySelector('#notifications') });
          const shown = [];
          const nativeShow = notifications.show;
          notifications.show = options => { shown.push({ type: options.type, message: options.message, time: options.time }); return nativeShow(options); };
          const state = { ctx: { notifications }, t: (_key, fallback) => fallback,
            dirty: false, saving: true, needsFinalSave: false, superdocSaveTimer: 0 };
          const handle = { saving: true, activity: 0 };
          let timerClears = 0;
          const active = () => scenario !== 'inactive';
          const failure = { code: 'sync_timeout', message: 'CTOX product sync push timed out: spreadsheet_blob_chunks' };
          const payload = scenario === 'nested' ? { error: failure } : failure;
          if (app === 'spreadsheets') {
            const run = new Function('state', 'handle', 'isCurrent', 'clearSaveTimer', 'markSpreadsheetAsDirty', 'error',
              `let errorReported = false; ${body}; return errorReported;`);
            run(state, handle, active, () => { timerClears += 1; }, s => { s.dirty = true; }, payload);
          } else {
            new Function('state', 'handle', 'isActive', 'payload', body)(state, handle, active, payload);
          }
          return { shown, dirty: state.dirty, saving: handle.saving, needsFinalSave: state.needsFinalSave,
            activity: handle.activity, timerClears, html: document.querySelector('#notifications').innerHTML };
        }, { app, scenario, body: handlers[app] });
        if (scenario === 'inactive') {
          assert.equal(observed.shown.length, 0);
          assert.equal(observed.dirty, false);
          assert.equal(observed.saving, true);
        } else {
          assert.equal(observed.shown.length, 1, 'one visible Shell notification must be emitted');
          assert.equal(observed.shown[0].type, 'error');
          assert.match(observed.shown[0].message, /CTOX product sync push timed out/);
          assert.equal(observed.shown[0].time, 0, 'save failure must remain visible until dismissed');
          assert.equal(observed.dirty, true);
          assert.equal(observed.saving, false);
          assert.equal(await page.getByRole('alert').count(), 1);
          if (app === 'documents') assert.equal(observed.needsFinalSave, true);
          else { assert.equal(observed.activity, 1); assert.equal(observed.timerClears, 1); }
          await page.screenshot({ path: path.join(output, `${app}-${scenario}.png`) });
          await page.getByRole('button', { name: 'Schließen', exact: true }).click();
          await page.waitForFunction(() => document.querySelectorAll('[role="alert"]').length === 0, undefined, { timeout: 3000 });
        }
        result.status = 'passed';
      } catch (error) { result.status = 'failed'; result.error = error.message; }
      report.cases.push(result);
      console.log(`${result.status}: ${result.name}${result.error ? `: ${result.error}` : ''}`);
    }
  }
  for (const [app, source] of Object.entries(sources)) {
    const invalid = /notifications\?\.\s*(error|success)\?\./.test(source);
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
