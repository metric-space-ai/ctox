// SPDX-License-Identifier: MIT OR AGPL-3.0-only
import test from 'node:test';
import assert from 'node:assert/strict';
import { chooseVisibleTopbarApps } from './topbar-apps.js';
import { compactFreshnessPresentation, renderCollectionFreshnessWarning } from './collection-freshness.js';
import { SYNC_TRANSPORT } from './sync-contract.js';

test('whole app names fit exactly without an overflow button', () => {
  assert.deepEqual(chooseVisibleTopbarApps([60, 70], 136), [0, 1]);
});

test('a narrow row keeps the active app and exposes the remaining apps in overflow', () => {
  assert.deepEqual(chooseVisibleTopbarApps([72, 104, 64, 71], 196, { priority: 2 }), [0, 2]);
});

test('a name wider than the available row is offered through overflow without clipping', () => {
  assert.deepEqual(chooseVisibleTopbarApps([180], 100, { priority: 0 }), []);
});

test('growing the row restores all apps in their original order', () => {
  const widths = [72, 104, 64, 71];
  assert.deepEqual(chooseVisibleTopbarApps(widths, 196, { priority: 2 }), [0, 2]);
  assert.deepEqual(chooseVisibleTopbarApps(widths, 329, { priority: 2 }), [0, 1, 2, 3]);
});

function diagnostics(state, confirmedAt = 0, pullEnabled = true) {
  return { mode: SYNC_TRANSPORT, collections: { catalogue: { frameTransport: {
    collectionFreshnessState: state, lastSuccessfulPullAtMs: confirmedAt, pullEnabled,
  } } } };
}

test('sync stays a dot before 30 seconds, then shows the unconfirmed label', () => {
  const options = { collections: ['catalogue'], diagnostics: diagnostics('catching-up'), nowMs: 129_999 };
  assert.equal(compactFreshnessPresentation(options, 100_000).label, '');
  assert.equal(compactFreshnessPresentation({ ...options, nowMs: 130_000 }, 100_000).label, 'Stand unbestätigt');
  assert.equal(compactFreshnessPresentation({ ...options, nowMs: 130_000 }, 100_000).state, 'unconfirmed');
});

test('offline data remains explicitly unconfirmed rather than acquiring a healthy dot', () => {
  const value = compactFreshnessPresentation({ collections: ['catalogue'], diagnostics: diagnostics('offline-pending'), nowMs: 130_000 }, 100_000);
  assert.equal(value.state, 'offline');
  assert.equal(value.label, 'Stand unbestätigt');
  assert.match(value.title, /Offline/);
});

test('recent pull confirmation clears the warning, while an expired confirmation cannot mark data healthy', () => {
  assert.equal(compactFreshnessPresentation({ collections: ['catalogue'], diagnostics: diagnostics('live', 129_999), nowMs: 130_000 }).state, 'healthy');
  assert.equal(compactFreshnessPresentation({ collections: ['catalogue'], diagnostics: diagnostics('live', 1_000), nowMs: 130_000 }).state, 'syncing');
});

test('collections that deliberately do not pull never wait for a remote confirmation', () => {
  assert.equal(compactFreshnessPresentation({ collections: ['catalogue'], diagnostics: diagnostics('catching-up', 0, false), nowMs: 130_000 }).pending, false);
});

test('an empty view is idle and does not claim remote data was confirmed', () => {
  assert.equal(compactFreshnessPresentation().state, 'idle');
});

test('the compact label uses the selected language', () => {
  assert.equal(compactFreshnessPresentation({ collections: ['catalogue'], diagnostics: diagnostics('catching-up'), language: 'en', nowMs: 130_000 }, 100_000).label, 'Unconfirmed');
});

function warningFixture() {
  const label = { textContent: '' };
  let dispose;
  return {
    label, cleanup: () => dispose?.(),
    warning: {
      dataset: {}, hidden: false, title: '', setAttribute() {},
      querySelector: () => label,
      ownerDocument: { defaultView: { addEventListener: (_name, listener) => { dispose = listener; } } },
    },
  };
}

test('a quiet sync stream still changes the indicator at 30s and a new session starts a fresh interval', (t) => {
  t.mock.timers.enable({ apis: ['setTimeout', 'Date'], now: 100_000 });
  const fixture = warningFixture();
  const contextKey = {}, firstSession = {}, secondSession = {};
  const options = { collections: ['catalogue'], diagnostics: diagnostics('catching-up'), compact: true, contextKey, sessionKey: firstSession };
  renderCollectionFreshnessWarning(fixture.warning, options);
  t.mock.timers.tick(29_999);
  assert.equal(fixture.label.textContent, '');
  t.mock.timers.tick(1);
  assert.equal(fixture.label.textContent, 'Stand unbestätigt');
  renderCollectionFreshnessWarning(fixture.warning, { ...options, sessionKey: secondSession });
  assert.equal(fixture.label.textContent, '');
  fixture.cleanup();
  t.mock.timers.tick(30_001);
  assert.equal(fixture.label.textContent, '');
});

test('confirmation cancels a pending late warning', (t) => {
  t.mock.timers.enable({ apis: ['setTimeout', 'Date'], now: 100_000 });
  const fixture = warningFixture();
  const options = { collections: ['catalogue'], compact: true, contextKey: {}, sessionKey: {} };
  renderCollectionFreshnessWarning(fixture.warning, { ...options, diagnostics: diagnostics('catching-up') });
  t.mock.timers.tick(10_000);
  renderCollectionFreshnessWarning(fixture.warning, { ...options, diagnostics: diagnostics('live', 110_000) });
  t.mock.timers.tick(30_001);
  assert.equal(fixture.warning.dataset.syncState, 'healthy');
  assert.equal(fixture.label.textContent, '');
  fixture.cleanup();
});
