'use strict';
const { test } = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const vm = require('node:vm');
const source = fs.readFileSync(require.resolve('./browser_rust_smoke.js'), 'utf8');
const anchor = source.indexOf('const appShellReloadStartedAt = Date.now();');
assert.ok(anchor >= 0);
const start = source.indexOf('await page.waitForFunction(', anchor) + 'await page.waitForFunction('.length;
const predicate = source.slice(start).match(/^([\s\S]*?), (?:null|smokeMode), \{ timeout: smokeHookWaitTimeoutMs \}\);/)?.[1];
assert.ok(predicate, 'the readiness predicate must be read from the actual reload boundary');
const state = { modules: [{ id: 'desktop' }], initialModuleOpened: false, bootTimings: { shellVisibleMs: null } };
const dataset = { moduleShell: 'full' };
const hook = { state, bootstrap: 'complete' };
const ready = vm.runInNewContext('(' + predicate + ')', {
  ctoxBusinessOsSmoke: hook, CTOX_BUSINESS_OS_STATUS: {}, document: { body: { dataset } },
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
  hook.bootstrap = 'inline';
  assert.equal(ready('business-os-app-audience-ui'), false, 'the inline hook is still a placeholder');
  hook.bootstrap = 'complete';
  state.initialModuleOpened = false;
  assert.equal(ready('business-os-ui-regression'), true, 'other modes retain their existing reload contract');
});
