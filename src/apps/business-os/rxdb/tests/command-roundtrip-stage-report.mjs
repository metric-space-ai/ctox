// Command-roundtrip stage report (AP1 measurement packet).
//
// Reads correlated marks, corrects a documented browser/native clock offset,
// and prints p50/p95/min/max per stage. Secrets, capability tokens and
// command payloads are never printed.
//
//   node src/apps/business-os/rxdb/tests/command-roundtrip-stage-report.mjs
//     --synthetic 30
//   node src/apps/business-os/rxdb/tests/command-roundtrip-stage-report.mjs
//     --input marks.json
//
// Input document: { samples: [{ command_id, marks: { <seven marks> } }] }
// or a bare array of those sample objects.

import { readFileSync, writeFileSync } from 'node:fs';
import { dirname, resolve } from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';

const MARK_NAMES = [
  'browser_dispatch_started',
  'browser_local_inserted',
  'browser_push_confirmed',
  'native_dispatch_entered',
  'native_handler_completed',
  'native_rxdb_projection_committed',
  'browser_terminal_observed',
];

const STAGE_DEFS = [
  ['browser_insert', 'browser_dispatch_started', 'browser_local_inserted'],
  ['push', 'browser_local_inserted', 'browser_push_confirmed'],
  // Native intake can precede the Browser's flush acknowledgement. These
  // two spans overlap; local insertion is the causal predecessor of intake.
  ['local_to_native_intake', 'browser_local_inserted', 'native_dispatch_entered'],
  ['native_processing', 'native_dispatch_entered', 'native_handler_completed'],
  ['projection_commit', 'native_handler_completed', 'native_rxdb_projection_committed'],
  ['commit_to_browser_observed', 'native_rxdb_projection_committed', 'browser_terminal_observed'],
  ['total', 'browser_dispatch_started', 'browser_terminal_observed'],
];

const SUM_STAGES = STAGE_DEFS
  .map(([name]) => name)
  .filter((name) => name !== 'total' && name !== 'push');

const NATIVE_MARKS = [
  'native_dispatch_entered',
  'native_handler_completed',
  'native_rxdb_projection_committed',
];

export function commandRoundtripStagesFromMarks(marks = {}) {
  const numeric = {};
  for (const name of MARK_NAMES) {
    const value = Number(marks[name]);
    if (!Number.isFinite(value)) return null;
    numeric[name] = value;
  }
  const stages = {};
  for (const [name, from, to] of STAGE_DEFS) {
    stages[name] = numeric[to] - numeric[from];
  }
  return stages;
}

export function estimateNativeClockOffsetMs(marks) {
  const inserted = Number(marks.browser_local_inserted);
  const intake = Number(marks.native_dispatch_entered);
  const committed = Number(marks.native_rxdb_projection_committed);
  const observed = Number(marks.browser_terminal_observed);
  if (![inserted, intake, committed, observed].every(Number.isFinite)) return null;
  // D = native_clock - browser_clock.
  // Native intake follows local insertion, but may precede the asynchronous
  // Browser push acknowledgement. Only causal marks bound clock offset.
  const low = committed - observed;
  const high = intake - inserted;
  if (low <= high) return (low + high) / 2;
  return null;
}

export function correctMarksToBrowserClock(marks, offsetMs = estimateNativeClockOffsetMs(marks)) {
  if (!Number.isFinite(offsetMs)) {
    throw new Error('Browser/native clock offset has no causally feasible interval');
  }
  const corrected = { ...marks };
  for (const name of NATIVE_MARKS) {
    const value = Number(marks[name]);
    if (Number.isFinite(value)) corrected[name] = value - offsetMs;
  }
  return { marks: corrected, clock_offset_ms: offsetMs };
}

export function summarizeStage(values) {
  const sorted = values.slice().sort((a, b) => a - b);
  if (sorted.length === 0) {
    return { count: 0, min: null, max: null, p50: null, p95: null };
  }
  return {
    count: sorted.length,
    min: sorted[0],
    max: sorted[sorted.length - 1],
    p50: percentile(sorted, 0.5),
    p95: percentile(sorted, 0.95),
  };
}

