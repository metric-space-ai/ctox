import { createSidecarWithMemoryBackend } from '../dist/ctox-rxdb-js.mjs';

let now = 5_000_000;
const sidecar = createSidecarWithMemoryBackend({ databaseName: 'gc', clock: () => now });

// Three windows: one fresh, one stale (8 days old).
await sidecar.upsertQueryWindow({ collection: 'c', queryFingerprint: 'fresh', offset: 0, limit: 100, documentIds: ['a'], complete: true });
now += 1000;
await sidecar.upsertQueryWindow({ collection: 'c', queryFingerprint: 'stale', offset: 0, limit: 100, documentIds: ['b'], complete: true });
// Fast-forward 8 days; stale window is now older than maxAgeMs (7 days).
now += 8 * 24 * 3600 * 1000;
// Touch fresh so it's still recent.
await sidecar.getQueryWindow(['c', 'fresh', 0, 100]);

const removed = await sidecar.runWindowGc({ maxAgeMs: 7 * 24 * 3600 * 1000 });
assert(removed === 1, `must remove 1 stale window (got ${removed})`);
assert(await sidecar.backend.getQueryWindow(['c', 'fresh', 0, 100].join('|')) !== null,
       'fresh window must survive');
assert(await sidecar.backend.getQueryWindow(['c', 'stale', 0, 100].join('|')) === null,
       'stale window must be gone');

console.log('ctox-rxdb-js projection + window GC smoke OK', { removed });

function assert(c, m) { if (!c) throw new Error(m); }
