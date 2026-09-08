// Canonical entry point for the ctox-rxdb smoke + guard suite.
//
//   node src/apps/business-os/rxdb/tests/run-all.mjs [--fail-fast]
//     [--require-wire-daemon] [--wire-daemon /absolute/path/to/v15_wire_daemon]
//
// Runs every *-smoke.mjs in this directory in its own node process (tests
// mutate globals like WebSocket/RTCPeerConnection, so isolation matters),
// prints a pass/fail table, and exits non-zero if anything failed. This is
// the command an agent MUST run (and keep green) after touching anything
// under src/apps/business-os/rxdb/, src/apps/business-os/shared/sync.js, or
// src/core/rxdb/src/plugins/replication_webrtc/.
//
// Notes:
// - cross-process-* tests need a release or debug wire daemon:
//     (cd src/core/rxdb && CARGO_TARGET_DIR=<repo>/runtime/build/cargo-target \
//        cargo build --release --example v15_wire_daemon)
//   They SKIP (loudly) when the binary is missing so the JS-only suite stays
//   runnable. CI builds the daemon and passes --require-wire-daemon, which
//   turns a missing binary into a hard failure instead of a skip — the
//   cross-process smokes are the only proof that the JS and Rust sides agree
//   on real wire bytes, so a silent skip there is missing coverage.
//   --wire-daemon selects one exact build for the runner and both child tests;
//   a missing explicit path fails instead of falling back to another build.
// - A red test is a finding, not noise. Never delete or weaken a test to make
//   this suite pass; fix the code or update the pinned contract on purpose.

import { execFileSync } from 'node:child_process';
import { readdirSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { resolveWireDaemon } from './wire-daemon-fixture.mjs';

const testDir = dirname(fileURLToPath(import.meta.url));
const failFast = process.argv.includes('--fail-fast');

const daemonBin = resolveWireDaemon();
const daemonAvailable = Boolean(daemonBin);
const requireWireDaemon = process.argv.includes('--require-wire-daemon');

if (requireWireDaemon && !daemonAvailable) {
  console.error('--require-wire-daemon: v15_wire_daemon binary not found, cross-process smokes cannot run.');
  console.error('Build it with:');
  console.error('  (cd src/core/rxdb && CARGO_TARGET_DIR=<repo>/runtime/build/cargo-target \\');
  console.error('     cargo build --release --example v15_wire_daemon)');
  process.exit(1);
}

const tests = readdirSync(testDir)
  .filter((name) => name.endsWith('-smoke.mjs'))
  .sort();

if (daemonBin) console.log(`Wire daemon: ${daemonBin}`);

const results = [];
let failed = 0;
let skipped = 0;

for (const name of tests) {
  if (name.startsWith('cross-process-') && !daemonAvailable) {
    results.push({ name, status: 'SKIP', detail: 'wire daemon not built' });
    skipped += 1;
    continue;
  }
  const startedAt = Date.now();
  try {
    const args = [join(testDir, name)];
    if (name.startsWith('cross-process-')) args.push('--wire-daemon', daemonBin);
    execFileSync(process.execPath, args, {
      stdio: 'pipe',
      timeout: 180_000,
    });
    results.push({ name, status: 'PASS', detail: `${Date.now() - startedAt}ms` });
  } catch (error) {
    failed += 1;
    const stderr = String(error?.stderr || '').trim().split('\n').slice(-6).join('\n      ');
    results.push({ name, status: 'FAIL', detail: stderr || String(error?.message || error) });
    if (failFast) break;
  }
}

const width = Math.max(...tests.map((name) => name.length)) + 2;
for (const { name, status, detail } of results) {
  console.log(`${status.padEnd(5)} ${name.padEnd(width)} ${status === 'FAIL' ? `\n      ${detail}` : detail}`);
}
console.log(`\nctox-rxdb suite: ${results.length - failed - skipped} passed, ${failed} failed, ${skipped} skipped (${tests.length} total)`);
if (skipped) {
  console.log('SKIPPED tests still count as missing coverage — build the wire daemon to run them.');
}
process.exit(failed ? 1 : 0);
