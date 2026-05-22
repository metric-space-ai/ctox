#!/usr/bin/env node
/*
 * Serial full-app Browser/Rust RxDB WebRTC smoke matrix.
 *
 * Requires a built CTOX binary at CTOX_BIN or the default integration target:
 *   cargo build --bin ctox --no-default-features --target-dir runtime/build/core-rxdb-integration-target
 *
 * Run the default full-app matrix:
 *   node src/core/rxdb/tools/browser_rust_smoke_matrix.js
 *
 * Run selected modes:
 *   SMOKE_MODES=rust-to-browser,workspace-rust-to-browser,workspace-update-rust-to-browser node src/core/rxdb/tools/browser_rust_smoke_matrix.js
 *   SMOKE_MODES=workspace-large-materialize-rust-to-browser node src/core/rxdb/tools/browser_rust_smoke_matrix.js
 *   SMOKE_MODES=workspace-large-file-viewer-rust-to-browser node src/core/rxdb/tools/browser_rust_smoke_matrix.js
 *   SMOKE_MODES=workspace-large-file-viewer-restart-rust-to-browser node src/core/rxdb/tools/browser_rust_smoke_matrix.js
 *   SMOKE_MODES=command-burst-browser-to-rust node src/core/rxdb/tools/browser_rust_smoke_matrix.js
 *   SMOKE_MODES=command-restart-browser-to-rust node src/core/rxdb/tools/browser_rust_smoke_matrix.js
 *   SMOKE_MODES=command-midflight-restart-browser-to-rust node src/core/rxdb/tools/browser_rust_smoke_matrix.js
 *   SMOKE_MODES=restart-browser-to-rust node src/core/rxdb/tools/browser_rust_smoke_matrix.js
 *   SMOKE_MODES=restart-signaling-browser-to-rust node src/core/rxdb/tools/browser_rust_smoke_matrix.js
 *   SMOKE_MODES=rollover-native-peer-browser-to-rust node src/core/rxdb/tools/browser_rust_smoke_matrix.js
 *   SMOKE_MODES=tab-freeze-browser-to-rust node src/core/rxdb/tools/browser_rust_smoke_matrix.js
 *   SMOKE_MODES=network-flap-browser-to-rust node src/core/rxdb/tools/browser_rust_smoke_matrix.js
 *   SMOKE_MODES=signaling-error-browser-status node src/core/rxdb/tools/browser_rust_smoke_matrix.js
 *   SMOKE_MODES=checkpoint-error-browser-status node src/core/rxdb/tools/browser_rust_smoke_matrix.js
 *   SMOKE_MODES=schema-error-browser-status node src/core/rxdb/tools/browser_rust_smoke_matrix.js
 */
const path = require('path');
const fs = require('fs');
const { spawnSync } = require('child_process');

