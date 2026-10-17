// Handler laufen in einem leeren vm-Kontext (Näherung an die App-Laufzeit §16.9).
import test from 'node:test';
import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import { fileURLToPath } from 'node:url';

test('Handler in leerem vm-Kontext ausführbar (keine Host-Globals nötig)', () => {
  const script = fileURLToPath(new URL('./vm-smoke.mjs', import.meta.url));
  const r = spawnSync(process.execPath, ['--experimental-vm-modules', '--no-warnings', script], { encoding: 'utf8' });
  assert.equal(r.status, 0, r.stderr);
  const out = JSON.parse(r.stdout);
  assert.equal(out.status, 'approved');
  assert.equal(out.unread, 0);
  assert.ok(out.modules >= 10);
});
