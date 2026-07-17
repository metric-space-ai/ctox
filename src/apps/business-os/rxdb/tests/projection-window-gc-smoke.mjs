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

// SYNC-52 (a): window GC actually runs in production via the eviction
// scheduler's maintenance tick — not only from tests. `runSchedulerMaintenance`
// is exactly what the scheduler timer invokes.
{
  let t = 9_000_000;
  const s = createSidecarWithMemoryBackend({ databaseName: 'sched-gc', clock: () => t });
  await s.upsertQueryWindow({ collection: 'c', queryFingerprint: 'old', offset: 0, limit: 50, documentIds: ['a'], complete: true });
  t += 8 * 24 * 3600 * 1000; // age past the 7-day TTL
  const maint = await s.runSchedulerMaintenance();
  assert(maint.windowsReclaimed === 1, `scheduler maintenance must reclaim the aged window (got ${maint.windowsReclaimed})`);
  assert(await s.backend.getQueryWindow(['c', 'old', 0, 50].join('|')) === null, 'scheduler-run GC hard-deletes the aged window');
}

// SYNC-52 (b): a window that was invalidated but NEVER completed is a pure
// tombstone (no local-first value) and is hard-deleted on a SHORT grace,
// while an ever-completed window keeps the full TTL.
{
  let t = 20_000_000;
  const s = createSidecarWithMemoryBackend({ databaseName: 'tombstone-gc', clock: () => t });
  // never-completed window (minted incomplete)
  await s.upsertQueryWindow({ collection: 'c', queryFingerprint: 'never', offset: 0, limit: 50, documentIds: ['n'], complete: false });
  // ever-completed then invalidated window (tombstone that still serves local-first)
  await s.upsertQueryWindow({ collection: 'c', queryFingerprint: 'ever', offset: 0, limit: 50, documentIds: ['e'], complete: true });
  await s.invalidateQueryWindow(['c', 'ever', 0, 50]);
  t += 2 * 3600 * 1000; // 2h: past the 1h incomplete grace, well under the 7d TTL
  const reclaimed = await s.runWindowGc();
  assert(reclaimed === 1, `only the never-completed tombstone is reclaimed on the short grace (got ${reclaimed})`);
  assert(await s.backend.getQueryWindow(['c', 'never', 0, 50].join('|')) === null, 'never-completed tombstone hard-deleted');
  const everWindow = await s.backend.getQueryWindow(['c', 'ever', 0, 50].join('|'));
  assert(everWindow !== null && everWindow.everCompleted === true, 'ever-completed (local-first) window keeps the full TTL');
}

// SYNC-52 (c): change-invalidation routes through the collection_documentId
// ref index and must NOT full-scan the window store. Also proves the store
// stays bounded (aged windows reclaimed) across many distinct queries.
{
  let t = 30_000_000;
  const s = createSidecarWithMemoryBackend({ databaseName: 'no-scan', clock: () => t });
  // one simple-equality window (member) + one non-simple (sorted) window
  await s.upsertQueryWindow({ collection: 'tickets', queryFingerprint: 'open', offset: 0, limit: 50, documentIds: ['ticket-1'], complete: true, queryShape: { selector: { status: 'open' }, sort: [] } });
  await s.upsertQueryWindow({ collection: 'tickets', queryFingerprint: 'sorted', offset: 0, limit: 50, documentIds: ['ticket-2'], complete: true, queryShape: { selector: { status: 'closed' }, sort: [{ n: 'desc' }] } });

  let scans = 0;
  const originalScan = s.backend.scanQueryWindows.bind(s.backend);
  s.backend.scanQueryWindows = async () => { scans += 1; return originalScan(); };

  // (i) a NON-member doc newly matching the equality selector, plus the
  //     conservative non-simple invalidation — resolved via refs, no scan.
  const invalidated = await s.invalidateQueryWindowsForChanges('tickets', [{ id: 'ticket-99', status: 'open' }], 'id');
  assert(invalidated === 2, `newly-matching equality + conservative sorted window invalidate via refs (got ${invalidated})`);
  assert((await s.getQueryWindow(['tickets', 'open', 0, 50])).complete === false, 'equality window invalidated for newly-matching change');
  assert((await s.getQueryWindow(['tickets', 'sorted', 0, 50])).complete === false, 'sorted window conservatively invalidated');

  // (ii) a change matching NOTHING must invalidate only the non-simple window.
  await s.upsertQueryWindow({ collection: 'tickets', queryFingerprint: 'open', offset: 0, limit: 50, documentIds: ['ticket-1'], complete: true, queryShape: { selector: { status: 'open' }, sort: [] } });
  const invalidated2 = await s.invalidateQueryWindowsForChanges('tickets', [{ id: 'ticket-77', status: 'closed' }], 'id');
  assert(invalidated2 === 1, `only the conservative sorted window invalidates (got ${invalidated2})`);
  assert((await s.getQueryWindow(['tickets', 'open', 0, 50])).complete === true, 'equality window stays complete for non-matching change');

  assert(scans === 0, 'change-invalidation must not call scanQueryWindows (ref-index only)');

  // Boundedness across many distinct one-off queries + changes: aged windows
  // are reclaimed by the scheduler maintenance so the store does not grow
  // monotonically.
  for (let i = 0; i < 60; i += 1) {
    await s.upsertQueryWindow({ collection: 'tickets', queryFingerprint: `oneoff-${i}`, offset: 0, limit: 50, documentIds: [`d-${i}`], complete: true, queryShape: { selector: { status: `s-${i}` }, sort: [] } });
    await s.invalidateQueryWindowsForChanges('tickets', [{ id: `d-${i}`, status: `s-${i}` }], 'id');
  }
  t += 8 * 24 * 3600 * 1000; // age everything past the TTL
  await s.runSchedulerMaintenance();
  const surviving = await originalScan();
  assert(surviving.length === 0, `aged one-off windows are reclaimed (still have ${surviving.length})`);
}

console.log('ctox-rxdb-js projection + window GC smoke OK', { removed });

function assert(c, m) { if (!c) throw new Error(m); }
