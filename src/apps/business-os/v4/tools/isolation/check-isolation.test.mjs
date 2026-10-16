// node --test src/apps/business-os/v4/tools/isolation/check-isolation.test.mjs
import test from 'node:test';
import assert from 'node:assert/strict';
import path from 'node:path';
import { spawnSync } from 'node:child_process';
import { fileURLToPath } from 'node:url';
import { scan, classifySpecifier, inventory, loadLibs } from './check-isolation.mjs';

const HERE = path.dirname(fileURLToPath(import.meta.url));
const TREE = path.join(HERE, 'fixtures', 'tree');
const LIBS = path.join(HERE, 'fixtures', 'standard-libs.json');
const CLI = path.join(HERE, 'check-isolation.mjs');
const res = scan({ root: TREE, libsFile: LIBS });
const at = (rule, file) => res.hits.filter((h) => h.rule === rule && h.file === file).map((h) => h.line);

test('R1: statische, mehrzeilige, export-from, dynamische und URL-Pfade aus dem App-Ordner heraus', () => {
  assert.deepEqual(at('R1', 'modules/alpha/index.js'), [4, 5, 8, 9, 10, 11, 14, 15, 17, 19]);
});

test('R1: erlaubt sind eigener Ordner, ctox:shared/, freigegebene ctox:lib/, node: in handlers/, /api', () => {
  const lines = at('R1', 'modules/alpha/index.js');
  for (const ok of [1, 2, 3, 16, 18, 20, 21]) assert.ok(!lines.includes(ok), `Zeile ${ok} darf nicht treffen`);
  assert.deepEqual(at('R1', 'modules/alpha/sub/inner.js'), []);
  assert.deepEqual(at('R1', 'modules/alpha/handlers/h.js'), []);
});

test('R1: Kommentare und isolation-allow zaehlen nicht', () => {
  const lines = at('R1', 'modules/alpha/index.js');
  assert.ok(!lines.includes(12) && !lines.includes(13), 'Kommentar-Importe');
  assert.ok(!lines.includes(22), 'isolation-allow');
});

test('R1: App-Vendor (<app>/vendor/**): nur echte Lade-Spezifizierer aus dem App-Ordner heraus, keine Build-Literale/nackten Namen', () => {
  assert.deepEqual(at('R1', 'modules/alpha/vendor/lib/bundle.mjs'), [6]);
});

test('R1: HTML <link>/<script src> und Inline-Modul, HTML-Kommentar nicht', () => {
  assert.deepEqual(at('R1', 'modules/alpha/index.html'), [3, 5, 7]);
});

test('R1: CSS url() aus dem Ordner heraus, Kommentar nicht', () => {
  assert.deepEqual(at('R1', 'modules/alpha/index.css'), [1]);
});

test('R1: Testdateien nur mit --include-tests', () => {
  assert.deepEqual(at('R1', 'modules/alpha/tests/alpha.test.mjs'), []);
  const all = scan({ root: TREE, libsFile: LIBS, includeTests: true });
  assert.equal(all.hits.filter((h) => h.file === 'modules/alpha/tests/alpha.test.mjs').length, 1);
});

test('R1: Einstufung einzelner Spezifizierer', () => {
  const inv = inventory(TREE);
  const libs = loadLibs(LIBS);
  const base = { kind: 'import', rel: 'modules/alpha/index.js', appDir: 'modules/alpha', appId: 'alpha', inv, libs };
  assert.equal(classifySpecifier('./a.js', base), null);
  assert.equal(classifySpecifier('ctox:shared/x.js', base), null);
  assert.equal(classifySpecifier('ctox:lib/approved@1/x.mjs', base), null);
  assert.match(classifySpecifier('ctox:lib/approved/x.mjs', base).why, /nicht freigegeben/);
  assert.match(classifySpecifier('../beta/x.js', base).why, /fremde App beta/);
  assert.match(classifySpecifier('https://cdn.example/x.mjs', base).why, /externer Import/);
  assert.match(classifySpecifier('/~a/beta@1/x.js', { ...base, kind: 'url' }).why, /fremden App/);
  assert.equal(classifySpecifier('/~a/alpha@1/x.js', { ...base, kind: 'url' }), null);
});

test('R2: Ops nicht exportiert, Schreib-Evidenz ohne write, Eigentuemer ohne Collection; Plattform frei', () => {
  const r2 = res.hits.filter((h) => h.rule === 'R2');
  assert.deepEqual(r2.map((h) => h.spec), ['beta_items', 'beta_log', 'beta_missing']);
  assert.deepEqual(r2.map((h) => h.line), [6, 7, 8]);
});

test('R3: App-ID, Artefakt-URL, App-Collection und data-app in Shell-Code; Kommentar, MIME, allow, lab, Tests nicht', () => {
  assert.deepEqual(at('R3', 'v4/shell/app-host.js'), [3, 4, 5]);
  assert.deepEqual(at('R3', 'v4/frame/frame.css'), [1]);
  assert.deepEqual(at('R3', 'v4/shell/lab/lab.js'), []);
  assert.deepEqual(at('R3', 'v4/frame/frame.test.mjs'), []);
  const lab = scan({ root: TREE, libsFile: LIBS, includeLab: true });
  assert.equal(lab.hits.filter((h) => h.file === 'v4/shell/lab/lab.js').length, 1);
});

test('R4: shared/ mit App-Collection und Import aus App-Ordner; mehrdeutige ID ctox nur als Pfad', () => {
  const r4 = res.hits.filter((h) => h.rule === 'R4' && h.file === 'shared/util.js');
  assert.deepEqual(r4.map((h) => h.line), [1, 2]);
  assert.match(r4[0].message, /laedt aus App-Ordner/);
});

test('CLI: Exit 1 bei Treffern, 0 bei leerem Filter-Ergebnis ohne Treffer, --json lesbar', () => {
  const bad = spawnSync(process.execPath, [CLI, '--root', TREE, '--libs', LIBS, '--summary'], { encoding: 'utf8' });
  assert.equal(bad.status, 1, bad.stderr);
  assert.match(bad.stdout, /FEHLER: \d+ Treffer/);
  const none = spawnSync(process.execPath, [CLI, '--root', TREE, '--libs', LIBS, '--app', 'gamma'], { encoding: 'utf8' });
  assert.equal(none.status, 0, none.stdout);
  const json = spawnSync(process.execPath, [CLI, '--root', TREE, '--libs', LIBS, '--json', '--rule', 'R2'], { encoding: 'utf8' });
  assert.equal(JSON.parse(json.stdout).counts.R2, 3);
  const usage = spawnSync(process.execPath, [CLI, '--bogus'], { encoding: 'utf8' });
  assert.equal(usage.status, 2);
});
