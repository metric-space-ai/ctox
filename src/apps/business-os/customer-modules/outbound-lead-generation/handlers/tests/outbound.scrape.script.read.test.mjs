// Strang OLG725: outbound.scrape.script.read – OLG liest den Skripttext mit seinen EIGENEN Dateien (files.get) und
// liefert ihn als Befehlsergebnis an den Rückruf der App outbound (commands_in from:['outbound'], Kern §16.47 Nr. 4).
import test from 'node:test';
import assert from 'node:assert/strict';
import { cmd, rejects } from './_ctx.mjs';
import { scrapeCtx, seedTarget } from './_scrape.mjs';
import { registerScript } from '../_lib/scrape-registry.js';
import handle, { SCRIPT_READ_MAX_BYTES } from '../outbound.scrape.script.read.js';

const read = (payload) => cmd('outbound.scrape.script.read', payload);

test('liefert die höchste Revision aus der eigenen Registry, Text über files.get der eigenen App', async () => {
  const { t } = scrapeCtx();
  await seedTarget(t.ctx, { target_key: 'dnb', script: 'console.log(1);\n' });
  await registerScript(t.ctx, { target_key: 'dnb', script_text: 'console.log(2);\n' });
  t.calls.caps.length = 0;
  const r = await handle(read({ target_key: 'dnb' }), t.ctx);
  assert.equal(r.ok, true);
  assert.equal(r.schema, 'ctox.outbound_lead_generation.scrape_script.v1');
  assert.equal(r.script.target_key, 'dnb');
  assert.equal(r.script.available, true);
  assert.equal(r.script.revision_no, 2);
  assert.equal(r.script.text, 'console.log(2);\n');
  assert.equal(r.script.truncated, false);
  assert.deepEqual(t.calls.caps.map((c) => c.name), ['files.get']);
});

test('max_bytes kürzt, ist aber auf 150 KB gedeckelt (Rust :1420)', async () => {
  const { t } = scrapeCtx();
  const gross = `// ${'x'.repeat(SCRIPT_READ_MAX_BYTES + 100)}\n`;
  await seedTarget(t.ctx, { target_key: 'big', script: gross });
  const klein = await handle(read({ target_key: 'big', max_bytes: 10 }), t.ctx);
  assert.equal(klein.script.truncated, true);
  assert.ok(klein.script.text.startsWith('// xxxxxxx'));
  assert.ok(klein.script.text.length < 80);
  const ueber = await handle(read({ target_key: 'big', max_bytes: 10_000_000 }), t.ctx);
  assert.equal(ueber.script.truncated, true, 'größerer Wunsch hebt die Obergrenze nicht auf');
});

test('ohne target_key invalid, unbekanntes Ziel not_found, Skript ohne Revision available:false', async () => {
  const { t } = scrapeCtx();
  assert.equal((await rejects(handle(read({}), t.ctx))).code, 'invalid');
  const e = await rejects(handle(read({ target_key: 'nope' }), t.ctx));
  assert.equal(e.code, 'not_found');
  await seedTarget(t.ctx, { target_key: 'leer', script: null });
  const r = await handle(read({ target_key: 'leer' }), t.ctx);
  assert.deepEqual(r.script, { target_key: 'leer', available: false });
});
