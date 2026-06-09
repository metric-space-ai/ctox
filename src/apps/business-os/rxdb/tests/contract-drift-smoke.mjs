// GUARD: wire-contract drift between rxdb-rs (Rust) and ctox-rxdb-js (browser).
//
// The four `*generated*` contract files (frame + protocol, one per side) are
// GENERATED from the shared fixtures in src/core/rxdb/tests/fixtures/ by the
// two build_webrtc_*_contract.mjs tools. Hand-editing any of them desyncs the
// two peers' wire expectations (frame sizes, capability lists, RPC names) and
// produces sync failures that look like network flakiness.
//
// This guard re-runs both generators and fails if any generated file in the
// worktree differs from what the fixtures produce. It always restores the
// original worktree contents, so it is side-effect free even on failure.
//
// If this test fails you (the agent reading this) must NOT "fix" it by editing
// the generated files or this test. Change the fixture, re-run the generators,
// update BOTH sides' consumers, and add/extend tests for the new contract.

import { execFileSync } from 'node:child_process';
import { readFileSync, writeFileSync } from 'node:fs';
import { dirname, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

const testDir = dirname(fileURLToPath(import.meta.url));
const repoRoot = resolve(testDir, '../../../../..');
const srcRoot = resolve(repoRoot, 'src');

const GENERATORS = [
  'src/core/rxdb/tools/build_webrtc_frame_protocol_contract.mjs',
  'src/core/rxdb/tools/build_webrtc_rxdb_protocol_contract.mjs',
];
const GENERATED = [
  'src/core/rxdb/src/plugins/replication_webrtc/frame_contract_generated.rs',
  'src/core/rxdb/src/plugins/replication_webrtc/protocol_contract_generated.rs',
  'src/apps/business-os/rxdb/src/frame-contract.generated.mjs',
  'src/apps/business-os/rxdb/src/protocol-contract.generated.mjs',
];

const originals = new Map();
for (const rel of GENERATED) {
  originals.set(rel, readFileSync(resolve(repoRoot, rel), 'utf8'));
}

let regenerated;
try {
  for (const generator of GENERATORS) {
    execFileSync(process.execPath, [resolve(repoRoot, generator)], {
      cwd: srcRoot,
      stdio: 'pipe',
    });
  }
  regenerated = new Map();
  for (const rel of GENERATED) {
    regenerated.set(rel, readFileSync(resolve(repoRoot, rel), 'utf8'));
  }
} finally {
  // Side-effect free: always restore the worktree contents.
  for (const [rel, content] of originals) {
    writeFileSync(resolve(repoRoot, rel), content);
  }
}

const drifted = [];
for (const rel of GENERATED) {
  if (originals.get(rel) !== regenerated.get(rel)) drifted.push(rel);
}

if (drifted.length) {
  console.error('CONTRACT DRIFT: generated wire-contract files do not match their fixtures.');
  console.error('Drifted files:');
  for (const rel of drifted) console.error(`  - ${rel}`);
  console.error('Never hand-edit generated contract files. Change the fixture in');
  console.error('src/core/rxdb/tests/fixtures/ and re-run the generators instead:');
  for (const generator of GENERATORS) console.error(`  node ${generator}`);
  process.exit(1);
}

// Semantic parity spot-checks: the values both peers must agree on, read from
// the live source files (not the fixture), so a partial regeneration or a
// one-sided manual edit is caught even if the generator output format changes.
const rustFrame = originals.get(GENERATED[0]);
const jsFrame = originals.get(GENERATED[2]);
const pairs = [
  ['MAX_INLINE_FRAME_BYTES', /MAX_INLINE_FRAME_BYTES[^=]*= (\d+)/],
  ['MAX_TRANSFER_BYTES', /MAX_TRANSFER_BYTES[^=]*= (\d+)/],
  ['FRAME_ACK_WINDOW', /FRAME_ACK_WINDOW[^=]*= (\d+)/],
  ['MAX_FRAME_RETRIES', /MAX_FRAME_RETRIES[^=]*= (\d+)/],
];
for (const [name, pattern] of pairs) {
  const rust = rustFrame.match(pattern)?.[1];
  const js = jsFrame.match(pattern)?.[1];
  if (!rust || !js || rust !== js) {
    console.error(`CONTRACT PARITY VIOLATION: ${name} differs (rust=${rust}, js=${js})`);
    process.exit(1);
  }
}
const protocolName = /ctox-rxdb-protocol-v\d+/;
const rustProtocol = originals.get(GENERATED[1]).match(protocolName)?.[0];
const jsProtocol = originals.get(GENERATED[3]).match(protocolName)?.[0];
if (!rustProtocol || rustProtocol !== jsProtocol) {
  console.error(`CONTRACT PARITY VIOLATION: protocol name differs (rust=${rustProtocol}, js=${jsProtocol})`);
  process.exit(1);
}

console.log('ctox-rxdb contract drift guard OK', {
  generated: GENERATED.length,
  parityChecks: pairs.length + 1,
});
