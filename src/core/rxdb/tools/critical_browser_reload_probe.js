'use strict';

// Real browser reloads against the already initialized native smoke host.
// This is the warm browser cohort, not native cold setup or a fresh profile.
const fs = require('node:fs');
const path = require('node:path');
const { performance } = require('node:perf_hooks');
const SAMPLE_COUNT = 30;
const LIMIT_P95_MS = 5000;

function summarize(samples, requiredCollections) {
  const issues = [];
  if (samples.length !== SAMPLE_COUNT) issues.push('expected exactly 30 completed reloads');
  if (!requiredCollections.length || new Set(requiredCollections).size !== requiredCollections.length) {
    issues.push('critical collection set is empty or duplicated');
  }
  for (const [index, sample] of samples.entries()) {
    if (sample.round !== index || !Number.isFinite(sample.reloadMs) || sample.reloadMs <= 0) {
      issues.push(`invalid timing or round at ${index}`);
    }
    const entries = sample.status?.sync?.initialSync?.entries || [];
    if (sample.status?.ok !== true || entries.length !== requiredCollections.length
      || requiredCollections.some(name => entries.filter(entry => entry.collection === name
        && entry.state === 'complete' && entry.streamingReady === true
        && entry.checkpointEpochAdvertised === true).length !== 1)) {
      issues.push(`reload ${index} lacks complete, live critical collections`);
    }
    if (sample.error) issues.push(`reload ${index} failed: ${sample.error}`);
  }
  const times = samples.map(sample => sample.reloadMs).filter(Number.isFinite).sort((a, b) => a - b);
  // Nearest rank; never interpolate away an observed slow tail.
  const p95Ms = times.length === SAMPLE_COUNT ? times[Math.ceil(times.length * .95) - 1] : null;
  if (p95Ms === null || p95Ms >= LIMIT_P95_MS) issues.push('warm browser reload p95 must be strictly below 5000 ms');
  return { schema: 'ctox.critical_browser_reload.v1', cohort: 'retained-browser-profile-initialized-native',
    expectedSampleCount: SAMPLE_COUNT, sampleCount: samples.length, requiredCollections,
    limitP95Ms: LIMIT_P95_MS, p95Ms, issues, ok: issues.length === 0, samples };
}

async function runCriticalBrowserReloads({ page, requiredCollections, waitForHealthyCompleteStatus,
  assertHealthyAdvancedStatusContract, readNativeLayout, outputPath }) {
  const samples = [];
  const url = page.url();
  const persist = () => {
    const report = summarize(samples, requiredCollections);
    fs.mkdirSync(path.dirname(outputPath), { recursive: true });
    fs.writeFileSync(outputPath, JSON.stringify(report, null, 2) + '\n');
    return report;
  };
  persist();
  for (let round = 0; round < SAMPLE_COUNT; round++) {
    const started = performance.now();
    let deadlineTimer;
    const sample = { round };
    try {
      await Promise.race([
        (async () => {
          await page.goto(url, { waitUntil: 'commit', timeout: 60000 });
          sample.navigationCommitMs = performance.now() - started;
          await page.waitForFunction(() => typeof globalThis.CTOX_BUSINESS_OS_STATUS?.waitForHealthy === 'function',
            null, { timeout: 60000 });
          sample.statusApiReadyMs = performance.now() - started;
          sample.status = await waitForHealthyCompleteStatus(page, {
            timeoutMs: Math.max(1, 60000 - (performance.now() - started)),
            requiredCollections, allowRestart: false,
          });
          sample.reloadMs = performance.now() - started;
          assertHealthyAdvancedStatusContract(sample.status);
        })(),
        new Promise((_, reject) => {
          deadlineTimer = setTimeout(() => reject(new Error('critical reload exceeded 60 seconds')), 60000);
        }),
      ]);
    } catch (error) {
      sample.reloadMs = performance.now() - started;
      sample.error = error.message;
      samples.push(sample);
      persist();
      throw error;
    } finally {
      clearTimeout(deadlineTimer);
    }
    samples.push(sample);
    persist();
    console.log(`critical_browser_reload_sample=${JSON.stringify({
      round, reloadMs: sample.reloadMs, navigationCommitMs: sample.navigationCommitMs,
      statusApiReadyMs: sample.statusApiReadyMs,
    })}`);
  }
  const report = persist();
  console.log(`critical_browser_reload_budget=${JSON.stringify({
    sampleCount: report.sampleCount, p95Ms: report.p95Ms, limitP95Ms: report.limitP95Ms, issues: report.issues,
  })}`);
  if (!report.ok) throw new Error(report.issues.join('; '));
  const pins = await require('./desktop_pin_reload_probe.js').runDesktopPinReload({
    page, readNativeLayout,
    outputPath: path.join(path.dirname(outputPath), 'desktop-pin-reload.json'),
  });
  return { mode: 'critical-browser-reload-timing', report, pins };
}

module.exports = { summarize, runCriticalBrowserReloads };
