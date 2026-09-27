'use strict';
const test = require('node:test');
const assert = require('node:assert/strict');
const { startupReadiness, startupDiagnostics } = require('./business_os_startup_readiness.js');
const version = 'business-os-advanced-status-v1';
function state() {
  return {
    activeModule: 'desktop', loading: false, shellVisible: true, moduleCount: 21,
    expectedTextFound: true, windows: [{ ownerId: 'desktop-app:ctox', visible: true }],
    advancedStatus: { version, ok: true, checks: {
      authenticated: true, shellLoaded: true, activeModuleLoaded: true,
      workspaceNotLoading: true, dataPlaneWebrtc: true, rxdbRuntimeAppLocal: true,
      moduleCatalogAvailable: true, requiredCollectionsConnected: true,
      requiredCollectionsInitialSyncComplete: true, requiredCollectionsStreamingReady: true,
      requiredCollectionsCheckpointEpochAdvertised: true, noCheckpointProtocolErrors: true,
      noSchemaProtocolErrors: true, noReplicationIoErrors: true, noFailedCollections: true,
      noStalledReconnect: true, frameTransportRealtimeHealthy: true, noAutomaticRepairRunning: true,
    }, shell: { bootTimings: { shellVisibleMs: 829 } } },
  };
}

test('current desktop window satisfies requested app without obsolete status text', () => {
  const result = startupReadiness(state(), 'ctox', version);
  assert.equal(result.shellVisible, true); assert.equal(result.moduleReady, true);
  assert.equal(result.connected, true); assert.equal(result.ready, true);
  assert.equal(result.bootTimingMs, 829);
});

test('early shell visibility cannot substitute for complete sync readiness', () => {
  for (const check of ['requiredCollectionsConnected', 'requiredCollectionsInitialSyncComplete',
    'requiredCollectionsStreamingReady', 'requiredCollectionsCheckpointEpochAdvertised', 'frameTransportRealtimeHealthy']) {
    const observed = state();
    observed.advancedStatus.checks[check] = false;
    observed.advancedStatus.ok = false;
    const result = startupReadiness(observed, 'ctox', version);
    assert.equal(result.shellVisible, true, check);
    assert.equal(result.ready, false, check);
    // Even an inconsistent aggregate cannot hide the individual failed check.
    observed.advancedStatus.ok = true;
    assert.equal(startupReadiness(observed, 'ctox', version).ready, false, check);
    delete observed.advancedStatus.checks[check];
    assert.equal(startupReadiness(observed, 'ctox', version).ready, false, check);
  }
});

test('wrong or hidden app, shell, status, timing and explicit expected text remain failures', () => {
  const variants = [
    s => { s.windows[0].ownerId = 'desktop-app:other'; },
    s => { s.windows[0].visible = false; },
    s => { s.windows = []; },
    s => { s.shellVisible = false; },
    s => { s.loading = true; },
    s => { s.moduleCount = 0; },
    s => { s.expectedTextFound = false; },
    s => { s.advancedStatus.version = 'unknown'; },
    s => { s.advancedStatus.ok = false; },
    s => { s.advancedStatus.shell.bootTimings.shellVisibleMs = null; },
    s => { s.advancedStatus.shell.bootTimings.shellVisibleMs = -1; },
    s => { delete s.advancedStatus; },
  ];
  for (const mutate of variants) {
    const observed = state(); mutate(observed);
    assert.equal(startupReadiness(observed, 'ctox', version).ready, false);
  }
  const legacy = state(); legacy.activeModule = 'ctox'; legacy.windows = [];
  assert.equal(startupReadiness(legacy, 'ctox', version).ready, true);
});

test('failure diagnostics preserve checks and timing without launch/session configuration', () => {
  const observed = state();
  observed.config = { capability_token: 'do-not-export', password: 'do-not-export' };
  observed.textSample = 'do-not-export';
  observed.advancedStatus.config = observed.config;
  const diagnostic = startupDiagnostics(observed);
  assert.equal(JSON.stringify(diagnostic).includes('do-not-export'), false);
  assert.deepEqual(diagnostic.advancedStatus.checks, observed.advancedStatus.checks);
  assert.equal(diagnostic.advancedStatus.bootTimings.shellVisibleMs, 829);
});
