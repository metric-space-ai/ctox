// Vertragsprüfung: 23 Handler vorhanden, default-Export, keine verbotenen Globals, Fehlerform {code,message}.
import test from 'node:test';
import assert from 'node:assert/strict';
import { readdirSync, readFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import path from 'node:path';
import { makeCtx, cmd } from './_ctx.mjs';

const dir = path.dirname(path.dirname(fileURLToPath(import.meta.url)));
const TYPES = [
  'threads.note.create', 'threads.note.update', 'threads.note.delete', 'threads.message.create',
  'threads.thread.watch', 'threads.thread.unwatch', 'threads.thread.archive', 'threads.thread.snooze',
  'threads.thread.state.update', 'threads.thread.assign', 'threads.thread.claim', 'threads.handoff.create',
  'threads.ai.request', 'threads.ctox_approval.request', 'threads.ctox_approval.edit',
  'threads.ctox_approval.approve', 'threads.ctox_approval.reject', 'threads.ctox_approval.cancel',
  'threads.link.create', 'threads.link.remove', 'threads.notification.mark_read',
  'threads.notification.dismiss', 'threads.notification.preferences.update',
  // SKL-A2: Rückruf-Befehle der agent.task-Läufe (Kern §16.30)
  'threads.ai.complete', 'threads.ai.failed', 'threads.ctox_approval.complete', 'threads.ctox_approval.failed',
];
const CODES = ['invalid', 'forbidden', 'not_found', 'conflict', 'revoked', 'expired', 'quota', 'unavailable'];

test('alle 23 Befehlstypen (is_threads_command, threads.rs:152) + 4 Rückrufe haben einen Handler', () => {
  const files = readdirSync(dir).filter((f) => f.startsWith('threads.') && f.endsWith('.js')).map((f) => f.slice(0, -3)).sort();
  assert.deepEqual(files, [...TYPES].sort());
});

test('Handler-Quellen: kein Dateisystem/Netz/Prozess/Zufall/Uhr außerhalb ctx', () => {
  for (const f of readdirSync(dir).filter((x) => x.endsWith('.js'))) {
    const src = readFileSync(path.join(dir, f), 'utf8');
    assert.doesNotMatch(src, /\bDate\.now|Math\.random|\bprocess\.|\brequire\(|\bfetch\(|from 'node:|\bimport\(/, f);
    for (const m of src.matchAll(/from '([^']+)'/g)) assert.match(m[1], /^(\.\/[a-z_.]+\.js|ctox:shared\/prompt-template\.js)$/, `${f}: ${m[1]}`);
  }
});

test('jeder Handler wirft bei leerem Payload {code,message} mit gültigem Code', async () => {
  for (const type of TYPES) {
    const { default: handle } = await import(path.join(dir, `${type}.js`));
    const h = makeCtx({});
    try {
      await handle(cmd(type, {}, { id: 'alice', role: 'user' }), h.ctx);
      // preferences.update ist mit leerem Payload gültig (Rust: Standardwerte)
      assert.equal(type, 'threads.notification.preferences.update', `${type} hätte scheitern müssen`);
    } catch (error) {
      assert.ok(error && typeof error === 'object' && !(error instanceof Error), `${type}: kein Fehlerobjekt ${error}`);
      assert.ok(CODES.includes(error.code), `${type}: Code ${error.code}`);
      assert.equal(typeof error.message, 'string');
    }
  }
});

test('Ergebnis bleibt unter 1 MiB und ist JSON', async () => {
  const { default: handle } = await import(path.join(dir, 'threads.note.create.js'));
  const h = makeCtx({});
  const out = await handle(cmd('threads.note.create', { body: 'x'.repeat(10000), target_user_ids: ['bob'] }), h.ctx);
  const size = JSON.stringify(out).length;
  assert.ok(size < 1024 * 1024);
  assert.deepEqual(JSON.parse(JSON.stringify(out)), out);
});
