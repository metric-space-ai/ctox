'use strict';
const { test } = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const vm = require('node:vm');
const source = fs.readFileSync(require.resolve('./browser_rust_smoke.js'), 'utf8');
const marker = source.indexOf('// The Audience fixture replaces the actor and grants.');
assert.ok(marker >= 0);
const start = source.lastIndexOf('await page.waitForFunction(', marker) + 'await page.waitForFunction('.length;
const end = source.indexOf(', smokeMode, { timeout: 60000 });', marker);
assert.ok(end > start);
const state = { modules: [{ id: 'desktop' }], initialModuleOpened: false, bootTimings: { shellVisibleMs: null } };
const dataset = { moduleShell: 'full' };
const ready = vm.runInNewContext('(' + source.slice(start, end) + ')', {
  ctoxBusinessOsSmoke: { state }, document: { body: { dataset } },
});

test('audience fixture waits for shell completion and the outstanding catalog notification', () => {
  assert.equal(ready('business-os-app-audience-ui'), false, 'mounted frame can precede bootstrap completion');
  state.initialModuleOpened = true;
  state.bootTimings.shellVisibleMs = 20;
  for (const field of ['catalogRefreshQueued', 'catalogRefreshTimer', 'catalogRefreshRunning']) {
    state[field] = true;
    assert.equal(ready('business-os-app-audience-ui'), false, field + ' still owns a projection');
    state[field] = false;
  }
  assert.equal(ready('business-os-app-audience-ui'), true);
  dataset.moduleLoading = 'desktop';
  assert.equal(ready('business-os-app-audience-ui'), false, 'an active mount is still pending');
  delete dataset.moduleLoading;
  state.initialModuleOpened = false;
  assert.equal(ready('business-os-ui-regression'), true, 'other modes retain their existing startup contract');
});
