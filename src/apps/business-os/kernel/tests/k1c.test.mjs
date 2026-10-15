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
