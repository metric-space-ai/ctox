'use strict';

// Explicit integration-test consumer, never installed in the product shell.
// Self-contained functions so Playwright can evaluate them in the page realm.
function ensureStartupFileConsumer(scope = globalThis) {
  const key = '__ctoxStartupFileConsumer';
  let consumer = scope[key];
  if (!consumer) {
    const app = scope.CTOX_BUSINESS_OS_APP || scope.ctoxBusinessOsSmoke?.state;
    if (typeof app?.sync?.leaseCollection !== 'function') return { phase: 'waiting', collections: [] };
    consumer = { phase: 'acquiring', leases: [], closed: false };
    scope[key] = consumer;
    consumer.pending = (async () => {
      for (const name of ['desktop_files', 'desktop_file_chunks']) {
        if (consumer.closed) return;
        const lease = await app.sync.leaseCollection(name, 'startup-file-consumer');
        if (consumer.closed) {
          await lease.release();
          return;
        }
        consumer.leases.push(lease);
      }
      consumer.phase = 'active';
    })().catch(() => {
      consumer.phase = 'failed';
    });
  }
  return {
    phase: consumer.phase,
    collections: consumer.leases.map(lease => lease.collection),
  };
}

async function releaseStartupFileConsumer(scope = globalThis) {
  const consumer = scope.__ctoxStartupFileConsumer;
  if (!consumer) return { released: 0, failed: 0 };
  consumer.closed = true;
  consumer.phase = 'closed';
  const leases = consumer.leases.splice(0);
  const results = await Promise.allSettled(leases.map(lease => lease.release()));
  // An acquisition already in flight releases its late lease above. The
  // fixture also closes its entire owned context in finally, bounding any
  // unresolved page operation without touching the user's browser.
  return {
    released: results.filter(result => result.status === 'fulfilled').length,
    failed: results.filter(result => result.status === 'rejected').length,
  };
}


// Preserve the original readiness evidence if cleanup also fails. The release
// race is bounded; closing the owned browser context is a separate final step.
async function finishStartupFileConsumer({ primaryError, release, close, timeoutMs = 2000 }) {
  const cleanupErrors = [];
  let timer;
  try {
    const cleanup = await Promise.race([
      Promise.resolve().then(release),
      new Promise((_, reject) => {
        timer = setTimeout(() => reject(new Error('release deadline')), timeoutMs);
      }),
    ]);
    if (cleanup.failed) cleanupErrors.push(new Error('file consumer lease release failed'));
  } catch {
    cleanupErrors.push(new Error('file consumer release rejected or exceeded its deadline'));
  } finally {
    clearTimeout(timer);
  }
  try {
    await close();
  } catch {
    cleanupErrors.push(new Error('owned browser context close failed'));
  }
  if (cleanupErrors.length) {
    throw new AggregateError(
      primaryError ? [primaryError, ...cleanupErrors] : cleanupErrors,
      primaryError?.message || 'startup file consumer cleanup failed',
      primaryError ? { cause: primaryError } : undefined,
    );
  }
}

module.exports = { ensureStartupFileConsumer, releaseStartupFileConsumer, finishStartupFileConsumer };
