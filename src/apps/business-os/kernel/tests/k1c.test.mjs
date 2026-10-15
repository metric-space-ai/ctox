// K-1c: Fake-Core mit zweitem Nutzer, Handler-Laufzeit, Trigger, Katalog-Primitiven, ctox.app.*, Aktivierungs-Zustand
// und Veröffentlichen ohne busy. Je Punkt ein Test; Mutationsbeweise in ~/.local/state/ctox-sync-v4/docs/befund-k-1b.md.
import { testLocks } from './web-locks.mjs';
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { createStubKernel } from '../kernel.mjs';

let n = 0;
async function mk(t, o = {}) {
  const k = createStubKernel({ locks: testLocks, instanceId: `k1c${process.pid}-${++n}`, userId: 'u1', latencyMs: 2, ...o });
  t.after(() => k.close());
  await k.ready;
  return k;
}
const tick = (ms = 0) => new Promise((r) => setTimeout(r, ms));
async function until(pred, ms = 4000, what = 'Bedingung') {
  const t0 = Date.now();
  while (!(await pred())) { if (Date.now() - t0 > ms) throw new Error(`until: Zeitüberschreitung (${what})`); await tick(5); }
}
const MOUNT = 'export function mount(){ return { unmount(){} }; }';
const ADMIN = { role: 'admin', permissions: ['apps.source.view', 'apps.source.edit', 'apps.modify', 'apps.install'] };
const USER = { role: 'user', permissions: [] };
const appVersion = (k, id) => k.apps.get().find((a) => a.id === id);
const cmdDone = async (h) => { await until(() => ['done', 'failed'].includes(h.status.get().status), 4000, 'Befehlsende'); return h.status.get(); };
// Zwei Nutzer derselben Instanz: ein Core, je Nutzer eine eigene Kernel-DB (§16.7).
async function twoUsers(t) {
  const instanceId = `k1c-inst-${process.pid}-${++n}`;
  const admin = await mk(t, { instanceId, coreNamespace: instanceId, userId: 'admin', actor: ADMIN });
  // Der Client behauptet Admin (wie v4/boot/boot.js); maßgeblich ist der Nutzer, den der Core kennt.
  const u2 = await mk(t, { instanceId, coreNamespace: instanceId, userId: 'u2', actor: ADMIN, coreUser: USER });
  return { admin, u2 };
}

test('K-1c 4 §16.39 Nr. 5: zweiter Nutzer ohne Admin – eigene DB, Rolle vom Core, Zeilen-/Collection-/Befehlsrechte wirksam', async (t) => {
  const { admin, u2 } = await twoUsers(t);
  assert.notEqual(admin.dbName, u2.dbName, 'eigene DB je (Instanz, Nutzer)');
  await admin.sim.publishAppVersion('crm', '1', {
    files: { 'index.js': MOUNT, 'handlers/crm.purge.js': 'export default async function handle() { return { purged: true }; }' },
    handlers: { 'crm.purge': 'handlers/crm.purge.js' },
    collections: {
      leads: { schema: { primaryKey: 'id', type: 'object', properties: { id: { type: 'string' }, name: { type: 'string' } }, required: ['id'] }, rights: { read: 'all', write: 'role:admin' } },
      notes: { schema: { primaryKey: 'id', type: 'object', properties: { id: { type: 'string' }, owner_id: { type: 'string' } }, required: ['id'] }, rights: { read: 'own', write: 'own' } },
    },
    manifestExtra: { command_rights: { 'crm.purge': { roles: ['admin'] } } },
  });
  await until(() => appVersion(u2, 'crm')?.state === 'activated', 4000, 'App beim zweiten Nutzer');
  const A = admin.appApi('crm', '1'), U = u2.appApi('crm', '1');
  // Collection-Recht write role:admin
  assert.equal((await A.put('leads', { id: 'l1', name: 'Ada' }).result).status, 'accepted');
  assert.deepEqual([(await U.put('leads', { id: 'l2', name: 'Eve' }).result).status, (await U.put('leads', { id: 'l3' }).result).reason], ['rejected', 'forbidden']);
  await until(() => U.query('leads', {}).get().length === 1, 4000, 'Lead beim zweiten Nutzer');
  // Zeilenrecht own: jeder sieht nur seine Notizen
  assert.equal((await U.put('notes', { id: 'n2', owner_id: 'u2' }).result).status, 'accepted');
  assert.equal((await A.put('notes', { id: 'n1', owner_id: 'admin' }).result).status, 'accepted');
  assert.equal((await U.put('notes', { id: 'n3', owner_id: 'admin' }).result).reason, 'forbidden', 'fremde Zeile schreiben');
  await tick(50);
  assert.deepEqual(U.query('notes', {}).get().map((d) => d.id), ['n2']);
  assert.deepEqual(A.query('notes', {}).get().map((d) => d.id), ['n1']);
  // Befehlsrecht command_rights: Admin darf, Nutzer nicht
  const ok = await cmdDone(A.command('crm.purge', {}));
  assert.deepEqual([ok.status, ok.result], ['done', { purged: true }]);
  const denied = U.command('crm.purge', {});
  assert.equal((await denied.result).reason, 'forbidden');
  assert.deepEqual(denied.status.get(), { status: 'failed', error: 'forbidden' });
  // Rolle kommt vom Core: der Nutzer darf keinen Quelltext veröffentlichen
  await u2.source('crm').draft.write('x.txt', 'x').catch(() => {});
  assert.equal((await u2.source('crm').draft.publish().result).reason, 'forbidden');
  // shell_preferences: die Zeile des Admins erreicht den zweiten Nutzer nicht
  await A.put('shell_preferences', { id: 'admin', pins: ['crm'] }).result;
  await tick(30);
  assert.equal((await u2.sim.inspect('shell_preferences', 'admin'))?.data ?? null, null);
});

