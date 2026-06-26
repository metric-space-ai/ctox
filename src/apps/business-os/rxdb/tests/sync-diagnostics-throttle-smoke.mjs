import { createSyncRuntime } from '../../shared/sync.js';

const emissions = [];
const runtime = createSyncRuntime({
  db: { mode: 'rxdb' },
  config: {
    transport: 'webrtc',
    sync_room: 'ctox-business-os:test:diagnostics-throttle',
    signaling_urls: ['ws://127.0.0.1:9/signaling'],
  },
  onDiagnostic: (diagnostics) => {
    emissions.push(diagnostics);
  },
});

assert(emissions.length === 1, `ready diagnostic should emit immediately (got ${emissions.length})`);

const collections = Array.from({ length: 80 }, (_, index) => `diagnostic_collection_${index}`);
await runtime.suspendCollections(collections, 'diagnostic-throttle-test');

assert(
  emissions.length === 1,
  `collection diagnostic burst must coalesce instead of emitting per collection (got ${emissions.length})`,
);

await delay(320);

assert(emissions.length === 2, `collection burst should emit one delayed snapshot (got ${emissions.length})`);
assert(
  Object.keys(emissions.at(-1)?.collections || {}).length === collections.length,
  'coalesced diagnostic snapshot must retain every collection update',
);

await runtime.stop();

assert(emissions.at(-1)?.phase === 'stopped', 'stop diagnostic should emit immediately');
assert(emissions.length === 3, `stop should not flush an extra stale timer (got ${emissions.length})`);

console.log('ctox-rxdb sync diagnostics throttle smoke OK');

function delay(ms) {
  return new Promise((resolve) => setTimeout(resolve, ms));
}

function assert(condition, message) {
  if (!condition) throw new Error(message);
}