function percentile(sorted, rank) {
  if (sorted.length === 1) return sorted[0];
  const index = (sorted.length - 1) * rank;
  const lower = Math.floor(index);
  const upper = Math.ceil(index);
  if (lower === upper) return sorted[lower];
  const weight = index - lower;
  return sorted[lower] + (sorted[upper] - sorted[lower]) * weight;
}

export function buildRoundtripStageReport(samples, { maxStageSumDeltaMs = 50 } = {}) {
  const rows = [];
  const issues = [];
  for (const sample of samples) {
    const commandId = String(sample.command_id || sample.id || '').slice(0, 120);
    const rawMarks = sample.marks || {};
    const missing = MARK_NAMES.filter((name) => !Number.isFinite(Number(rawMarks[name])));
    if (missing.length) {
      issues.push({ command_id: commandId, kind: 'incomplete', missing });
      continue;
    }
    const clockOffsetMs = estimateNativeClockOffsetMs(rawMarks);
    if (clockOffsetMs === null) {
      issues.push({ command_id: commandId, kind: 'clock_offset_infeasible' });
      continue;
    }
    const { marks: correctedMarks } = correctMarksToBrowserClock(rawMarks, clockOffsetMs);
    const rawStages = commandRoundtripStagesFromMarks(rawMarks);
    const stages = commandRoundtripStagesFromMarks(correctedMarks);
    const negatives = Object.entries(stages)
      .filter(([, value]) => value < 0)
      .map(([name, value]) => ({ name, value }));
    const stageSum = SUM_STAGES.reduce((sum, name) => sum + stages[name], 0);
    const stageSumDeltaMs = stageSum - stages.total;
    if (negatives.length) {
      issues.push({ command_id: commandId, kind: 'negative_duration', negatives });
    }
    if (Math.abs(stageSumDeltaMs) > maxStageSumDeltaMs) {
      issues.push({
        command_id: commandId,
        kind: 'stage_sum_mismatch',
        stage_sum_ms: stageSum,
        total_ms: stages.total,
        delta_ms: stageSumDeltaMs,
      });
    }
    rows.push({
      command_id: commandId,
      clock_offset_ms: clockOffsetMs,
      marks_raw: pickMarks(rawMarks),
      marks_corrected: pickMarks(correctedMarks),
      stages_raw: rawStages,
      stages_corrected: stages,
      stage_sum_delta_ms: stageSumDeltaMs,
    });
  }
  const summary = {};
  for (const [name] of STAGE_DEFS) {
    summary[name] = {
      raw: summarizeStage(rows.map((row) => row.stages_raw[name])),
      corrected: summarizeStage(rows.map((row) => row.stages_corrected[name])),
    };
  }
  return {
    schema: 'ctox.command_roundtrip.stage_report.v2',
    sample_count: samples.length,
    complete_count: rows.length,
    issues,
    summary,
    samples: rows,
  };
}

function pickMarks(marks) {
  const picked = {};
  for (const name of MARK_NAMES) picked[name] = Number(marks[name]);
  return picked;
}

export function synthesizeRoundtripSamples(count = 30, { seed = 20260813, nativeClockLeadMs = 37 } = {}) {
  const samples = [];
  for (let index = 0; index < count; index += 1) {
    const wave = seededUnit(seed, index);
    const started = 1_700_000_000_000 + index * 17_000;
    const browserInsert = 4 + Math.round(wave * 12);
    const push = 8 + Math.round(seededUnit(seed, index + 31) * 40);
    const nativeIntake = 6 + Math.round(seededUnit(seed, index + 67) * 25);
    const nativeProcessing = 3 + Math.round(seededUnit(seed, index + 97) * 18);
    const projectionCommit = 2 + Math.round(seededUnit(seed, index + 131) * 9);
    const commitToBrowser = 7 + Math.round(seededUnit(seed, index + 173) * 30);
    const browserDispatchStarted = started;
    const browserLocalInserted = browserDispatchStarted + browserInsert;
    const browserPushConfirmed = browserLocalInserted + push;
    const nativeDispatchEntered = browserPushConfirmed + nativeIntake + nativeClockLeadMs;
    const nativeHandlerCompleted = nativeDispatchEntered + nativeProcessing;
    const nativeRxdbProjectionCommitted = nativeHandlerCompleted + projectionCommit;
    const browserTerminalObserved = (nativeRxdbProjectionCommitted - nativeClockLeadMs) + commitToBrowser;
    samples.push({
      command_id: `cmd_synthetic_${String(index + 1).padStart(2, '0')}`,
      marks: {
        browser_dispatch_started: browserDispatchStarted,
        browser_local_inserted: browserLocalInserted,
        browser_push_confirmed: browserPushConfirmed,
        native_dispatch_entered: nativeDispatchEntered,
        native_handler_completed: nativeHandlerCompleted,
        native_rxdb_projection_committed: nativeRxdbProjectionCommitted,
        browser_terminal_observed: browserTerminalObserved,
      },
    });
  }
  return samples;
}