const caps = ['audit.event', 'agent.task.enqueue'];
test('K-1c 5: Fake-Core reicht ctx.cap durch (audit.event, agent.task.enqueue) – App mit capabilities_required aktiviert', async (t) => {
  const k = await mk(t);
  await k.sim.publishAppVersion('probe', '1', {
    files: { 'index.js': MOUNT, 'handlers/p.audit.js': "export default async function handle(c, ctx) { return ctx.cap.audit.event({ event_type: 'p', record_id: c.payload.id }); }",
      'handlers/p.other.js': 'export default async function handle(c, ctx) { return ctx.cap.agent.task.get({ task_id: "x" }); }' },
    handlers: { 'p.audit': 'handlers/p.audit.js', 'p.other': 'handlers/p.other.js' }, capabilitiesRequired: caps,
  });
  const a = appVersion(k, 'probe');
  assert.deepEqual([a.state, a.error], ['activated', null], 'kein needs_update');
  const api = k.appApi('probe', '1');
  const r = await cmdDone(api.command('p.audit', { id: 'r1' }));
  assert.equal(r.status, 'done');
  const audit = await k.sim.coreRows('_audit');
  assert.deepEqual(audit.map((x) => [x.id, x.app_id, x.event_type, x.record_id]), [[r.result.id, 'probe', 'p', 'r1']]);
  const undeclared = await cmdDone(api.command('p.other', {}));
  assert.match(undeclared.error, /^forbidden: cap\.agent\.task\.get nicht in capabilities_required/);
});

test('K-1c 6: Handler-Laufzeit (Routing app_id+type, relative Importe, ctx.put/read/asset) und Trigger (on, every, fremd nur mit watch-Export)', async (t) => {
  const k = await mk(t, { everyMs: 40, agentDelayMs: 5 });
  const leads = { schema: { primaryKey: 'id', type: 'object', additionalProperties: false, properties: { id: { type: 'string' }, name: { type: 'string' }, n: { type: 'number' } }, required: ['id', 'name'] } };
  const log = { schema: { primaryKey: 'id', type: 'object', properties: { id: { type: 'string' }, src: { type: 'string' } }, required: ['id'] } };
  await k.sim.publishAppVersion('crm', '1', {
    files: {
      'index.js': MOUNT, 'prompts/hi.md': 'Hallo {{name}}',
      'handlers/_lib.js': "export const norm = (s) => String(s).trim().toUpperCase();",
      'handlers/crm.add.js': "import { norm } from './_lib.js';\nexport default async function handle(c, ctx) { const name = norm(c.payload.name) + ' ' + (await ctx.asset('prompts/hi.md')).slice(0, 5); await ctx.put('leads', { id: c.payload.id, name }); const all = await ctx.read('leads', { selector: {} }); return { id: c.payload.id, count: all.length }; }",
      'handlers/crm.bad.js': "export default async function handle(c, ctx) { await ctx.put('leads', { id: 'x' }); }",
      'handlers/crm.foreign.js': "export default async function handle(c, ctx) { await ctx.put('hr_people', { id: 'p' }); }",
      'handlers/on-lead.js': "export default async function handle(c, ctx) { await ctx.put('crm_log', { id: `lead:${c.payload.id}`, src: c.payload.op }); }",
      'handlers/tick.js': "export default async function handle(c, ctx) { await ctx.put('crm_log', { id: 'tick', src: 'every' }); }",
    },
    handlers: { 'crm.add': 'handlers/crm.add.js', 'crm.bad': 'handlers/crm.bad.js', 'crm.foreign': 'handlers/crm.foreign.js' },
    collections: { leads, crm_log: log },
    manifestExtra: { triggers: [{ handler: 'handlers/on-lead.js', on: { collection: 'leads', op: 'insert' } }, { handler: 'handlers/tick.js', every: 'PT1M' }], exports: { leads: ['watch'] } },
  });
  // zweite App beobachtet leads der ersten (fremd, §16.34 Nr. 3) – eine dritte ohne foreign-Deklaration nicht
  const watcher = (id, foreign) => k.sim.publishAppVersion(id, '1', {
    files: { 'index.js': MOUNT, 'handlers/w.js': `export default async function handle(c, ctx) { await ctx.put('${id}_seen', { id: c.payload.id }); }` },
    collections: { [`${id}_seen`]: { schema: { primaryKey: 'id', type: 'object', properties: { id: { type: 'string' } } } } },
    manifestExtra: { triggers: [{ handler: 'handlers/w.js', on: { collection: 'leads', op: 'insert' } }], ...(foreign ? { foreign_collections: { leads: { app: 'crm', ops: ['watch'] } } } : {}) },
  });
  await watcher('sales', true);
  await watcher('spy', false);
  const api = k.appApi('crm', '1');
  const r = await cmdDone(api.command('crm.add', { id: 'l1', name: ' ada ' }));
  assert.deepEqual([r.status, r.result], ['done', { id: 'l1', count: 1 }], JSON.stringify(r));
  await until(() => api.get('leads', 'l1').get()?.name === 'ADA Hallo', 4000, 'Handler-Write live im Client');
  // Schemafehler und fremde, nicht deklarierte Collection → failed; ohne Handler → no_handler (nie mehr {ok:true})
  assert.match((await cmdDone(api.command('crm.bad', {}))).error, /^invalid/);
  assert.match((await cmdDone(api.command('crm.foreign', {}))).error, /^forbidden/);
  assert.match((await cmdDone(api.command('crm.unknown', {}))).error, /^no_handler/);
  // on-Trigger der eigenen App und der fremd beobachtenden App; die nicht deklarierte bleibt still
  await until(async () => (await k.sim.coreRows('crm_log')).some((x) => x.id === 'lead:l1'), 4000, 'on-Trigger');
  await until(async () => (await k.sim.coreRows('sales_seen')).length === 1, 4000, 'fremder Trigger');
  await tick(50);
  assert.deepEqual(await k.sim.coreRows('spy_seen'), []);
  // every-Trigger (Takt im Test 40 ms statt PT1M)
  await until(async () => (await k.sim.coreRows('crm_log')).some((x) => x.id === 'tick'), 4000, 'every-Trigger');
});

