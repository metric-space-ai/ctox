import { createSidecarWithMemoryBackend } from '../dist/ctox-rxdb-js.mjs';

const sidecar = createSidecarWithMemoryBackend({ databaseName: 'q' });
await sidecar.setBudgetBytes(8192);
await sidecar.touchDocuments('c', ['a', 'b', 'c'], { estimatedBytes: 4096 });
await sidecar.recordEstimatedBytes(12288);

let attempts = 0;
const result = await sidecar.withQuotaRecovery(async () => {
  attempts += 1;
  if (attempts === 1) {
    const err = new Error('storage full');
    err.name = 'QuotaExceededError';
    throw err;
  }
  return 'OK';
});
assert(result === 'OK', `recovery returned ${result}`);
assert(attempts === 2, `must retry exactly once (got ${attempts})`);

// Eviction scheduler smoke.
let ranEviction = false;
const orig = sidecar.runEvictionIfOverBudget.bind(sidecar);
sidecar.runEvictionIfOverBudget = async () => { ranEviction = true; return orig(); };
const handle = sidecar.startEvictionScheduler({ intervalMs: 30 });
await new Promise((r) => setTimeout(r, 80));
handle.stop();
assert(ranEviction === true, 'scheduler must invoke eviction');

// Idempotent start
const h2 = sidecar.startEvictionScheduler({ intervalMs: 30 });
const h3 = sidecar.startEvictionScheduler({ intervalMs: 30 });
h2.stop();
h3.stop();

console.log('ctox-rxdb-js quota recovery + scheduler smoke OK');

function assert(c, m) { if (!c) throw new Error(m); }