function seededUnit(seed, index) {
  const x = Math.sin(seed * 12.9898 + index * 78.233) * 43758.5453;
  return x - Math.floor(x);
}

export function assertSyntheticReportInvariants(report, { maxStageSumDeltaMs = 50 } = {}) {
  if (report.complete_count !== 30) {
    throw new Error(`expected 30 complete samples, got ${report.complete_count}`);
  }
  if (report.issues.length) {
    throw new Error(`synthetic report has issues: ${JSON.stringify(report.issues)}`);
  }
  for (const row of report.samples) {
    for (const name of MARK_NAMES) {
      if (!Number.isFinite(row.marks_raw[name]) || !Number.isFinite(row.marks_corrected[name])) {
        throw new Error(`${row.command_id} is missing mark ${name}`);
      }
    }
    for (const [name, value] of Object.entries(row.stages_corrected)) {
      if (value < 0) throw new Error(`${row.command_id} has negative ${name}=${value}`);
    }
    if (Math.abs(row.stage_sum_delta_ms) > maxStageSumDeltaMs) {
      throw new Error(
        `${row.command_id} stage sum delta ${row.stage_sum_delta_ms} exceeds ±${maxStageSumDeltaMs} ms`,
      );
    }
  }
  return true;
}

function parseArgs(argv) {
  const options = { synthetic: 0, input: '', output: '' };
  for (let index = 0; index < argv.length; index += 1) {
    const arg = argv[index];
    if (arg === '--synthetic') {
      options.synthetic = Number(argv[index + 1] || 30);
      index += 1;
    } else if (arg === '--input') {
      options.input = String(argv[index + 1] || '');
      index += 1;
    } else if (arg === '--output') {
      options.output = String(argv[index + 1] || '');
      index += 1;
    }
  }
  return options;
}

function loadSamples(options) {
  if (options.input) {
    const parsed = JSON.parse(readFileSync(options.input, 'utf8'));
    return Array.isArray(parsed) ? parsed : (parsed.samples || []);
  }
  return synthesizeRoundtripSamples(options.synthetic || 30);
}

function printReport(report) {
  const lines = [
    `schema=${report.schema}`,
    `samples=${report.complete_count}/${report.sample_count}`,
    'stage                     p50_raw   p95_raw   min_raw   max_raw   p50_corr  p95_corr  min_corr  max_corr',
  ];
  for (const [name] of STAGE_DEFS) {
    const raw = report.summary[name].raw;
    const corr = report.summary[name].corrected;
    lines.push([
      name.padEnd(24),
      fmt(raw.p50),
      fmt(raw.p95),
      fmt(raw.min),
      fmt(raw.max),
      fmt(corr.p50),
      fmt(corr.p95),
      fmt(corr.min),
      fmt(corr.max),
    ].join('  '));
  }
  if (report.issues.length) {
    lines.push(`issues=${report.issues.length}`);
    for (const issue of report.issues) {
      lines.push(`  ${issue.command_id || '?'} ${issue.kind}`);
    }
  }
  console.log(lines.join('\n'));
}

function fmt(value) {
  if (!Number.isFinite(value)) return '     n/a';
  return value.toFixed(2).padStart(8);
}

const invokedAsScript = process.argv[1]
  && pathToFileURL(resolve(process.argv[1])).href === import.meta.url;

if (invokedAsScript) {
  const options = parseArgs(process.argv.slice(2));
  const samples = loadSamples(options);
  const report = buildRoundtripStageReport(samples);
  if (!options.input) assertSyntheticReportInvariants(report);
  printReport(report);
  if (options.output) {
    writeFileSync(options.output, `${JSON.stringify(report, null, 2)}\n`);
  }
}
