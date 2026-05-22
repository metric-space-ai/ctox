#!/usr/bin/env node
/*
 * Print a compact, CI-readable summary for browser_rust_soak.js JSON output.
 */
const fs = require('fs');

const summaryPath = process.argv[2] || process.env.SOAK_RESULT_PATH || 'rxdb-soak-summary.json';

if (!fs.existsSync(summaryPath)) {
  console.warn(`rxdb soak summary not found: ${summaryPath}`);
  process.exit(0);
}

const summary = JSON.parse(fs.readFileSync(summaryPath, 'utf8'));
console.log(`rxdb_soak_summary ok=${Boolean(summary.ok)} cycles=${summary.cycles ?? ''} retries=${summary.retryCount ?? 0} modes=${summary.modes || ''}`);

const attemptRows = [];
const cycleErrors = [];
if (summary.configurationError) {
  cycleErrors.push({ cycle: '', error: summary.configurationError });
  console.log(`rxdb_soak_cycle_error cycle= error=${formatEvidenceValue(summary.configurationError)}`);
}
for (const cycle of summary.cycleResults || []) {
  const cycleError = cycle.configurationError || cycle.error || '';
  if (cycleError) {
    cycleErrors.push({ cycle: cycle.cycle ?? '', error: cycleError });
    console.log(`rxdb_soak_cycle_error cycle=${cycle.cycle ?? ''} error=${formatEvidenceValue(cycleError)}`);
  }
  for (const mode of cycle.modes || []) {
    for (const attempt of mode.attempts || []) {
      const evidence = attempt.evidence || {};
      const evidencePairs = Object.keys(evidence)
        .sort()
        .map((key) => `${key}=${formatEvidenceValue(evidence[key])}`)
        .join(' ');
      const problems = Array.isArray(attempt.evidenceProblems) && attempt.evidenceProblems.length
        ? ` evidenceProblems=${attempt.evidenceProblems.join(',')}`
        : '';
      attemptRows.push({ cycle, mode, attempt, evidencePairs, problems: problems.trim() });
      console.log([
        `rxdb_soak_attempt cycle=${cycle.cycle ?? ''}`,
        `mode=${mode.mode}`,
        `attempt=${attempt.attempt}`,
        `ok=${Boolean(attempt.ok)}`,
        `status=${attempt.status ?? ''}`,
        `signal=${attempt.signal || ''}`,
        `businessPort=${attempt.businessPort ?? ''}`,
        `signalingPort=${attempt.signalingPort ?? ''}`,
        evidencePairs,
        problems,
      ].filter(Boolean).join(' '));
    }
  }
}

writeGitHubStepSummary(summary, attemptRows, cycleErrors);

if (!summary.ok) process.exit(1);

function formatEvidenceValue(value) {
  const raw = redactSensitiveValue(String(value ?? ''));
  if (!raw.includes(' ') && raw.length <= 120) return raw;
  return JSON.stringify(raw.length > 240 ? `${raw.slice(0, 237)}...` : raw);
}

function writeGitHubStepSummary(summary, rows, cycleErrors) {
  const target = process.env.GITHUB_STEP_SUMMARY;
  if (!target) return;

  const lines = [
    '## RxDB WebRTC Soak',
    '',
    `- Result: ${summary.ok ? 'passed' : 'failed'}`,
    `- Cycles: ${summary.cycles ?? ''}`,
    `- Retries: ${summary.retryCount ?? 0}`,
    `- Modes: ${summary.modes || ''}`,
    '',
    '| Cycle | Mode | Attempt | Result | Status | Evidence | Problems |',
    '|---:|---|---:|---|---:|---|---|',
  ];

  if (!rows.length) {
    lines.push('|  | none |  | failed |  | none | see cycle errors |');
  }
  for (const row of rows) {
    const evidence = truncateCell(row.evidencePairs || 'none', 600);
    lines.push([
      row.cycle.cycle ?? '',
      row.mode.mode || '',
      row.attempt.attempt ?? '',
      row.attempt.ok ? 'passed' : 'failed',
      row.attempt.status ?? '',
      evidence,
      row.problems || 'none',
    ].map(markdownCell).join('|').replace(/^/, '|').replace(/$/, '|'));
  }

  if (cycleErrors.length) {
    lines.push('', '### Cycle Errors', '', '| Cycle | Error |', '|---:|---|');
    for (const error of cycleErrors) {
      lines.push([
        error.cycle,
        truncateCell(formatEvidenceValue(error.error), 600),
      ].map(markdownCell).join('|').replace(/^/, '|').replace(/$/, '|'));
    }
  }

  fs.appendFileSync(target, `${lines.join('\n')}\n`, 'utf8');
}

function markdownCell(value) {
  return ` ${String(value ?? '').replace(/\r?\n/g, ' ').replace(/\|/g, '\\|')} `;
}

function truncateCell(value, maxLength) {
  const raw = String(value ?? '');
  if (raw.length <= maxLength) return raw;
  return `${raw.slice(0, maxLength - 3)}...`;
}

function redactSensitiveValue(value) {
  return value.replace(
    /((?:token|password|secret|authorization|auth|ctox_config|room_password)[^=\s]*=)[^&\s]+/gi,
    '$1[redacted]',
  );
}
