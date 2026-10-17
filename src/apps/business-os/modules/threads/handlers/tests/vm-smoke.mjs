// Läuft mit --experimental-vm-modules (von vm.test.mjs gestartet): lädt alle Handler in einen
// leeren vm-Kontext (nur ES-Builtins, kein crypto/TextEncoder/process) und führt einen
// Ablauf note.create → mark_read → request → approve aus.
import vm from 'node:vm';
import { readFileSync } from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { makeCtx, cmd, USERS } from './_ctx.mjs';

const dir = path.dirname(path.dirname(fileURLToPath(import.meta.url)));
const SHARED_TEMPLATE = '../../../shared/prompt-template.js';
const context = vm.createContext({});
const cache = new Map();
async function load(file) {
  if (cache.has(file)) return cache.get(file);
  const mod = new vm.SourceTextModule(readFileSync(path.join(dir, file), 'utf8'), { context, identifier: file });
  cache.set(file, mod);
  await mod.link(async (spec) => {
    // ctox:shared/prompt-template.js ist der einzige freigegebene Shell-Baustein (Kern §16.30 Nr. 4).
    if (spec === 'ctox:shared/prompt-template.js') return load(SHARED_TEMPLATE);
    if (!/^\.\/[a-z_.]+\.js$/.test(spec)) throw new Error(`forbidden import ${spec}`);
    return load(spec.slice(2));
  });
  return mod;
}
async function handler(type) {
  const mod = await load(`${type}.js`);
  if (mod.status !== 'evaluated') await mod.evaluate();
  return mod.namespace.default;
}
const h = makeCtx({ users: { ...USERS, lead: { display_name: 'Lead' } } });
const BOB = { id: 'bob', role: 'user' };
await (await handler('threads.note.create'))(cmd('threads.note.create', { thread_id: 'vm', message_id: 'm', message_type: 'mention', body: 'hi', target_user_ids: ['alice'] }, BOB), h.ctx);
await (await handler('threads.notification.mark_read'))(cmd('threads.notification.mark_read', { notification_id: 'notif_m_alice' }), h.ctx);
const req = await (await handler('threads.ctox_approval.request'))(cmd('threads.ctox_approval.request', { thread_id: 'vm', prompt: 'p', reviewer_user_id: 'lead' }, BOB), h.ctx);
const v = h.doc('ctox_task_approval_requests', req.approval_request_id).updated_at_ms;
const out = await (await handler('threads.ctox_approval.approve'))(cmd('threads.ctox_approval.approve', { approval_request_id: req.approval_request_id, expected_updated_at_ms: v }, { id: 'lead', role: 'user' }), h.ctx);
for (const t of ['threads.thread.watch', 'threads.handoff.create', 'threads.ai.request', 'threads.notification.preferences.update',
  'threads.ai.complete', 'threads.ai.failed', 'threads.ctox_approval.complete', 'threads.ctox_approval.failed']) await handler(t);
// Rückruf des freigegebenen Agenten-Tasks im vm-Kontext (Kern §16.30).
const [{ task_id: approvedTask }] = h.sim.enqueued;
await (await handler('threads.ctox_approval.complete'))({ id: 'cb1', type: 'threads.ctox_approval.complete', app_id: 'threads', module: 'threads', payload: { task_id: approvedTask, status: 'completed', result: { text: 'fertig' } } }, h.ctx);
if (h.doc('ctox_task_approval_requests', req.approval_request_id).execution_status !== 'completed') throw new Error('approval callback in vm failed');
const projectCtox = await handler('_projection.ctox_relevance');
const projectApp = await handler('_projection.app_relevance');
await h.ctx.put('business_commands', { id: 'c1', command_id: 'c1', module: 'support', record_id: 'case-1', status: 'completed', client_context: { actor: { id: 'bob' } }, updated_at_ms: 5 });
const pc = await projectCtox({ command_since_ms: 0, task_since_ms: 0 }, h.ctx);
const pa = await projectApp({ collection: 'notes', docs: [{ id: 'n1', title: 'N', owner_user_id: 'bob', status: 'done', updated_at_ms: 6 }] }, h.ctx);
if (!(pc.changed_count > 0 && pa.changed_count > 0)) throw new Error('projection in vm failed');
process.stdout.write(JSON.stringify({ status: out.status, unread: h.doc('user_thread_states', 'thread_state_alice_vm').unread_count, modules: cache.size }));
