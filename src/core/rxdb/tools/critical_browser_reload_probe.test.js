'use strict';
const { test } = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const os = require('node:os');
const path = require('node:path');
const { summarize, runCriticalBrowserReloads } = require('./critical_browser_reload_probe.js');

const required = ['catalog', 'commands'];
const healthy = () => ({ ok: true, sync: { initialSync: {
  entries: required.map(collection => ({
    collection, state: 'complete', streamingReady: true, checkpointEpochAdvertised: true,
  })),
} } });
const samples = () => Array.from({ length: 30 }, (_, round) => ({
  round, reloadMs: 100 + round, status: healthy(),
}));

test('strict p95 uses the observed tail, retaining all 30 samples', () => {
  const rows = samples();
  rows[28].reloadMs = 5000;
  rows[29].reloadMs = 9000;
  const report = summarize(rows, required);
  assert.equal(report.p95Ms, 5000);
  assert.equal(report.ok, false);
  assert.equal(report.samples.length, 30);
  rows[28].reloadMs = 4999;
  assert.equal(summarize(rows, required).ok, true);
});

test('partial, duplicate-round and invalid measurements cannot pass', () => {
  assert.equal(summarize(samples().slice(1), required).ok, false);
  for (const invalid of [NaN, Infinity, -1, 0]) {
    const rows = samples();
    rows[5].reloadMs = invalid;
    assert.equal(summarize(rows, required).ok, false);
  }
  const rows = samples();
  rows[5].round = 4;
  assert.equal(summarize(rows, required).ok, false);
});

test('healthy flag cannot mask missing, duplicated or non-live collections', () => {
  for (const change of [
    status => { status.sync.initialSync.entries.pop(); },
    status => { status.sync.initialSync.entries[1].collection = 'catalog'; },
    status => { status.sync.initialSync.entries[0].state = 'pending'; },
    status => { status.sync.initialSync.entries[0].streamingReady = false; },
    status => { status.sync.initialSync.entries[0].checkpointEpochAdvertised = false; },
    status => { status.ok = false; },
  ]) {
    const rows = samples();
    change(rows[0].status);
    assert.equal(summarize(rows, required).ok, false);
  }
  assert.equal(summarize(samples(), []).ok, false);
});

test('runner preserves completed evidence and failure without restarting collections', async t => {
  const dir = fs.mkdtempSync(path.join(os.tmpdir(), 'critical-reload-test-'));
  t.after(() => fs.rmSync(dir, { recursive: true, force: true }));
  t.mock.method(console, 'log', () => {});
  let navigations = 0;
  let statuses = 0;
  const outputPath = path.join(dir, 'proof.json');
  const page = {
    url: () => 'http://127.0.0.1:8880/index.html',
    async goto(url, options) {
      assert.equal(url, this.url());
      assert.equal(options.waitUntil, 'commit');
      if (++navigations === 3) throw new Error('navigation lost');
    },
    async waitForFunction() {},
  };
  await assert.rejects(runCriticalBrowserReloads({
    page, requiredCollections: required, outputPath,
    async waitForHealthyCompleteStatus(target, options) {
      assert.equal(target, page);
      assert.equal(options.allowRestart, false);
      assert.deepEqual(options.requiredCollections, required);
      statuses++;
      return healthy();
    },
    assertHealthyAdvancedStatusContract(status) { assert.equal(status.ok, true); },
  }), /navigation lost/);
  const report = JSON.parse(fs.readFileSync(outputPath));
  assert.equal(report.ok, false);
  assert.equal(report.sampleCount, 3);
  assert.equal(report.p95Ms, null);
  assert.equal(statuses, 2);
  assert.equal(report.samples[2].error, 'navigation lost');
  assert.equal(report.samples[0].status.ok, true);
});
