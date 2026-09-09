// A missing bundle builder must never count as a passed reproducibility check.
import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import { mkdtempSync, readdirSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { fileURLToPath } from 'node:url';

const fixture = mkdtempSync(join(tmpdir(), 'ctox-bundle-guard-failure-'));
try {
  const result = spawnSync(process.execPath, [
    fileURLToPath(new URL('./bundle-reproducible-smoke.mjs', import.meta.url)),
  ], {
    env: { ...process.env, PATH: fixture, TMPDIR: fixture, TMP: fixture, TEMP: fixture },
    encoding: 'utf8',
    timeout: 10_000,
  });
  assert.ifError(result.error);
  assert.equal(result.signal, null);
  assert.equal(result.status, 1, 'unavailable esbuild must fail the actual bundle guard');
  assert.match(result.stderr, /could not run pinned esbuild/);
  assert.doesNotMatch(result.stdout, /SKIPPED|guard OK/);
  assert.deepEqual(readdirSync(fixture), [], 'failed verification must clean its scratch directory');
} finally {
  rmSync(fixture, { recursive: true, force: true });
}
console.log('ctox-rxdb bundle guard failure smoke OK');