const toolPath = path.join(__dirname, 'browser_rust_smoke.js');
const defaultModes = [
  'rust-to-browser',
  'browser-to-rust',
  'command-browser-to-rust',
  'command-burst-browser-to-rust',
  'command-restart-browser-to-rust',
  'command-midflight-restart-browser-to-rust',
  'restart-browser-to-rust',
  'restart-signaling-browser-to-rust',
  'rollover-native-peer-browser-to-rust',
  'tab-freeze-browser-to-rust',
  'network-flap-browser-to-rust',
  'signaling-error-browser-status',
  'checkpoint-error-browser-status',
  'schema-error-browser-status',
  'workspace-rust-to-browser',
  'workspace-update-rust-to-browser',
  'workspace-large-materialize-rust-to-browser',
  'workspace-large-file-viewer-rust-to-browser',
  'workspace-large-file-viewer-restart-rust-to-browser',
];
const modeEvidenceRequirements = {
  'rust-to-browser': { keys: ['replicated_id'] },
  'browser-to-rust': { keys: ['readiness_payload', 'replicated_id'] },
  'command-browser-to-rust': {
    keys: ['command_id', 'task_id', 'task_count_for_command', 'status', 'task_status'],
  },
  'command-burst-browser-to-rust': {
    keys: ['command_count', 'task_count_for_commands', 'command_ids', 'task_ids'],
  },
  'command-restart-browser-to-rust': {
    keys: ['command_id', 'task_id', 'task_count_for_command', 'status', 'task_status'],
  },
  'command-midflight-restart-browser-to-rust': {
    keys: ['command_id', 'task_id', 'task_count_for_command', 'status', 'task_status'],
  },
  'restart-browser-to-rust': {
    keys: ['advanced_status', 'readiness_payload', 'replicated_id'],
    values: { advanced_status: 'business-os-advanced-status-v1' },
  },
  'restart-signaling-browser-to-rust': {
    keys: ['advanced_status', 'readiness_payload', 'replicated_id'],
    values: { advanced_status: 'business-os-advanced-status-v1' },
  },
  'rollover-native-peer-browser-to-rust': {
    keys: ['advanced_status', 'readiness_payload', 'replicated_id'],
    values: { advanced_status: 'business-os-advanced-status-v1' },
  },
  'tab-freeze-browser-to-rust': {
    keys: ['advanced_status', 'readiness_payload', 'replicated_id'],
    values: { advanced_status: 'business-os-advanced-status-v1' },
  },
  'network-flap-browser-to-rust': {
    keys: ['advanced_status', 'readiness_payload', 'replicated_id'],
    values: { advanced_status: 'business-os-advanced-status-v1' },
  },
  'signaling-error-browser-status': {
    keys: ['signaling_error_collection', 'signaling_error_code', 'signaling_error_name'],
    values: {
      signaling_error_code: 'instance_mismatch',
      signaling_error_name: 'CtoxSignalingControlPlaneError',
    },
  },
  'checkpoint-error-browser-status': {
    keys: ['checkpoint_error_collection', 'checkpoint_error_code', 'checkpoint_error_name'],
    values: {
      checkpoint_error_code: 'ctox_checkpoint_epoch_missing',
      checkpoint_error_name: 'CtoxCheckpointProtocolError',
    },
  },
  'schema-error-browser-status': {
    keys: ['schema_error_collection', 'schema_error_code', 'schema_error_name'],
    values: {
      schema_error_code: 'ctox_schema_hash_mismatch',
      schema_error_name: 'CtoxSchemaProtocolError',
    },
  },
  'workspace-rust-to-browser': { keys: ['replicated_id'] },
  'workspace-update-rust-to-browser': {
    keys: ['replicated_id', 'previous_generation', 'updated_generation'],
  },
  'workspace-large-materialize-rust-to-browser': {
    keys: ['replicated_id', 'generation', 'chunk_count', 'payload_length'],
  },
  'workspace-large-file-viewer-rust-to-browser': {
    keys: ['replicated_id', 'generation', 'chunk_count', 'payload_length'],
  },
  'workspace-large-file-viewer-restart-rust-to-browser': {
    keys: ['advanced_status', 'replicated_id', 'generation', 'chunk_count', 'payload_length'],
    values: { advanced_status: 'business-os-advanced-status-v1' },
  },
};
const modes = (process.env.SMOKE_MODES || defaultModes.join(','))
  .split(/[,\s]+/)
  .map((mode) => mode.trim())
  .filter(Boolean);
const knownModes = new Set(defaultModes);
const pagePath = process.env.SMOKE_PAGE_PATH || '/index.html';
const businessPortBaseInput = process.env.BUSINESS_PORT || '8877';
const signalingPortBaseInput = process.env.SIGNALING_PORT || '18876';
const attemptsInput = process.env.SMOKE_MATRIX_ATTEMPTS || '2';
const resultPath = process.env.SMOKE_MATRIX_RESULT_PATH || '';
const requireEvidence = process.env.SMOKE_REQUIRE_EVIDENCE !== '0';
const summary = {
  pagePath,
  requireEvidence,
  modes: [],
  startedAt: new Date().toISOString(),
  endedAt: null,
  ok: false,
};
const businessPortBase = parsePositiveIntegerConfig('BUSINESS_PORT', businessPortBaseInput, { max: 65535 });
const signalingPortBase = parsePositiveIntegerConfig('SIGNALING_PORT', signalingPortBaseInput, { max: 65535 });
const attempts = parsePositiveIntegerConfig('SMOKE_MATRIX_ATTEMPTS', attemptsInput, { max: 20 });

const unknownModes = modes.filter((mode) => !knownModes.has(mode));
if (!modes.length) {
  failConfiguration('SMOKE_MODES did not contain any smoke modes');
}
if (unknownModes.length) {
  failConfiguration(`SMOKE_MODES contains unsupported mode(s): ${unknownModes.join(', ')}`);
}
const maxPortOffset = (modes.length * attempts) - 1;
if (businessPortBase + maxPortOffset > 65535) {
  failConfiguration(`BUSINESS_PORT plus matrix port range exceeds 65535: ${businessPortBase}+${maxPortOffset}`);
}
if (signalingPortBase + maxPortOffset > 65535) {
  failConfiguration(`SIGNALING_PORT plus matrix port range exceeds 65535: ${signalingPortBase}+${maxPortOffset}`);
}

