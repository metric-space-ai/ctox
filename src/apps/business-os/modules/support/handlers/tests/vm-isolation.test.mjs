// Prüft, dass die Handler in einem leeren vm-Kontext (nur ECMAScript-Builtins, kein process,
// kein crypto, kein TextEncoder) laufen – Annahme der App-Laufzeit §16.9.
// Läuft nur mit --experimental-vm-modules; sonst übersprungen.
import test from 'node:test';
import assert from 'node:assert/strict';
import vm from 'node:vm';
import { readFileSync } from 'node:fs';
import { dirname, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { makeCtx } from './_ctx.mjs';

const handlersDir = resolve(dirname(fileURLToPath(import.meta.url)), '..');

test('handlers run inside an empty vm context', { skip: typeof vm.SourceTextModule !== 'function' && 'needs --experimental-vm-modules' }, async () => {
  const context = vm.createContext({});
  const cache = new Map();
  async function load(file) {
    if (cache.has(file)) return cache.get(file);
    const mod = new vm.SourceTextModule(readFileSync(resolve(handlersDir, file), 'utf8'), { context, identifier: file });
    cache.set(file, mod);
    await mod.link((specifier) => load(specifier.replace(/^\.\//, '')));
    return mod;
  }
  assert.equal(vm.runInContext('typeof process + typeof crypto + typeof TextEncoder', context), 'undefinedundefinedundefined');
  const env = makeCtx();
  for (const [type, payload] of [
    ['support.conversation.open_from_thread', { conversation_id: 'c1', thread_key: 'mail:t1', search_text: 'Grüße' }],
    ['support.note.create', { conversation_id: 'c1', body: 'n' }],
    ['support.reporting.rebuild_rollups', {}],
  ]) {
    const mod = await load(`${type}.js`);
    await mod.evaluate();
    const result = await mod.namespace.default({ id: `cmd_vm_${type}`, type, payload, actor: { id: 'u', role: 'admin' }, app_id: 'support', app_ver: 't' }, env.ctx);
    assert.equal(result.ok, true, type);
  }
  assert.equal(env.count('support_notes'), 1);
});
