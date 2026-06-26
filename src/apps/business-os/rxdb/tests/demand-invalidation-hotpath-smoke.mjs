import { readFileSync } from 'node:fs';
import { dirname, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

const testDir = dirname(fileURLToPath(import.meta.url));
const source = readFileSync(resolve(testDir, '../src/replication-webrtc.mjs'), 'utf8');

const pullInvalidations = source.match(/await this\.invalidateDemandCacheForRemoteWrite\(documents\);/g) || [];
const masterWriteInvalidations = source.match(/await this\.invalidateDemandCacheForRemoteWrite\(docs\);/g) || [];

assert(
  pullInvalidations.length === 1,
  `remote pull batches must invalidate demand cache once, got ${pullInvalidations.length}`,
);
assert(
  masterWriteInvalidations.length === 1,
  `masterWrite batches must invalidate demand cache once, got ${masterWriteInvalidations.length}`,
);

console.log('ctox-rxdb-js demand invalidation hot path smoke OK');

function assert(condition, message) {
  if (!condition) throw new Error(message);
}
