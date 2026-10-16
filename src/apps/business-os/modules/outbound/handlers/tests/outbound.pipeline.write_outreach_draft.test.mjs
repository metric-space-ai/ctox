import test from 'node:test';
import assert from 'node:assert/strict';
import { createTestCtx, cmd, rejects } from './_ctx.mjs';
import handle from '../outbound.pipeline.write_outreach_draft.js';

const item = () => ({ id: 'p1', contacts: [{ name: 'A', outreach_generating: true }, 'x'] });

test('writes message keys into contact and clears generating flag', async () => {
  const t = createTestCtx({ now: 5, data: { outbound_pipeline_items: { p1: item() } } });
  await handle(cmd('outbound.pipeline.write_outreach_draft', {
    pipeline_id: 'p1', contact_index: 0, messages: { message_mail_subject: 'S', message_mail_body: 'B', other: 'no', message_followup_1: 3 },
  }), t.ctx);
  const contact = t.doc('outbound_pipeline_items', 'p1').contacts[0];
  assert.deepEqual(contact.messages, { message_mail_subject: 'S', message_mail_body: 'B' });
  assert.equal(contact.outreach_generating, false);
  assert.equal(contact.outreach_status, 'drafted');
});

test('non-object contact replaced; range and messages validation', async () => {
  const t = createTestCtx({ data: { outbound_pipeline_items: { p1: item() } } });
  await handle(cmd('outbound.pipeline.write_outreach_draft', { pipeline_id: 'p1', contact_index: 1, messages: {} }), t.ctx);
  assert.equal(t.doc('outbound_pipeline_items', 'p1').contacts[1].outreach_status, 'drafted');
  assert.equal((await rejects(handle(cmd('outbound.pipeline.write_outreach_draft', { pipeline_id: 'p1', contact_index: 5, messages: {} }), t.ctx))).code, 'invalid');
  assert.equal((await rejects(handle(cmd('outbound.pipeline.write_outreach_draft', { pipeline_id: 'p1', contact_index: -1, messages: {} }), t.ctx))).code, 'invalid');
  assert.equal((await rejects(handle(cmd('outbound.pipeline.write_outreach_draft', { pipeline_id: 'p1' }), t.ctx))).code, 'invalid');
});
