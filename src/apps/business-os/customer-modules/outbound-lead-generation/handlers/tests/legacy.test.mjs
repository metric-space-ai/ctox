// Legacy-Weiterleitung (_lib/legacy.js): seit OLG3 nutzt kein Handler dieser App sie mehr (Legacy 0);
// das Werkzeug bleibt für künftige Übergänge geprüft.
import test from 'node:test';
import assert from 'node:assert/strict';
import { createTestCtx, cmd, rejects } from './_ctx.mjs';
import { legacyForwarder } from '../_lib/legacy.js';

test('legacy forwarder passes the command unchanged to ctx.cap.legacy', async () => {
  const type = 'example.legacy_type';
  const handle = legacyForwarder(type);
  const t = createTestCtx({ caps: { [`legacy.${type}`]: async (c) => ({ ok: true, native: c.type, payload: c.payload }) } });
  const r = await handle(cmd(type, { a: 1 }, { id: 'cmd_x' }), t.ctx);
  assert.deepEqual(r, { ok: true, native: type, payload: { a: 1 } });
  assert.equal(t.calls.caps[0].args[0].id, 'cmd_x');
  assert.equal(t.calls.writes.length, 0);
  assert.equal((await rejects(handle(cmd(type, {}), createTestCtx().ctx))).code, 'unavailable');
});
