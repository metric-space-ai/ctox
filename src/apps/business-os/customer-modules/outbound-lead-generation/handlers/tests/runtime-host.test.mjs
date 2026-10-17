// Strang OLG725 (§16.48 Nr. 3): runtime/host.mjs – Host des Allowlist-Eintrags olg-scrape-js (node + runtime/host.mjs).
// Echter Kindprozess: Host-Dokument auf stdin, Ergebnis des erzeugten Skripts auf stdout.
import test from 'node:test';
import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import { createHash } from 'node:crypto';
import { mkdtempSync, readFileSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { hostInput } from '../_lib/scrape-runner.js';

const HOST = resolve(dirname(fileURLToPath(import.meta.url)), '../../runtime/host.mjs');
const sha = (s) => createHash('sha256').update(s.trim()).digest('hex');
function run(stdin) {
  const cwd = mkdtempSync(join(tmpdir(), 'olg-host-'));
  try {
    const r = spawnSync(process.execPath, [HOST], { cwd, input: stdin, encoding: 'utf8', env: { PATH: process.env.PATH, LANG: 'C' } });
    return { ...r, cwd };
  } finally {
    rmSync(cwd, { recursive: true, force: true });
  }
}
const doc = (script, env = {}, language = 'javascript') => hostInput(language, sha(script), script, env);

test('host CommonJS-Skript wie früher `node -`: require, env aus dem Dokument, Grundumgebung unberührt', () => {
  const script = "const path = require('node:path');\nconsole.log(JSON.stringify({ records: [{ k: process.env.CTOX_SCRAPE_TARGET_KEY, p: Boolean(process.env.PATH), lang: process.env.LANG, sep: path.sep }] }));\n";
  const r = run(doc(script, { CTOX_SCRAPE_TARGET_KEY: 'xing' }));
  assert.equal(r.status, 0, r.stderr);
  assert.deepEqual(JSON.parse(r.stdout), { records: [{ k: 'xing', p: true, lang: 'C', sep: '/' }] });
});

test('host ES-Modul-Skript (import/Top-Level-await) läuft als .mjs', () => {
  const script = "import { sep } from 'node:path';\nawait Promise.resolve();\nconsole.log(JSON.stringify({ sep }));\n";
  const r = run(doc(script));
  assert.equal(r.status, 0, r.stderr);
  assert.deepEqual(JSON.parse(r.stdout), { sep: '/' });
});

test('host lehnt ab: falsche SHA-256, fremdes Schema, PATH/CTOX_BIN/LANG im env, TypeScript über node; nichts auf stdout', () => {
  const script = 'console.log(1);\n';
  const cases = [
    [hostInput('javascript', 'f'.repeat(64), script, {}), /SHA-256/],
    [JSON.stringify({ schema: 'x', language: 'javascript', script, script_sha256: sha(script), env: {} }), /Schema/],
    [doc(script, { PATH: '/evil' }), /PATH nicht erlaubt/],
    [doc(script, { CTOX_BIN: '/evil' }), /CTOX_BIN nicht erlaubt/],
    [doc(script, { LANG: 'de' }), /LANG nicht erlaubt/],
    [doc(script, {}, 'typescript'), /olg-scrape-ts/],
    ['kein json', /kein gültiges JSON/],
  ];
  for (const [stdin, why] of cases) {
    const r = run(stdin);
    assert.equal(r.status, 1, String(why));
    assert.equal(r.stdout, '', String(why));
    assert.match(r.stderr, why);
  }
});

test('host.ts ist nur der tsx-Einstieg in dieselbe Logik', () => {
  const ts = readFileSync(resolve(dirname(HOST), 'host.ts'), 'utf8');
  assert.match(ts, /import \{ runHost \} from '\.\/host\.mjs';/);
  assert.match(ts, /runHost\(\{ extension: '\.ts' \}\)/);
});
