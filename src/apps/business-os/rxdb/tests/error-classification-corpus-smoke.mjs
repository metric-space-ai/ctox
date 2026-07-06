// OS-G1: shared error-classification corpus (JS side).
//
// The browser's error$ cascade order is load-bearing (control-plane ->
// schema-protocol -> replication-io -> transient-shutdown -> peer-lifecycle
// -> signaling-blip -> generic; see the guard comment in sync.js). This
// smoke runs every corpus case from
// src/core/rxdb/tests/fixtures/replication-error-classification.json through
// the pure `classifyReplicationErrorKind` chain and asserts kind + code —
// including explicit ORDER PIN cases whose payloads match several
// classifiers at once. The rxdb-rs contract test consumes the same fixture
// to keep the ctox_rxdb_* codes aligned with the generated protocol
// contract, so a one-sided rename fails a test instead of shipping as
// "network flakiness".

import { readFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import { dirname, resolve } from 'node:path';
import { __ctoxSyncTestHooks } from '../../shared/sync.js';

const { classifyReplicationErrorKind } = __ctoxSyncTestHooks;

const here = dirname(fileURLToPath(import.meta.url));
const fixturePath = resolve(
  here, '..', '..', '..', '..',
  'core', 'rxdb', 'tests', 'fixtures', 'replication-error-classification.json',
);
const fixture = JSON.parse(readFileSync(fixturePath, 'utf8'));

const assert = (condition, message) => {
  if (!condition) throw new Error(message);
};

assert(Array.isArray(fixture.cases) && fixture.cases.length >= 10,
  'corpus must not silently shrink');

for (const testCase of fixture.cases) {
  const { kind, classified } = classifyReplicationErrorKind(testCase.collection, testCase.error);
  assert(
    kind === testCase.expectedKind,
    `case "${testCase.name}": expected kind ${testCase.expectedKind}, got ${kind}`,
  );
  if (testCase.expectedCode === null) {
    assert(
      classified === null,
      `case "${testCase.name}": ${testCase.expectedKind} must not produce a classified error object`,
    );
  } else {
    assert(
      classified?.code === testCase.expectedCode,
      `case "${testCase.name}": expected code ${testCase.expectedCode}, got ${classified?.code}`,
    );
  }
}

// The kinds the corpus covers must span the full cascade, so a future edit
// cannot quietly drop a branch from coverage.
const coveredKinds = new Set(fixture.cases.map((testCase) => testCase.expectedKind));
for (const kind of [
  'control-plane', 'schema-protocol', 'replication-io',
  'transient-shutdown', 'peer-lifecycle', 'signaling-blip', 'generic',
]) {
  assert(coveredKinds.has(kind), `corpus must cover the ${kind} branch`);
}

console.log(`ctox-rxdb error-classification corpus smoke OK (${fixture.cases.length} cases)`);
process.exit(0);
