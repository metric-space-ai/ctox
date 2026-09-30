import assert from 'node:assert/strict';
import { readFile } from 'node:fs/promises';
import test from 'node:test';

import {
  batchSizeFor,
  COLLECTION_READINESS_STATES,
  collectionReadinessFromDiagnostics,
  collectionFreshnessFromDiagnostics,
  normalizeCollectionReadinessState,
} from './sync-contract.js';

test('knowledge table replication pulls one byte-bounded document at a time', () => {
  assert.equal(batchSizeFor('knowledge_tables'), 1);
  assert.equal(batchSizeFor('desktop_file_chunks'), 6);
  assert.equal(batchSizeFor('research_runs'), 20);
});

test('collection readiness normalization accepts only canonical states', () => {
  for (const readinessState of COLLECTION_READINESS_STATES) {
    assert.equal(normalizeCollectionReadinessState(readinessState), readinessState);
  }
  for (const value of [null, undefined, '', 'complete', 'LIVE', 1]) {
    assert.equal(normalizeCollectionReadinessState(value), null);
  }
});

test('collection readiness follows the canonical diagnostics derivation table', () => {
  const updatedAt = '2026-07-27T10:15:00.000Z';
  const cases = [
    {
      name: 'non-WebRTC mode treats the local database as authoritative',
      entry: null,
      syncMode: 'local',
      expected: { state: 'live', ready: true, syncing: false, updatedAt: null },
    },
    {
      name: 'missing bridge report means catch-up is still starting',
      entry: null,
      syncMode: 'webrtc',
      expected: { state: 'catching-up', ready: false, syncing: true, updatedAt: null },
    },
    ...COLLECTION_READINESS_STATES.map((readinessState) => ({
      name: `frame transport state ${readinessState} is authoritative`,
      entry: {
        frameTransport: { collectionReadinessState: readinessState },
        initialReplicationState: 'complete',
        updatedAt,
      },
      syncMode: 'webrtc',
      expected: {
        state: readinessState,
        ready: readinessState === 'live',
        syncing: readinessState === 'catching-up' || readinessState === 'never-synced',
        updatedAt,
      },
    })),
    {
      name: 'legacy complete replication falls back to live',
      entry: { initialReplicationState: 'complete', updatedAt },
      syncMode: 'webrtc',
      expected: { state: 'live', ready: true, syncing: false, updatedAt },
    },
    {
      name: 'legacy failed replication falls back to offline pending',
      entry: { initialReplicationState: 'failed', updatedAt },
      syncMode: 'webrtc',
      expected: { state: 'offline-pending', ready: false, syncing: false, updatedAt },
    },
    {
      name: 'legacy unsupported replication falls back to offline pending',
      entry: { initialReplicationState: 'unsupported', updatedAt },
      syncMode: 'webrtc',
      expected: { state: 'offline-pending', ready: false, syncing: false, updatedAt },
    },
    {
      name: 'unknown transport and legacy states fall back to catching up',
      entry: {
        frameTransport: { collectionReadinessState: 'unknown' },
        initialReplicationState: 'pending',
        updatedAt,
      },
      syncMode: 'webrtc',
      expected: { state: 'catching-up', ready: false, syncing: true, updatedAt },
    },
  ];

  for (const { name, entry, syncMode, expected } of cases) {
    const snapshot = collectionReadinessFromDiagnostics('research_runs', entry, { syncMode });
    assert.deepEqual(snapshot, { collection: 'research_runs', ...expected }, name);
    assert.equal(Object.isFrozen(snapshot), true, `${name}: snapshot must be frozen`);
  }
});

test('current pull freshness is distinct from historical live cache readiness', () => {
  for (const freshness of ['catching-up', 'offline-pending']) {
    const snapshot = collectionFreshnessFromDiagnostics('outbound_leads', {
      initialReplicationState: 'complete',
      frameTransport: { collectionReadinessState: 'live', collectionFreshnessState: freshness },
    }, { syncMode: 'webrtc' });
    assert.equal(snapshot.ready, false);
    assert.equal(snapshot.state, freshness);
  }
  assert.equal(collectionFreshnessFromDiagnostics('outbound_leads', {
    frameTransport: { collectionReadinessState: 'live', collectionFreshnessState: 'live', lastSuccessfulPullAtMs: 1000 },
  }, { syncMode: 'webrtc', nowMs: 1001 }).ready, true);
  assert.equal(collectionFreshnessFromDiagnostics('outbound_leads', {
    frameTransport: { collectionFreshnessState: 'live', lastSuccessfulPullAtMs: 1000 },
  }, { syncMode: 'webrtc', nowMs: 121001 }).ready, false, 'suspended or stalled confirmation expires');
  assert.equal(collectionFreshnessFromDiagnostics('outbound_leads', {
    initialReplicationState: 'complete', frameTransport: { collectionReadinessState: 'live' },
  }, { syncMode: 'webrtc' }).ready, false, 'old runtime history alone is unconfirmed');
  assert.equal(collectionFreshnessFromDiagnostics('query_only', {
    frameTransport: { pullEnabled: false },
  }, { syncMode: 'webrtc' }).requiresPullConfirmation, false, 'query-only data retains its demand-query contract');
});

test('sync runtime version-binds its nested sync contract import', async () => {
  const source = await readFile(new URL('./sync.js', import.meta.url), 'utf8');
  assert.match(
    source,
    /from '\.\/sync-contract\.js\?v=[^']+'/,
    'mutable nested sync contract imports must not use an unversioned CDN URL',
  );
});

test('sync transport diagnostics preserve canonical collection readiness', async () => {
  const source = await readFile(new URL('./sync.js', import.meta.url), 'utf8');
  assert.match(
    source,
    /collectionReadinessState:\s*normalizeCollectionReadinessState\(status\.collectionReadinessState\)/,
  );
  assert.match(source, /firstPullCompletedAtMs:\s*numberField\('firstPullCompletedAtMs'\)/);
});

test('slow collection startup remains pending instead of dereferencing an empty timeout result', async () => {
  const source = await readFile(new URL('./sync.js', import.meta.url), 'utf8');
  assert.match(source, /if \(!bridge\) \{/);
  assert.match(source, /reason: 'startup-in-progress'/);
  assert.match(source, /return pendingBridge;/);
  assert.match(
    source,
    /const currentBridge = await withTimeout\(currentBridgePromise, 3000\);[\s\S]*?if \(!currentBridge\) \{[\s\S]*?createPendingCollectionBridge\(collection, currentBridgePromise\)[\s\S]*?return pendingBridge;/,
    'a repeated acquisition of the same slow bridge must return a bounded pending handle',
  );
  assert.doesNotMatch(
    source,
    /return bridges\.get\(collection\);/,
    'an async start must never adopt an unresolved cached bridge promise as its return value',
  );
});