for (const [index, mode] of modes.entries()) {
  let lastStatus = 1;
  let lastSignal = null;
  const modeSummary = {
    mode,
    attempts: [],
    ok: false,
  };
  summary.modes.push(modeSummary);
  for (let attempt = 1; attempt <= attempts; attempt++) {
    const portOffset = index * attempts + (attempt - 1);
    const env = {
      ...process.env,
      SMOKE_PAGE_PATH: pagePath,
      SMOKE_MODE: mode,
      BUSINESS_PORT: String(businessPortBase + portOffset),
      SIGNALING_PORT: String(signalingPortBase + portOffset),
    };
    console.log(`\n=== rxdb smoke: ${mode} (${pagePath}) attempt ${attempt}/${attempts} ===`);
    const result = spawnSync(process.execPath, [toolPath], {
      cwd: path.resolve(__dirname, '../../../..'),
      env,
      encoding: 'utf8',
      stdio: ['ignore', 'pipe', 'pipe'],
    });
    if (result.stdout) process.stdout.write(result.stdout);
    if (result.stderr) process.stderr.write(result.stderr);
    const evidence = parseSmokeEvidence(`${result.stdout || ''}\n${result.stderr || ''}`);
    const evidenceProblems = requireEvidence && result.status === 0 && !result.signal
      ? validateModeEvidence(mode, evidence)
      : [];
    if (evidenceProblems.length) {
      console.error(`smoke ${mode} missing required evidence: ${evidenceProblems.join(', ')}`);
    }
    lastStatus = evidenceProblems.length ? 1 : (result.status || 0);
    lastSignal = result.signal || null;
    modeSummary.attempts.push({
      attempt,
      status: result.status,
      signal: result.signal || null,
      businessPort: Number(env.BUSINESS_PORT),
      signalingPort: Number(env.SIGNALING_PORT),
      ok: lastStatus === 0 && !lastSignal,
      evidence,
      evidenceProblems,
    });
    if (lastStatus === 0 && !lastSignal) break;
    if (attempt < attempts) {
      console.error(`smoke ${mode} failed; retrying once`);
    }
  }
  modeSummary.ok = lastStatus === 0 && !lastSignal;
  if (lastSignal) {
    console.error(`smoke ${mode} terminated by signal ${lastSignal}`);
    writeSummary(false);
    process.exit(1);
  }
  if (lastStatus !== 0) {
    writeSummary(false);
    process.exit(lastStatus || 1);
  }
}

writeSummary(true);
console.log(`\nrxdb smoke matrix OK: ${modes.join(', ')}`);

function writeSummary(ok) {
  summary.ok = ok;
  summary.endedAt = new Date().toISOString();
  if (!resultPath) return;
  fs.mkdirSync(path.dirname(resultPath), { recursive: true });
  fs.writeFileSync(resultPath, `${JSON.stringify(summary, null, 2)}\n`);
}

function failConfiguration(message) {
  summary.configurationError = message;
  console.error(`rxdb smoke matrix configuration error: ${message}`);
  writeSummary(false);
  process.exit(1);
}

function parsePositiveIntegerConfig(name, value, options = {}) {
  const parsed = Number(value);
  const min = options.min ?? 1;
  const max = options.max ?? Number.MAX_SAFE_INTEGER;
  if (!Number.isInteger(parsed) || parsed < min || parsed > max) {
    failConfiguration(`${name} must be an integer between ${min} and ${max}; got ${JSON.stringify(String(value))}`);
  }
  return parsed;
}

function parseSmokeEvidence(output) {
  const evidence = {};
  for (const line of String(output || '').split(/\r?\n/)) {
    const match = line.match(/^([a-zA-Z][a-zA-Z0-9_:-]*)=(.*)$/);
    if (!match) continue;
    const [, key, rawValue] = match;
    const value = rawValue.trim();
    if (!value) {
      evidence[key] = '';
      continue;
    }
    const numeric = Number(value);
    evidence[key] = Number.isFinite(numeric) && String(numeric) === value ? numeric : value;
  }
  return evidence;
}

function validateModeEvidence(mode, evidence) {
  const required = evidenceRequirementsForMode(mode);
  const problems = [];
  for (const key of required.keys) {
    if (!Object.prototype.hasOwnProperty.call(evidence, key)) {
      problems.push(key);
    }
  }
  for (const [key, expected] of Object.entries(required.values || {})) {
    if (evidence[key] !== expected) {
      problems.push(`${key}=${JSON.stringify(expected)}`);
    }
  }
  return problems;
}

function evidenceRequirementsForMode(mode) {
  const required = modeEvidenceRequirements[mode];
  if (!required) {
    throw new Error(`No smoke evidence requirements registered for mode=${mode}`);
  }
  return required;
}
