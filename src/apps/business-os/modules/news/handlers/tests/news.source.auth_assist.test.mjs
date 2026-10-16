import assert from 'node:assert/strict';
import test from 'node:test';
import './_ctox-shared.mjs';
import { cmd, rejects } from './_ctx.mjs';
import { NOW, setup, source } from './_fixtures.mjs';

const { default: handle } = await import('../news.source.auth_assist.js');

test('Reddit-Quelle: Login-Hilfe anstossen, Quelle auth_requested mit Effekt', async () => {
  const requests = [];
  const t = await setup({
    seed: { news_sources: [source({ payload: { keep: 1 } })] },
    caps: { 'browser.auth_assist.request': async (req) => (requests.push(req), { request_id: 'aa_1', status: 'queued' }) },
  });
  const r = await handle(cmd('news.source.auth_assist', { source_id: 'source_1', target_url: 'https://evil.example' }, { id: 'cmd_x' }), t.ctx);
  assert.deepEqual(r, { ok: true, source_id: 'source_1', auth_assist: { request_id: 'aa_1', status: 'queued' } });
  assert.equal(requests[0].source_id, 'reddit.com');
  assert.equal(requests[0].target_url_override, null);
  assert.equal(requests[0].requesting_task_id, 'cmd_x');
  // CAP-3b #844: nur Vertragsfelder (deny_unknown_fields); Eigentümer bindet der Kern aus ctx.actor.
  assert.deepEqual(Object.keys(requests[0]).sort(), ['command_path', 'credential_ref', 'login_hint', 'preferred_auth_assist',
    'requesting_task_id', 'source_id', 'source_module', 'target_url_override']);
  const s = t.doc('news_sources', 'source_1');
  assert.equal(s.status, 'auth_requested');
  assert.equal(s.account_handle, '');
  assert.deepEqual(s.payload, { keep: 1, auth_assist: { request_id: 'aa_1', status: 'queued' } });
  assert.equal(s.updated_at_ms, NOW);
});

test('nur X/Reddit; unbekannte Quelle; ungueltige ID', async () => {
  const t = await setup({ seed: { news_sources: [source({ id: 'source_lab', kind: 'lab' })] } });
  await rejects(handle(cmd('news.source.auth_assist', { source_id: 'source_lab' }), t.ctx), 'invalid', /only X and Reddit/);
  await rejects(handle(cmd('news.source.auth_assist', { source_id: 'nope' }), t.ctx), 'not_found');
  await rejects(handle(cmd('news.source.auth_assist', { source_id: 'a/b' }), t.ctx), 'invalid');
});
