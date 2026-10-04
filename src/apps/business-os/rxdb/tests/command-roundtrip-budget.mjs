// Acceptance limit for measured warm Browser -> WebRTC -> CTOX commands.
// This validates the supplied marks; the CI fixture owns their provenance.
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { resolve } from 'node:path';
import { pathToFileURL } from 'node:url';
import { buildRoundtripStageReport } from './command-roundtrip-stage-report.mjs';

const REQUIRED_MARKS = [
  'browser_dispatch_started', 'browser_local_inserted', 'browser_push_confirmed',
  'native_dispatch_entered', 'native_handler_completed',
  'native_rxdb_projection_committed', 'browser_terminal_observed',
];

export function assertWarmCommandBudget(samples) {
  assert.ok(Array.isArray(samples) && samples.length >= 30, 'at least 30 measured warm commands required');
  const ids = new Set();
  for (const sample of samples) {
    assert.ok(typeof sample?.command_id === 'string' && sample.command_id.length > 0, 'command ID required');
    assert.ok(!ids.has(sample.command_id), 'duplicate command measurement');
    ids.add(sample.command_id);
    for (const mark of REQUIRED_MARKS) {
      const value = sample.marks?.[mark];
      assert.ok(typeof value === 'number' && Number.isFinite(value) && value > 0,
        `missing or invalid measured mark: ${mark}`);
    }
  }
  // Recompute from the marks; never trust a caller-supplied percentile summary.
  const report = buildRoundtripStageReport(samples);
  assert.equal(report.complete_count, samples.length, 'every measurement must be complete');
  assert.deepEqual(report.issues, [], 'measurement stage ordering must be consistent');
  const total = report.summary.total.raw;
  assert.ok(Number.isFinite(total.p50) && total.p50 < 300,
    `warm command p50 must be below 300 ms; measured ${total.p50} ms`);
  return report;
}

if (process.argv[1] && pathToFileURL(resolve(process.argv[1])).href === import.meta.url) {
  assert.equal(process.argv.length, 4, 'usage: command-roundtrip-budget.mjs --input measured-marks.json');
  assert.equal(process.argv[2], '--input', 'a measured input file is required');
  const input = JSON.parse(readFileSync(process.argv[3], 'utf8'));
  const report = assertWarmCommandBudget(Array.isArray(input) ? input : input.samples);
  console.log(JSON.stringify({
    sampleCount: report.complete_count,
    warmCommandP50Ms: report.summary.total.raw.p50,
    warmCommandP95Ms: report.summary.total.raw.p95,
    limitP50Ms: 300,
  }));
}