test('K-1c 6b §16.30: agent.task.enqueue mit App-Skill → Fake-Harness → Rückruf genau einmal an die eigene App', async (t) => {
  // Der Fake-Harness braucht wie ein echter Lauf etwas Zeit; der Start-Handler schreibt seinen Lauf erst nach enqueue.
  const k = await mk(t, { agentDelayMs: 60 });
  const runs = { schema: { primaryKey: 'id', type: 'object', properties: { id: { type: 'string' }, task_id: { type: 'string' }, status: { type: 'string' }, n: { type: 'number' } }, required: ['id'] } };
  await k.sim.publishAppVersion('rep', '1', {
    files: {
      'index.js': MOUNT, 'skills/sum/SKILL.md': '# sum',
      'handlers/rep.start.js': "export default async function handle(c, ctx) { const { task_id } = await ctx.cap.agent.task.enqueue({ title: 't', instruction: 'i', skill: 'rep/sum', idempotency_key: c.payload.key, on_complete: 'rep.done', on_failed: 'rep.failed' }); await ctx.put('runs', { id: c.payload.key, task_id, status: 'queued', n: 0 }); return { task_id }; }",
      'handlers/rep.done.js': "export default async function handle(c, ctx) { const [r] = await ctx.read('runs', { selector: { task_id: c.payload.task_id } }); await ctx.put('runs', { ...r, status: 'done', n: r.n + 1 }); }",
      'handlers/rep.failed.js': "export default async function handle(c, ctx) { const [r] = await ctx.read('runs', { selector: { task_id: c.payload.task_id } }); await ctx.put('runs', { ...r, status: 'failed', n: r.n + 1 }); }",
    },
    handlers: { 'rep.start': 'handlers/rep.start.js', 'rep.done': 'handlers/rep.done.js', 'rep.failed': 'handlers/rep.failed.js' },
    collections: { runs }, capabilitiesRequired: caps, manifestExtra: { skills: [{ name: 'sum', path: 'skills/sum/SKILL.md' }] },
  });
  const api = k.appApi('rep', '1');
  const a = await cmdDone(api.command('rep.start', { key: 'k1' }));
  const b = await cmdDone(api.command('rep.start', { key: 'k1' })); // gleicher idempotency_key → gleicher Lauf
  assert.equal(a.result.task_id, b.result.task_id);
  await until(() => api.get('runs', 'k1').get()?.status === 'done', 4000, 'Rückruf');
  await tick(60);
  assert.equal(api.get('runs', 'k1').get().n, 1, 'genau ein Rückruf je task_id');
  k.sim.agentOutcome(() => ({ status: 'failed', result: 'kaputt' }));
  await cmdDone(api.command('rep.start', { key: 'k2' }));
  await until(() => api.get('runs', 'k2').get()?.status === 'failed', 4000, 'on_failed');
});
