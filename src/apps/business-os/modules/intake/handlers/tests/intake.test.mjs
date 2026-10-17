import test from 'node:test';
import assert from 'node:assert/strict';
import { createCtx, run } from './_ctx.mjs';

const NOW = 1_700_000_000_000;

test('intake.capture: Vorgaben, Dublettenschlüssel E-Mail vor Name, Prüfspur', async () => {
  const ctx = createCtx({ now: NOW });
  const out = await run(ctx, 'ats.intake.capture', { name: 'Ada L', email: ' ADA@Example.org ', vacancy_id: 'v1' });
  assert.deepEqual(out, { ok: true, application_id: `appl_${NOW}`, dedupe_key: 'email:ada@example.org' });
  const doc = ctx.$.doc('applications', `appl_${NOW}`);
  assert.equal(doc.channel, 'email');
  assert.equal(doc.status, 'new');
  assert.deepEqual(doc.candidate, { name: 'Ada L', email: 'ada@example.org', phone: '' });
  assert.equal(doc.received_at_ms, NOW);
  assert.deepEqual(ctx.$.capsTo('audit.event')[0].args[0].payload.summary,
    { application_id: `appl_${NOW}`, channel: 'email', dedupe_key: 'email:ada@example.org', has_email: true });
  const byName = await run(createCtx({ now: NOW }), 'ats.intake.capture', { name: 'Bob B', channel: 'phone' });
  assert.equal(byName.dedupe_key, 'name:bob b');
});
